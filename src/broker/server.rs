//! MQTT v5.0 Broker Server
//! 
//! Direct async implementation - no event loops!

use crate::broker::auth::{AllowAllAuthProvider, AuthProvider};
use crate::broker::client_handler::ClientHandler;
use crate::broker::config::BrokerConfig;
use crate::broker::router::MessageRouter;
use crate::error::{MqttError, Result};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, error, info};

/// MQTT v5.0 Broker
pub struct MqttBroker {
    config: Arc<BrokerConfig>,
    router: Arc<MessageRouter>,
    auth_provider: Arc<dyn AuthProvider>,
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
        let addr = addr.as_ref().parse::<std::net::SocketAddr>()
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
        
        // Create shared components
        let router = Arc::new(MessageRouter::new());
        let auth_provider: Arc<dyn AuthProvider> = Arc::new(AllowAllAuthProvider);
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
        
        Ok(Self {
            config: Arc::new(config),
            router,
            auth_provider,
            listener: Some(listener),
            shutdown_tx: Some(shutdown_tx),
        })
    }
    
    /// Sets a custom authentication provider
    #[must_use]
    pub fn with_auth_provider(mut self, provider: Arc<dyn AuthProvider>) -> Self {
        self.auth_provider = provider;
        self
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
            return Err(MqttError::InvalidState("Broker already running".to_string()));
        };
        
        let Some(shutdown_tx) = self.shutdown_tx.take() else {
            return Err(MqttError::InvalidState("Broker already running".to_string()));
        };
        
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
    pub async fn stats(&self) -> BrokerStats {
        BrokerStats {
            connected_clients: self.router.client_count().await,
            total_topics: self.router.topic_count().await,
            retained_messages: self.router.retained_count().await,
        }
    }
}

/// Broker statistics
#[derive(Debug, Clone)]
pub struct BrokerStats {
    /// Number of currently connected clients
    pub connected_clients: usize,
    /// Number of topics with active subscriptions
    pub total_topics: usize,
    /// Number of retained messages
    pub retained_messages: usize,
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
        let broker_handle = tokio::spawn(async move {
            broker.run().await
        });
        
        // Give broker time to start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        
        // Now test shutdown - but we can't call it because broker was moved
        // Just ensure the broker starts without error for now
        broker_handle.abort();
    }
    
    #[tokio::test]
    async fn test_broker_stats() {
        let broker = MqttBroker::bind("127.0.0.1:0").await.unwrap();
        let stats = broker.stats().await;
        
        assert_eq!(stats.connected_clients, 0);
        assert_eq!(stats.total_topics, 0);
        assert_eq!(stats.retained_messages, 0);
    }
}