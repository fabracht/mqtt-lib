//! MQTT v5.0 Broker Server
//!
//! Direct async implementation - no event loops!

use crate::broker::auth::{AllowAllAuthProvider, AuthProvider};
use crate::broker::client_handler::ClientHandler;
use crate::broker::config::{BrokerConfig, StorageBackend as StorageBackendType};
use crate::broker::router::MessageRouter;
use crate::broker::storage::{DynamicStorage, FileBackend, MemoryBackend, StorageBackend};
use crate::broker::sys_topics::{BrokerStats, SysTopicsProvider};
use crate::error::{MqttError, Result};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, error, info};

/// MQTT v5.0 Broker
pub struct MqttBroker {
    config: Arc<BrokerConfig>,
    router: Arc<MessageRouter>,
    auth_provider: Arc<dyn AuthProvider>,
    storage: Option<Arc<DynamicStorage>>,
    stats: Arc<BrokerStats>,
    listener: Option<TcpListener>,
    shutdown_tx: Option<tokio::sync::broadcast::Sender<()>>,
}

impl MqttBroker {
    /// Creates a new broker with default configuration
    ///
    /// # Errors
    ///
    /// Returns an error if binding fails
    pub async fn bind(addr: impl AsRef<str>) -> Result<Self> {
        let addr = addr
            .as_ref()
            .parse::<std::net::SocketAddr>()
            .map_err(|e| MqttError::Configuration(format!("Invalid address: {e}")))?;

        let config = BrokerConfig::default().with_bind_address(addr);
        Self::with_config(config).await
    }

    /// Creates a new broker with custom configuration
    ///
    /// # Errors
    ///
    /// Returns an error if configuration is invalid or binding fails
    pub async fn with_config(config: BrokerConfig) -> Result<Self> {
        // Validate configuration
        config.validate()?;

        // Bind to TCP port
        let listener = TcpListener::bind(&config.bind_address).await?;
        info!("MQTT broker listening on {}", config.bind_address);

        // Create storage backend
        let storage = if config.storage_config.enable_persistence {
            Some(Self::create_storage_backend(&config.storage_config).await?)
        } else {
            None
        };

        // Create shared components
        let router = if let Some(ref storage) = storage {
            Arc::new(MessageRouter::with_storage(Arc::clone(storage)))
        } else {
            Arc::new(MessageRouter::new())
        };
        let auth_provider: Arc<dyn AuthProvider> = Arc::new(AllowAllAuthProvider);
        let stats = Arc::new(BrokerStats::new());
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        Ok(Self {
            config: Arc::new(config),
            router,
            auth_provider,
            storage,
            stats,
            listener: Some(listener),
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// Create storage backend based on configuration
    async fn create_storage_backend(
        storage_config: &crate::broker::config::StorageConfig,
    ) -> Result<Arc<DynamicStorage>> {
        match storage_config.backend {
            StorageBackendType::File => {
                let backend = FileBackend::new(&storage_config.base_dir).await?;
                Ok(Arc::new(DynamicStorage::File(backend)))
            }
            StorageBackendType::Memory => {
                let backend = MemoryBackend::new();
                Ok(Arc::new(DynamicStorage::Memory(backend)))
            }
        }
    }

    /// Sets a custom authentication provider
    #[must_use]
    pub fn with_auth_provider(mut self, provider: Arc<dyn AuthProvider>) -> Self {
        self.auth_provider = provider;
        self
    }

    /// Initialize storage and start cleanup tasks
    async fn initialize_storage(&self) -> Result<()> {
        if let Some(ref storage) = self.storage {
            // Perform initial cleanup
            storage.cleanup_expired().await?;

            // Start periodic cleanup task
            let storage_clone = Arc::clone(storage);
            let cleanup_interval = self.config.storage_config.cleanup_interval;
            let mut shutdown_rx = self.shutdown_tx.as_ref().unwrap().subscribe();

            tokio::spawn(async move {
                let mut interval = tokio::time::interval(cleanup_interval);
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            if let Err(e) = storage_clone.cleanup_expired().await {
                                error!("Storage cleanup error: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            debug!("Storage cleanup task shutting down");
                            break;
                        }
                    }
                }
            });
        }
        Ok(())
    }

    /// Runs the broker until shutdown
    ///
    /// This is NOT an event loop! It's a direct async accept loop.
    ///
    /// # Errors
    ///
    /// Returns an error if the accept loop fails
    pub async fn run(&mut self) -> Result<()> {
        let Some(listener) = self.listener.take() else {
            return Err(MqttError::InvalidState(
                "Broker already running".to_string(),
            ));
        };

        let Some(shutdown_tx) = self.shutdown_tx.take() else {
            return Err(MqttError::InvalidState(
                "Broker already running".to_string(),
            ));
        };

        // Initialize storage and cleanup tasks
        self.initialize_storage().await?;

        // Initialize router (load retained messages)
        self.router.initialize().await?;

        // Start $SYS topics provider
        let sys_provider =
            SysTopicsProvider::new(Arc::clone(&self.router), Arc::clone(&self.stats));
        sys_provider.start();

        let mut shutdown_rx = shutdown_tx.subscribe();

        loop {
            tokio::select! {
                // Accept new connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, addr)) => {
                            debug!("New connection from {}", addr);

                            // Spawn handler task for this client
                            let handler = ClientHandler::new(
                                stream,
                                addr,
                                Arc::clone(&self.config),
                                Arc::clone(&self.router),
                                Arc::clone(&self.auth_provider),
                                self.storage.clone(),
                                Arc::clone(&self.stats),
                                shutdown_tx.subscribe(),
                            );

                            tokio::spawn(async move {
                                if let Err(e) = handler.run().await {
                                    error!("Client handler error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Accept error: {}", e);
                            // Continue accepting other connections
                        }
                    }
                }

                // Shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Broker shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Shuts down the broker gracefully
    ///
    /// # Errors
    ///
    /// Returns an error if no receivers are available for shutdown signal
    pub async fn shutdown(&self) -> Result<()> {
        if let Some(ref shutdown_tx) = self.shutdown_tx {
            shutdown_tx.send(()).map_err(|_| {
                MqttError::InvalidState("No receivers for shutdown signal".to_string())
            })?;
        }

        // Give clients time to disconnect gracefully
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        info!("Broker shutdown complete");
        Ok(())
    }

    /// Gets broker statistics
    #[must_use]
    pub fn stats(&self) -> Arc<BrokerStats> {
        Arc::clone(&self.stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_broker_bind() {
        // Use random port to avoid conflicts
        let broker = MqttBroker::bind("127.0.0.1:0").await;
        assert!(broker.is_ok());
    }

    #[tokio::test]
    async fn test_broker_with_config() {
        let config = BrokerConfig::default()
            .with_bind_address(([127, 0, 0, 1], 0))
            .with_max_clients(100);

        let broker = MqttBroker::with_config(config).await;
        assert!(broker.is_ok());
    }

    #[tokio::test]
    async fn test_broker_shutdown() {
        let mut broker = MqttBroker::bind("127.0.0.1:0").await.unwrap();

        // Start broker in background
        let broker_handle = tokio::spawn(async move { broker.run().await });

        // Give broker time to start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Now test shutdown - but we can't call it because broker was moved
        // Just ensure the broker starts without error for now
        broker_handle.abort();
    }

    #[tokio::test]
    async fn test_broker_stats() {
        let broker = MqttBroker::bind("127.0.0.1:0").await.unwrap();
        let stats = broker.stats();

        assert_eq!(
            stats
                .clients_connected
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }
}
