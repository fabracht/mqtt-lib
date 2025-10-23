//! Bridge connection implementation
//!
//! Manages a single bridge connection to a remote broker using our existing
//! MqttClient implementation.

use crate::broker::bridge::{BridgeConfig, BridgeDirection, BridgeError, BridgeStats, Result};
use crate::broker::router::MessageRouter;
use crate::client::MqttClient;
use crate::packet::publish::PublishPacket;
use crate::types::ConnectOptions;
use crate::validation::topic_matches_filter;
use crate::QoS;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

/// A bridge connection to a remote broker
pub struct BridgeConnection {
    /// Bridge configuration
    config: BridgeConfig,
    /// MQTT client for remote connection
    client: Arc<MqttClient>,
    /// Local message router
    router: Arc<MessageRouter>,
    /// Bridge statistics
    stats: Arc<RwLock<BridgeStats>>,
    /// Whether the bridge is running
    running: Arc<AtomicBool>,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
    /// Message counters for stats
    messages_sent: Arc<AtomicU64>,
    messages_received: Arc<AtomicU64>,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
}

impl BridgeConnection {
    /// Creates a new bridge connection
    pub fn new(config: BridgeConfig, router: Arc<MessageRouter>) -> Result<Self> {
        // Validate configuration
        config
            .validate()
            .map_err(|e| BridgeError::ConfigurationError(e.to_string()))?;

        // Create MQTT client for bridge
        let client = Arc::new(MqttClient::new(&config.client_id));

        // Create shutdown channel
        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            config,
            client,
            router,
            stats: Arc::new(RwLock::new(BridgeStats::default())),
            running: Arc::new(AtomicBool::new(false)),
            shutdown_tx,
            messages_sent: Arc::new(AtomicU64::new(0)),
            messages_received: Arc::new(AtomicU64::new(0)),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Starts the bridge connection
    pub async fn start(&self) -> Result<()> {
        if self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        self.running.store(true, Ordering::Relaxed);
        info!("Starting bridge '{}'", self.config.name);

        // Connect to remote broker
        Box::pin(self.connect()).await?;

        // Set up subscriptions for incoming topics
        self.setup_subscriptions().await?;

        Ok(())
    }

    /// Stops the bridge connection
    pub async fn stop(&self) -> Result<()> {
        if !self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        info!("Stopping bridge '{}'", self.config.name);
        self.running.store(false, Ordering::Relaxed);

        // Send shutdown signal
        let _ = self.shutdown_tx.send(());

        // Disconnect from remote broker (ignore errors if not connected)
        let _ = self.client.disconnect().await;

        // Update stats
        let mut stats = self.stats.write().await;
        stats.connected = false;
        stats.connected_since = None;

        Ok(())
    }

    /// Connects to the remote broker with failover support
    async fn connect(&self) -> Result<()> {
        let mut stats = self.stats.write().await;
        stats.connection_attempts += 1;
        drop(stats);

        // Build connection options
        let mut options = ConnectOptions::new(&self.config.client_id);
        options.clean_start = self.config.clean_start;
        options.keep_alive = Duration::from_secs(self.config.keepalive as u64);

        if let Some(ref username) = self.config.username {
            options.username = Some(username.clone());
        }
        if let Some(ref password) = self.config.password {
            options.password = Some(password.clone().into_bytes());
        }

        // Build connection string with TLS if needed
        let connection_string = if self.config.use_tls {
            format!("mqtts://{}", self.config.remote_address)
        } else {
            format!("mqtt://{}", self.config.remote_address)
        };

        // Try primary broker first
        match self
            .client
            .connect_with_options(&connection_string, options.clone())
            .await
        {
            Ok(_) => {
                info!(
                    "Bridge '{}' connected to primary broker: {}",
                    self.config.name, self.config.remote_address
                );
                self.update_connected_stats().await;
                return Ok(());
            }
            Err(e) => {
                warn!("Failed to connect to primary broker: {}", e);
                self.update_error_stats(e.to_string()).await;
            }
        }

        // Try backup brokers
        for backup in &self.config.backup_brokers {
            let backup_connection_string = if self.config.use_tls {
                format!("mqtts://{}", backup)
            } else {
                format!("mqtt://{}", backup)
            };
            match self
                .client
                .connect_with_options(&backup_connection_string, options.clone())
                .await
            {
                Ok(_) => {
                    info!(
                        "Bridge '{}' connected to backup broker: {}",
                        self.config.name, backup
                    );
                    self.update_connected_stats().await;
                    return Ok(());
                }
                Err(e) => {
                    warn!("Failed to connect to backup broker {}: {}", backup, e);
                    self.update_error_stats(e.to_string()).await;
                }
            }
        }

        Err(BridgeError::ConnectionFailed(
            "Failed to connect to any broker".to_string(),
        ))
    }

    /// Sets up subscriptions for incoming topics
    async fn setup_subscriptions(&self) -> Result<()> {
        for mapping in &self.config.topics {
            match mapping.direction {
                BridgeDirection::In | BridgeDirection::Both => {
                    let remote_topic = self.apply_remote_prefix(&mapping.pattern);

                    // Subscribe with a callback that forwards to local broker
                    let router = self.router.clone();
                    let local_prefix = mapping.local_prefix.clone();
                    let stats_received = self.messages_received.clone();
                    let stats_bytes = self.bytes_received.clone();

                    self.client
                        .subscribe(&remote_topic, move |msg| {
                            let router = router.clone();
                            let local_prefix = local_prefix.clone();
                            let stats_received = stats_received.clone();
                            let stats_bytes = stats_bytes.clone();

                            // Update stats
                            stats_received.fetch_add(1, Ordering::Relaxed);
                            stats_bytes.fetch_add(msg.payload.len() as u64, Ordering::Relaxed);

                            // Apply local prefix if configured
                            let local_topic = if let Some(ref prefix) = local_prefix {
                                format!("{}{}", prefix, msg.topic)
                            } else {
                                msg.topic.clone()
                            };

                            // Create packet for local routing
                            let packet =
                                PublishPacket::new(local_topic, msg.payload.clone(), msg.qos);

                            // Forward to local broker
                            tokio::spawn(async move {
                                router.route_message(&packet, None).await;
                            });
                        })
                        .await?;

                    info!(
                        "Bridge '{}' subscribed to remote topic: {} (QoS: {:?})",
                        self.config.name, remote_topic, mapping.qos
                    );
                }
                BridgeDirection::Out => {
                    // No subscription needed for outgoing only
                }
            }
        }

        Ok(())
    }

    /// Forwards a message to the remote broker
    pub async fn forward_message(&self, packet: &PublishPacket) -> Result<()> {
        if !self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Check which topic mappings match
        for mapping in &self.config.topics {
            match mapping.direction {
                BridgeDirection::Out | BridgeDirection::Both => {
                    if topic_matches_filter(&packet.topic_name, &mapping.pattern) {
                        // Apply remote prefix if configured
                        let remote_topic = if let Some(ref prefix) = mapping.remote_prefix {
                            format!("{}{}", prefix, packet.topic_name)
                        } else {
                            packet.topic_name.clone()
                        };

                        // Forward with configured QoS (may be different from original)
                        let result = match mapping.qos {
                            QoS::AtMostOnce => {
                                self.client
                                    .publish(&remote_topic, packet.payload.clone())
                                    .await
                            }
                            QoS::AtLeastOnce => {
                                self.client
                                    .publish_qos1(&remote_topic, packet.payload.clone())
                                    .await
                            }
                            QoS::ExactlyOnce => {
                                self.client
                                    .publish_qos2(&remote_topic, packet.payload.clone())
                                    .await
                            }
                        };

                        match result {
                            Ok(_) => {
                                debug!("Forwarded message to remote topic: {}", remote_topic);
                                self.messages_sent.fetch_add(1, Ordering::Relaxed);
                                self.bytes_sent
                                    .fetch_add(packet.payload.len() as u64, Ordering::Relaxed);
                            }
                            Err(e) => {
                                error!("Failed to forward message: {}", e);
                                return Err(BridgeError::ClientError(e));
                            }
                        }

                        // Only forward once per message (first matching rule wins)
                        break;
                    }
                }
                BridgeDirection::In => {
                    // Skip incoming-only mappings
                }
            }
        }

        Ok(())
    }

    /// Applies remote prefix to a topic
    fn apply_remote_prefix(&self, topic: &str) -> String {
        // Find the mapping for this topic to get its remote prefix
        for mapping in &self.config.topics {
            if mapping.pattern == topic {
                if let Some(ref prefix) = mapping.remote_prefix {
                    return format!("{}{}", prefix, topic);
                }
            }
        }
        topic.to_string()
    }

    /// Updates stats when connected
    async fn update_connected_stats(&self) {
        let mut stats = self.stats.write().await;
        stats.connected = true;
        stats.connected_since = Some(Instant::now());
        stats.last_error = None;
    }

    /// Updates stats when an error occurs
    async fn update_error_stats(&self, error: String) {
        let mut stats = self.stats.write().await;
        stats.connected = false;
        stats.connected_since = None;
        stats.last_error = Some(error);
    }

    /// Gets current statistics
    pub async fn get_stats(&self) -> BridgeStats {
        let mut stats = self.stats.read().await.clone();
        stats.messages_sent = self.messages_sent.load(Ordering::Relaxed);
        stats.messages_received = self.messages_received.load(Ordering::Relaxed);
        stats.bytes_sent = self.bytes_sent.load(Ordering::Relaxed);
        stats.bytes_received = self.bytes_received.load(Ordering::Relaxed);
        stats
    }

    /// Runs the bridge connection with automatic reconnection
    pub async fn run(&self) -> Result<()> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let mut attempt = 0u32;

        while self.running.load(Ordering::Relaxed) {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Bridge '{}' received shutdown signal", self.config.name);
                    break;
                }
                _ = self.run_connection() => {
                    if !self.running.load(Ordering::Relaxed) {
                        break;
                    }

                    attempt += 1;

                    // Check max attempts
                    if let Some(max) = self.config.max_reconnect_attempts {
                        if attempt >= max {
                            error!("Bridge '{}' exceeded max reconnection attempts", self.config.name);
                            break;
                        }
                    }

                    warn!("Bridge '{}' disconnected, reconnecting in {:?} (attempt {})",
                        self.config.name, self.config.reconnect_delay, attempt);

                    // Wait before reconnecting
                    tokio::time::sleep(self.config.reconnect_delay).await;
                }
            }
        }

        Ok(())
    }

    /// Runs a single connection until disconnected
    async fn run_connection(&self) -> Result<()> {
        // Connect and setup subscriptions
        Box::pin(self.connect()).await?;
        self.setup_subscriptions().await?;

        // Wait for client disconnection
        // This is a simplified version - in production we'd monitor the client connection
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            // Check if still connected (simplified check)
            if !self.client.is_connected().await {
                warn!(
                    "Bridge '{}' disconnected from remote broker",
                    self.config.name
                );
                break;
            }

            if !self.running.load(Ordering::Relaxed) {
                break;
            }
        }

        Ok(())
    }
}
