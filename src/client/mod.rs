//! MQTT v5.0 Client - Direct Async Implementation
//!
//! CRITICAL: NO EVENT LOOPS
//! This client uses direct async/await patterns throughout.
//! We do NOT use event loops, command channels, or actor patterns.

use crate::callback::PublishCallback;
use crate::error::{MqttError, Result};
use crate::packet::publish::PublishPacket;
use crate::packet::subscribe::{SubscribePacket, TopicFilter};
use crate::packet::unsubscribe::UnsubscribePacket;
use crate::protocol::v5::properties::Properties;
use crate::transport::tcp::TcpConfig;
use crate::transport::tls::TlsConfig;
use crate::transport::{TcpTransport, TlsTransport, Transport, TransportType};
use crate::types::{
    ConnectOptions, ConnectResult, PublishOptions, PublishResult, SubscribeOptions,
};
use crate::QoS;
use std::future::Future;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::Duration;

mod connection;
mod direct;
mod error_recovery;
pub mod mock;
mod retry;
pub mod r#trait;

pub use self::connection::{ConnectionEvent, DisconnectReason, ReconnectConfig};
pub use self::error_recovery::{ErrorCallback, ErrorRecoveryConfig, RecoverableError, RetryState};
pub use self::mock::{MockCall, MockMqttClient};
pub use self::r#trait::MqttClientTrait;

use self::direct::DirectClientInner;

/// Type alias for connection event callback
pub type ConnectionEventCallback = Arc<dyn Fn(ConnectionEvent) + Send + Sync>;

/// Thread-safe MQTT v5.0 client
///
/// This client uses DIRECT async methods - NO event loops!
///
/// # Examples
///
/// ## Basic usage
///
/// ````no_run
/// use mqtt_v5::MqttClient;
///
/// #[tokio::main]
/// async fn main() -> `Result`<(), Box<dyn std::error::Error>> {
///     // Create a client with a unique ID
///     let client = MqttClient::new("my-client-id");
///     
///     // Connect to the broker
///     client.connect("mqtt://localhost:1883").await?;
///     
///     // Subscribe to a topic
///     client.subscribe("temperature/room1", |msg| {
///         println!("Received: {} on topic {}",
///                  String::from_utf8_lossy(&msg.payload),
///                  msg.topic);
///     }).await?;
///     
///     // Publish a message
///     client.publish("temperature/room1", b"22.5").await?;
///     
///     // Disconnect when done
///     client.disconnect().await?;
///     Ok(())
/// }
/// ```
///
/// ## Advanced usage with options
///
/// ````no_run
/// use mqtt_v5::{`MqttClient`, ConnectOptions, PublishOptions, `QoS`};
/// use std::time::Duration;
///
/// #[tokio::main]
/// async fn main() -> `Result`<(), Box<dyn std::error::Error>> {
///     // Create client with custom options
///     let options = ConnectOptions::new("my-client")
///         .with_clean_start(true)
///         .with_keep_alive(Duration::from_secs(30))
///         .with_credentials("user", b"pass");
///     
///     let client = MqttClient::with_options(options);
///     
///     // Connect with TLS
///     client.connect("mqtts://broker.example.com:8883").await?;
///     
///     // Publish with `QoS` 2 and retain flag
///     let mut pub_options = PublishOptions::default();
///     pub_options.qos = `QoS`::ExactlyOnce;
///     pub_options.retain = true;
///     
///     client.publish_with_options(
///         "status/device1",
///         b"online",
///         pub_options
///     ).await?;
///     
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct MqttClient {
    /// Shared inner state - uses direct async implementation
    inner: Arc<RwLock<DirectClientInner>>,
    /// Connection event callbacks
    connection_event_callbacks: Arc<RwLock<Vec<ConnectionEventCallback>>>,
    /// Error callbacks
    error_callbacks: Arc<RwLock<Vec<ErrorCallback>>>,
    /// Error recovery configuration
    error_recovery_config: Arc<RwLock<ErrorRecoveryConfig>>,
}

impl MqttClient {
    /// Creates a new MQTT client with default options
    ///
    /// # Examples
    ///
    /// ```
    /// use mqtt_v5::MqttClient;
    ///
    /// let client = MqttClient::new("my-device-001");
    /// ```
    pub fn new(client_id: impl Into<String>) -> Self {
        let options = ConnectOptions::new(client_id).with_clean_start(false); // Default to persistent session
        Self::with_options(options)
    }

    /// Creates a new MQTT client with custom options
    ///
    /// # Examples
    ///
    /// ```
    /// use mqtt_v5::{`MqttClient`, ConnectOptions};
    /// use std::time::Duration;
    ///
    /// let options = ConnectOptions::new("client-001")
    ///     .with_clean_start(true)
    ///     .with_keep_alive(Duration::from_secs(60))
    ///     .with_credentials("mqtt_user", b"secret");
    ///
    /// let client = MqttClient::with_options(options);
    /// ```
    #[must_use]
    pub fn with_options(options: ConnectOptions) -> Self {
        let inner = DirectClientInner::new(options);

        Self {
            inner: Arc::new(RwLock::new(inner)),
            connection_event_callbacks: Arc::new(RwLock::new(Vec::new())),
            error_callbacks: Arc::new(RwLock::new(Vec::new())),
            error_recovery_config: Arc::new(RwLock::new(ErrorRecoveryConfig::default())),
        }
    }

    /// Checks if the client is connected
    pub async fn is_connected(&self) -> bool {
        self.inner.read().await.is_connected()
    }

    /// Gets the client ID
    pub async fn client_id(&self) -> String {
        self.inner
            .read()
            .await
            .session
            .read()
            .await
            .client_id()
            .to_string()
    }

    /// Sets a callback for connection events
    ///
    /// # Examples
    ///
    /// ````no_run
    /// use mqtt_v5::{`MqttClient`, ConnectionEvent, DisconnectReason};
    ///
    /// # async fn example() -> `Result`<(), Box<dyn std::error::Error>> {
    /// let client = MqttClient::new("my-client");
    ///
    /// client.on_connection_event(|event| {
    ///     match event {
    ///         ConnectionEvent::Connected { `session_present` } => {
    ///             println!("Connected! Session present: {}", `session_present`);
    ///         }
    ///         ConnectionEvent::Disconnected { reason } => {
    ///             println!("Disconnected: {:?}", reason);
    ///         }
    ///         ConnectionEvent::Reconnecting { attempt } => {
    ///             println!("Reconnecting attempt {}", attempt);
    ///         }
    ///         ConnectionEvent::ReconnectFailed { error } => {
    ///             println!("Reconnection failed: {}", error);
    ///         }
    ///     }
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the callback storage is inaccessible
    #[allow(clippy::missing_errors_doc)]
    pub async fn on_connection_event<F>(&self, callback: F) -> Result<()>
    where
        F: Fn(ConnectionEvent) + Send + Sync + 'static,
    {
        let mut callbacks = self.connection_event_callbacks.write().await;
        callbacks.push(Arc::new(callback));
        Ok(())
    }

    /// Triggers a connection event to all registered callbacks
    async fn trigger_connection_event(&self, event: ConnectionEvent) {
        let callbacks = self.connection_event_callbacks.read().await.clone();
        for callback in callbacks {
            callback(event.clone());
        }
    }

    /// Sets an error callback
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn on_error<F>(&self, callback: F) -> Result<()>
    where
        F: Fn(&MqttError) + Send + Sync + 'static,
    {
        let mut callbacks = self.error_callbacks.write().await;
        callbacks.push(Box::new(callback));
        Ok(())
    }

    /// Connects to the MQTT broker with default options
    ///
    /// # Examples
    ///
    /// ````no_run
    /// # use mqtt_v5::MqttClient;
    /// # async fn example() -> `Result`<(), Box<dyn std::error::Error>> {
    /// let client = MqttClient::new("my-client");
    ///
    /// // Connect via TCP
    /// client.connect("mqtt://broker.example.com:1883").await?;
    ///
    /// // Or connect via TLS
    /// // client.connect("mqtts://secure.broker.com:8883").await?;
    ///
    /// // Or use just host:port (defaults to TCP)
    /// // client.connect("localhost:1883").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if connection fails, address is invalid, or transport cannot be established
    #[allow(clippy::missing_errors_doc)]
    pub async fn connect(&self, address: &str) -> Result<()> {
        let options = self.inner.read().await.options.clone();
        self.connect_with_options(address, options)
            .await
            .map(|_| ())
    }

    /// Connects to the MQTT broker with custom options
    ///
    /// This is a DIRECT async method - no event loops!
    /// Returns `session_present` flag from CONNACK
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn connect_with_options(
        &self,
        address: &str,
        options: ConnectOptions,
    ) -> Result<ConnectResult> {
        // Check if already connected
        if self.is_connected().await {
            return Err(MqttError::AlreadyConnected);
        }

        // Update the inner client with new options
        {
            let mut inner = self.inner.write().await;
            inner.options = options.clone();
            // Always store address for potential reconnection
            inner.last_address = Some(address.to_string());
        }

        // Try to connect
        let result = self.connect_internal(address).await;

        // Handle reconnection if enabled and initial connection fails
        if let Err(ref error) = result {
            if options.reconnect_config.enabled {
                // Trigger initial disconnection event
                self.trigger_connection_event(ConnectionEvent::Disconnected {
                    reason: DisconnectReason::NetworkError(error.to_string()),
                })
                .await;

                // Start reconnection attempts in background
                let client = self.clone();
                let address_clone = address.to_string();
                let config = options.reconnect_config.clone();
                tokio::spawn(async move {
                    if let Err(e) = client.attempt_reconnection(&address_clone, &config).await {
                        tracing::error!("All reconnection attempts failed: {}", e);
                    }
                });
            }
        } else if result.is_ok() && options.reconnect_config.enabled {
            // Start monitoring for future disconnections
            let client = self.clone();
            tokio::spawn(async move {
                client.monitor_connection().await;
            });
        }

        result
    }

    /// Internal connection method that does the actual connection work
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    async fn connect_internal(&self, address: &str) -> Result<ConnectResult> {
        // Parse address to determine transport type
        let (client_transport_type, host, port) = Self::parse_address(address)?;

        // Resolve address
        let addr_str = format!("{host}:{port}");
        let addr = addr_str
            .to_socket_addrs()
            .map_err(|e| MqttError::ConnectionError(format!("Failed to resolve address: {e}")))?
            .next()
            .ok_or_else(|| MqttError::ConnectionError("No valid address found".to_string()))?;

        // Create transport based on type
        let transport =
            match client_transport_type {
                ClientTransportType::Tcp => {
                    let config = TcpConfig::new(addr);
                    let mut tcp_transport = TcpTransport::new(config);

                    // Connect TCP transport directly
                    tcp_transport.connect().await.map_err(|e| {
                        MqttError::ConnectionError(format!("TCP connect failed: {e}"))
                    })?;

                    TransportType::Tcp(tcp_transport)
                }
                ClientTransportType::Tls => {
                    let config = TlsConfig::new(addr, host);
                    let mut tls_transport = TlsTransport::new(config);

                    // Connect TLS transport directly
                    tls_transport.connect().await.map_err(|e| {
                        MqttError::ConnectionError(format!("TLS connect failed: {e}"))
                    })?;

                    TransportType::Tls(Box::new(tls_transport))
                }
            };

        // Reset reconnect attempt counter
        {
            let mut inner = self.inner.write().await;
            inner.reconnect_attempt = 0;
        }

        // Connect using direct async method - NO event loop!
        let mut inner = self.inner.write().await;
        let result = inner.connect(transport).await?;

        // Get stored subscriptions before releasing the lock
        let stored_subs = inner.stored_subscriptions.read().await.clone();
        let session_present = result.session_present;
        drop(inner); // Release lock before potentially resubscribing

        // Trigger connected event
        self.trigger_connection_event(ConnectionEvent::Connected { session_present })
            .await;

        // Restore callbacks and subscriptions
        if !stored_subs.is_empty() {
            if session_present {
                // Session was resumed - broker has subscriptions, but we need to restore callbacks
                tracing::info!("Session resumed, restoring {} callbacks", stored_subs.len());
                let inner = self.inner.read().await;
                for (topic, _, callback_id) in stored_subs {
                    if let Err(e) = inner.callback_manager.restore_callback(callback_id).await {
                        tracing::warn!("Failed to restore callback for {}: {}", topic, e);
                    }
                }
            } else {
                // Session was not resumed - need to resubscribe and restore callbacks
                tracing::info!(
                    "Session not resumed, restoring {} subscriptions",
                    stored_subs.len()
                );
                for (topic, options, callback_id) in stored_subs {
                    if let Err(e) = self
                        .resubscribe_internal(&topic, options, callback_id)
                        .await
                    {
                        tracing::warn!("Failed to restore subscription to {}: {}", topic, e);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Disconnects from the MQTT broker
    ///
    /// This is a DIRECT async method - no event loops!
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn disconnect(&self) -> Result<()> {
        let mut inner = self.inner.write().await;
        inner.disconnect().await?;

        // Trigger disconnected event
        self.trigger_connection_event(ConnectionEvent::Disconnected {
            reason: DisconnectReason::ClientInitiated,
        })
        .await;

        Ok(())
    }

    /// Publishes a message to a topic
    ///
    /// # Examples
    ///
    /// ````no_run
    /// # use mqtt_v5::MqttClient;
    /// # async fn example() -> `Result`<(), Box<dyn std::error::Error>> {
    /// let client = MqttClient::new("my-client");
    /// client.connect("mqtt://localhost:1883").await?;
    ///
    /// // Publish a simple string message
    /// client.publish("sensors/temperature", "23.5°C").await?;
    ///
    /// // Publish binary data
    /// let data = vec![0x01, 0x02, 0x03, 0x04];
    /// client.publish("sensors/binary", data).await?;
    ///
    /// // Publish JSON
    /// let json = r#"{"temperature": 23.5, "humidity": 45}"#;
    /// client.publish("sensors/json", json).await?;
    /// # Ok(())
    /// # }
    /// ````
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn publish(
        &self,
        topic: impl Into<String>,
        payload: impl Into<Vec<u8>>,
    ) -> Result<PublishResult> {
        let options = PublishOptions::default();
        self.publish_with_options(topic, payload, options).await
    }

    /// Publishes a message to a topic with specific `QoS` (compatibility method)
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn publish_qos(
        &self,
        topic: impl Into<String>,
        payload: impl Into<Vec<u8>>,
        qos: QoS,
    ) -> Result<PublishResult> {
        let options = PublishOptions {
            qos,
            ..Default::default()
        };
        self.publish_with_options(topic, payload, options).await
    }

    /// Publishes a message with custom options
    ///
    /// This is a DIRECT async method - no event loops!
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn publish_with_options(
        &self,
        topic: impl Into<String>,
        payload: impl Into<Vec<u8>>,
        options: PublishOptions,
    ) -> Result<PublishResult> {
        let topic_str = topic.into();
        let payload_vec = payload.into();

        // Direct publish - no command channels!
        let inner = self.inner.read().await;
        inner.publish(topic_str, payload_vec, options).await
    }

    /// Subscribes to a topic with a callback
    ///
    /// This is a DIRECT async method - no event loops!
    ///
    /// # Examples
    ///
    /// ````no_run
    /// # use mqtt_v5::MqttClient;
    /// # async fn example() -> `Result`<(), Box<dyn std::error::Error>> {
    /// let client = MqttClient::new("my-client");
    /// client.connect("mqtt://localhost:1883").await?;
    ///
    /// // Subscribe to a specific topic
    /// client.subscribe("sensors/temperature", |msg| {
    ///     println!("Temperature: {}", String::from_utf8_lossy(&msg.payload));
    /// }).await?;
    ///
    /// // Subscribe with wildcards
    /// client.subscribe("sensors/+/status", |msg| {
    ///     println!("Status update on {}: {}", msg.topic,
    ///              String::from_utf8_lossy(&msg.payload));
    /// }).await?;
    ///
    /// // Subscribe to all topics under sensors/
    /// client.subscribe("sensors/#", |msg| {
    ///     println!("Sensor data: {} = {}", msg.topic,
    ///              String::from_utf8_lossy(&msg.payload));
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if subscription fails, topic filter is invalid, or client is not connected
    #[allow(clippy::missing_errors_doc)]
    pub async fn subscribe<F>(
        &self,
        topic_filter: impl Into<String>,
        callback: F,
    ) -> Result<(u16, QoS)>
    where
        F: Fn(crate::types::Message) + Send + Sync + 'static,
    {
        let options = SubscribeOptions::default();
        self.subscribe_with_options(topic_filter, options, callback)
            .await
    }

    /// Subscribes to a topic with custom options and a callback (using Message type)
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn subscribe_with_options<F>(
        &self,
        topic_filter: impl Into<String>,
        options: SubscribeOptions,
        callback: F,
    ) -> Result<(u16, QoS)>
    where
        F: Fn(crate::types::Message) + Send + Sync + 'static,
    {
        // Wrap the callback to convert PublishPacket to Message
        let wrapped_callback = move |packet: PublishPacket| {
            let msg = crate::types::Message::from(packet);
            callback(msg);
        };
        self.subscribe_with_options_raw(topic_filter, options, wrapped_callback)
            .await
    }

    /// Internal method that accepts `PublishPacket` callbacks
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    async fn subscribe_with_options_raw<F>(
        &self,
        topic_filter: impl Into<String>,
        options: SubscribeOptions,
        callback: F,
    ) -> Result<(u16, QoS)>
    where
        F: Fn(PublishPacket) + Send + Sync + 'static,
    {
        let topic_filter = topic_filter.into();

        // Register callback first and get callback ID
        let inner = self.inner.read().await;
        let callback: PublishCallback = Arc::new(callback);
        let callback_id = inner
            .callback_manager
            .register_with_id(topic_filter.clone(), callback)
            .await?;

        // Create subscribe packet
        let filter = TopicFilter {
            filter: topic_filter.clone(),
            options: crate::packet::subscribe::SubscriptionOptions {
                qos: options.qos,
                no_local: options.no_local,
                retain_as_published: options.retain_as_published,
                retain_handling: match options.retain_handling {
                    crate::types::RetainHandling::SendAtSubscribe => {
                        crate::packet::subscribe::RetainHandling::SendAtSubscribe
                    }
                    crate::types::RetainHandling::SendIfNew => {
                        crate::packet::subscribe::RetainHandling::SendAtSubscribeIfNew
                    }
                    crate::types::RetainHandling::DontSend => {
                        crate::packet::subscribe::RetainHandling::DoNotSend
                    }
                },
            },
        };

        let mut packet = SubscribePacket {
            packet_id: 0, // Will be assigned in subscribe method
            filters: vec![filter],
            properties: Properties::default(),
        };

        // Add subscription identifier if provided
        if let Some(sub_id) = options.subscription_identifier {
            packet = packet.with_subscription_identifier(sub_id);
        }

        // Direct subscribe with callback ID - no command channels!
        match inner.subscribe_with_callback(packet, callback_id).await {
            Ok(results) => {
                if let Some(&(packet_id, qos)) = results.first() {
                    Ok((packet_id, qos))
                } else {
                    // Unregister callback on failure
                    inner.callback_manager.unregister(&topic_filter).await?;
                    Err(MqttError::ProtocolError(
                        "No results returned for subscription".to_string(),
                    ))
                }
            }
            Err(e) => {
                // Unregister callback on failure
                inner.callback_manager.unregister(&topic_filter).await?;
                Err(e)
            }
        }
    }

    /// Unsubscribes from a topic
    ///
    /// This is a DIRECT async method - no event loops!
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn unsubscribe(&self, topic_filter: impl Into<String>) -> Result<()> {
        let topic_filter = topic_filter.into();

        // Unregister callback first
        let inner = self.inner.read().await;
        inner.callback_manager.unregister(&topic_filter).await?;

        // Create unsubscribe packet
        let packet = UnsubscribePacket {
            packet_id: 0, // Will be assigned in unsubscribe method
            filters: vec![topic_filter],
            properties: Properties::default(),
        };

        // Direct unsubscribe - no command channels!
        inner.unsubscribe(packet).await
    }

    /// Subscribe to multiple topics at once
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn subscribe_many<F>(
        &self,
        topics: Vec<(&str, QoS)>,
        callback: F,
    ) -> Result<Vec<(u16, QoS)>>
    where
        F: Fn(crate::types::Message) + Send + Sync + 'static + Clone,
    {
        let mut results = Vec::new();
        for (topic, qos) in topics {
            let opts = SubscribeOptions {
                qos,
                ..Default::default()
            };
            let result = self
                .subscribe_with_options(topic, opts, callback.clone())
                .await?;
            results.push(result);
        }
        Ok(results)
    }

    /// Unsubscribe from multiple topics at once
    ///
    /// Returns a vector of results, one for each topic. Each result contains the topic
    /// and whether the unsubscribe operation succeeded for that topic.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn unsubscribe_many(&self, topics: Vec<&str>) -> Result<Vec<(String, Result<()>)>> {
        let mut results = Vec::with_capacity(topics.len());

        for topic in topics {
            let topic_string = topic.to_string();
            let result = self.unsubscribe(topic).await;
            results.push((topic_string, result));
        }

        Ok(results)
    }

    /// Publish a retained message
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn publish_retain(
        &self,
        topic: impl Into<String>,
        payload: impl Into<Vec<u8>>,
    ) -> Result<PublishResult> {
        let opts = PublishOptions {
            retain: true,
            ..Default::default()
        };
        self.publish_with_options(topic, payload, opts).await
    }

    /// Publish with `QoS` 0 (convenience method)
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn publish_qos0(
        &self,
        topic: impl Into<String>,
        payload: impl Into<Vec<u8>>,
    ) -> Result<PublishResult> {
        self.publish_qos(topic, payload, QoS::AtMostOnce).await
    }

    /// Publish with `QoS` 1 (convenience method)
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn publish_qos1(
        &self,
        topic: impl Into<String>,
        payload: impl Into<Vec<u8>>,
    ) -> Result<PublishResult> {
        self.publish_qos(topic, payload, QoS::AtLeastOnce).await
    }

    /// Publish with `QoS` 2 (convenience method)
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn publish_qos2(
        &self,
        topic: impl Into<String>,
        payload: impl Into<Vec<u8>>,
    ) -> Result<PublishResult> {
        self.publish_qos(topic, payload, QoS::ExactlyOnce).await
    }

    /// Check if message queuing is enabled
    pub async fn is_queue_on_disconnect(&self) -> bool {
        let inner = self.inner.read().await;
        inner.is_queue_on_disconnect()
    }

    /// Set whether to queue messages when disconnected
    pub async fn set_queue_on_disconnect(&self, enabled: bool) {
        let mut inner = self.inner.write().await;
        inner.set_queue_on_disconnect(enabled);
    }

    /// Get error recovery configuration
    pub async fn error_recovery_config(&self) -> ErrorRecoveryConfig {
        self.error_recovery_config.read().await.clone()
    }

    /// Set error recovery configuration
    pub async fn set_error_recovery_config(&self, config: ErrorRecoveryConfig) {
        *self.error_recovery_config.write().await = config;
    }

    /// Clear all error callbacks
    pub async fn clear_error_callbacks(&self) {
        self.error_callbacks.write().await.clear();
    }

    /// Clear all connection event callbacks
    pub async fn clear_connection_event_callbacks(&self) {
        self.connection_event_callbacks.write().await.clear();
    }

    /// Get direct access to session state (for testing)
    #[cfg(test)]
    pub async fn session_state(&self) -> Arc<RwLock<crate::session::SessionState>> {
        Arc::clone(&self.inner.read().await.session)
    }
}

/// Implementation of `MqttClientTrait` for `MqttClient`
#[allow(clippy::manual_async_fn)]
impl MqttClientTrait for MqttClient {
    fn is_connected(&self) -> impl Future<Output = bool> + Send + '_ {
        async move { self.is_connected().await }
    }

    fn client_id(&self) -> impl Future<Output = String> + Send + '_ {
        async move { self.client_id().await }
    }

    fn connect<'a>(&'a self, address: &'a str) -> impl Future<Output = Result<()>> + Send + 'a {
        async move { self.connect(address).await }
    }

    fn connect_with_options<'a>(
        &'a self,
        address: &'a str,
        options: ConnectOptions,
    ) -> impl Future<Output = Result<ConnectResult>> + Send + 'a {
        async move { self.connect_with_options(address, options).await }
    }

    fn disconnect(&self) -> impl Future<Output = Result<()>> + Send + '_ {
        async move { self.disconnect().await }
    }

    fn publish<'a>(
        &'a self,
        topic: impl Into<String> + Send + 'a,
        payload: impl Into<Vec<u8>> + Send + 'a,
    ) -> impl Future<Output = Result<PublishResult>> + Send + 'a {
        async move { self.publish(topic, payload).await }
    }

    fn publish_qos<'a>(
        &'a self,
        topic: impl Into<String> + Send + 'a,
        payload: impl Into<Vec<u8>> + Send + 'a,
        qos: QoS,
    ) -> impl Future<Output = Result<PublishResult>> + Send + 'a {
        async move { self.publish_qos(topic, payload, qos).await }
    }

    fn publish_with_options<'a>(
        &'a self,
        topic: impl Into<String> + Send + 'a,
        payload: impl Into<Vec<u8>> + Send + 'a,
        options: PublishOptions,
    ) -> impl Future<Output = Result<PublishResult>> + Send + 'a {
        async move { self.publish_with_options(topic, payload, options).await }
    }

    fn subscribe<'a, F>(
        &'a self,
        topic_filter: impl Into<String> + Send + 'a,
        callback: F,
    ) -> impl Future<Output = Result<(u16, QoS)>> + Send + 'a
    where
        F: Fn(crate::types::Message) + Send + Sync + 'static,
    {
        async move { self.subscribe(topic_filter, callback).await }
    }

    fn subscribe_with_options<'a, F>(
        &'a self,
        topic_filter: impl Into<String> + Send + 'a,
        options: SubscribeOptions,
        callback: F,
    ) -> impl Future<Output = Result<(u16, QoS)>> + Send + 'a
    where
        F: Fn(crate::types::Message) + Send + Sync + 'static,
    {
        async move {
            self.subscribe_with_options(topic_filter, options, callback)
                .await
        }
    }

    fn unsubscribe<'a>(
        &'a self,
        topic_filter: impl Into<String> + Send + 'a,
    ) -> impl Future<Output = Result<()>> + Send + 'a {
        async move { self.unsubscribe(topic_filter).await }
    }

    fn subscribe_many<'a, F>(
        &'a self,
        topics: Vec<(&'a str, QoS)>,
        callback: F,
    ) -> impl Future<Output = Result<Vec<(u16, QoS)>>> + Send + 'a
    where
        F: Fn(crate::types::Message) + Send + Sync + 'static + Clone,
    {
        async move { self.subscribe_many(topics, callback).await }
    }

    fn unsubscribe_many<'a>(
        &'a self,
        topics: Vec<&'a str>,
    ) -> impl Future<Output = Result<Vec<(String, Result<()>)>>> + Send + 'a {
        async move { self.unsubscribe_many(topics).await }
    }

    fn publish_retain<'a>(
        &'a self,
        topic: impl Into<String> + Send + 'a,
        payload: impl Into<Vec<u8>> + Send + 'a,
    ) -> impl Future<Output = Result<PublishResult>> + Send + 'a {
        async move { self.publish_retain(topic, payload).await }
    }

    fn publish_qos0<'a>(
        &'a self,
        topic: impl Into<String> + Send + 'a,
        payload: impl Into<Vec<u8>> + Send + 'a,
    ) -> impl Future<Output = Result<PublishResult>> + Send + 'a {
        async move { self.publish_qos0(topic, payload).await }
    }

    fn publish_qos1<'a>(
        &'a self,
        topic: impl Into<String> + Send + 'a,
        payload: impl Into<Vec<u8>> + Send + 'a,
    ) -> impl Future<Output = Result<PublishResult>> + Send + 'a {
        async move { self.publish_qos1(topic, payload).await }
    }

    fn publish_qos2<'a>(
        &'a self,
        topic: impl Into<String> + Send + 'a,
        payload: impl Into<Vec<u8>> + Send + 'a,
    ) -> impl Future<Output = Result<PublishResult>> + Send + 'a {
        async move { self.publish_qos2(topic, payload).await }
    }

    fn is_queue_on_disconnect(&self) -> impl Future<Output = bool> + Send + '_ {
        async move { self.is_queue_on_disconnect().await }
    }

    fn set_queue_on_disconnect(&self, enabled: bool) -> impl Future<Output = ()> + Send + '_ {
        async move { self.set_queue_on_disconnect(enabled).await }
    }
}

impl MqttClient {
    /// Simulate abnormal disconnection (for testing will messages)
    /// This method closes the connection without sending a DISCONNECT packet,
    /// which causes the broker to send the will message.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub async fn disconnect_abnormally(&self) -> Result<()> {
        let mut inner = self.inner.write().await;
        inner.disconnect_with_packet(false).await
    }

    /// Parses an address string to determine transport type and components
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    fn parse_address(address: &str) -> Result<(ClientTransportType, &str, u16)> {
        if let Some(rest) = address.strip_prefix("mqtt://") {
            let (host, port) = Self::split_host_port(rest, 1883)?;
            Ok((ClientTransportType::Tcp, host, port))
        } else if let Some(rest) = address.strip_prefix("mqtts://") {
            let (host, port) = Self::split_host_port(rest, 8883)?;
            Ok((ClientTransportType::Tls, host, port))
        } else if let Some(rest) = address.strip_prefix("tcp://") {
            let (host, port) = Self::split_host_port(rest, 1883)?;
            Ok((ClientTransportType::Tcp, host, port))
        } else if let Some(rest) = address.strip_prefix("ssl://") {
            let (host, port) = Self::split_host_port(rest, 8883)?;
            Ok((ClientTransportType::Tls, host, port))
        } else {
            // Default to TCP if no scheme
            let (host, port) = Self::split_host_port(address, 1883)?;
            Ok((ClientTransportType::Tcp, host, port))
        }
    }

    /// Splits a host:port string
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    fn split_host_port(address: &str, default_port: u16) -> Result<(&str, u16)> {
        if let Some(colon_pos) = address.rfind(':') {
            let host = &address[..colon_pos];
            let port_str = &address[colon_pos + 1..];
            let port = port_str
                .parse::<u16>()
                .map_err(|_| MqttError::ConnectionError(format!("Invalid port: {port_str}")))?;
            Ok((host, port))
        } else {
            Ok((address, default_port))
        }
    }

    /// Monitor connection and handle automatic reconnection
    async fn monitor_connection(&self) {
        loop {
            // Wait for disconnection
            tokio::time::sleep(Duration::from_secs(1)).await;

            let inner = self.inner.read().await;
            if !inner.is_connected() {
                // Get reconnection config
                let reconnect_config = inner.options.reconnect_config.clone();
                let last_address = inner.last_address.clone();
                drop(inner); // Release lock before potentially long-running operation

                if !reconnect_config.enabled {
                    break;
                }

                if let Some(address) = last_address {
                    // Attempt reconnection with exponential backoff
                    if let Err(e) = self.attempt_reconnection(&address, &reconnect_config).await {
                        tracing::error!("Reconnection failed: {}", e);
                        break;
                    }
                }
            }
        }
    }

    /// Attempt reconnection with exponential backoff
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    async fn attempt_reconnection(
        &self,
        address: &str,
        config: &crate::types::ReconnectConfig,
    ) -> Result<()> {
        let mut delay = config.initial_delay;

        loop {
            // Increment attempt counter
            let attempt = {
                let mut inner = self.inner.write().await;
                inner.reconnect_attempt += 1;
                inner.reconnect_attempt
            };

            // Check max attempts
            if config.max_attempts > 0 && attempt > config.max_attempts {
                return Err(MqttError::ConnectionError(
                    "Max reconnection attempts exceeded".to_string(),
                ));
            }

            // Trigger reconnecting event
            self.trigger_connection_event(ConnectionEvent::Reconnecting { attempt })
                .await;

            // Wait before attempting
            tokio::time::sleep(delay).await;

            // Try to reconnect using internal method to avoid recursion
            match self.connect_internal(address).await {
                Ok(_) => {
                    tracing::info!("Reconnected successfully after {} attempts", attempt);

                    // Restore subscriptions if session was not resumed
                    let inner = self.inner.read().await;
                    let stored_subs = inner.stored_subscriptions.read().await.clone();
                    drop(inner); // Release lock before resubscribing

                    for (topic, options, callback_id) in stored_subs {
                        if let Err(e) = self
                            .resubscribe_internal(&topic, options, callback_id)
                            .await
                        {
                            tracing::warn!("Failed to restore subscription to {}: {}", topic, e);
                        }
                    }

                    // Send queued messages
                    self.send_queued_messages().await;

                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!("Reconnection attempt {} failed: {}", attempt, e);

                    // Calculate next delay with exponential backoff
                    delay = std::cmp::min(
                        Duration::from_secs_f32(delay.as_secs_f32() * config.backoff_multiplier),
                        config.max_delay,
                    );
                }
            }
        }
    }

    /// Send messages that were queued during disconnection
    async fn send_queued_messages(&self) {
        let messages = {
            let inner = self.inner.read().await;
            let mut queued = inner.queued_messages.lock().await;
            std::mem::take(&mut *queued)
        };

        for mut msg in messages {
            // Set DUP flag for replayed messages
            msg.dup = true;

            if let Err(e) = self.publish_packet(msg).await {
                tracing::warn!("Failed to send queued message: {}", e);
            }
        }
    }

    /// Internal method to resubscribe with stored options and callback
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    async fn resubscribe_internal(
        &self,
        topic: &str,
        options: crate::packet::subscribe::SubscriptionOptions,
        callback_id: crate::callback::CallbackId,
    ) -> Result<()> {
        // Restore the callback first
        let inner = self.inner.read().await;
        inner.callback_manager.restore_callback(callback_id).await?;
        drop(inner);

        // Create subscribe packet
        let packet = SubscribePacket {
            packet_id: self.inner.read().await.packet_id_generator.next(),
            filters: vec![crate::packet::subscribe::TopicFilter {
                filter: topic.to_string(),
                options,
            }],
            properties: Properties::new(),
        };

        // Send the subscribe packet with the callback ID
        let inner = self.inner.read().await;
        inner.subscribe_with_callback(packet, callback_id).await?;
        Ok(())
    }

    /// Internal method to publish a packet
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    async fn publish_packet(&self, packet: PublishPacket) -> Result<()> {
        let mut inner = self.inner.write().await;
        inner
            .send_packet(crate::packet::Packet::Publish(packet))
            .await
    }
}

/// Client transport type for parsing addresses
#[derive(Debug, Clone, Copy)]
enum ClientTransportType {
    Tcp,
    Tls,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let client = MqttClient::new("test-client");
        assert!(!client.is_connected().await);
        assert_eq!(client.client_id().await, "test-client");
    }

    #[test]
    fn test_parse_address() {
        // Test MQTT scheme
        let (transport, host, port) = MqttClient::parse_address("mqtt://localhost:1883").unwrap();
        assert!(matches!(transport, ClientTransportType::Tcp));
        assert_eq!(host, "localhost");
        assert_eq!(port, 1883);

        // Test MQTTS scheme
        let (transport, host, port) =
            MqttClient::parse_address("mqtts://broker.example.com").unwrap();
        assert!(matches!(transport, ClientTransportType::Tls));
        assert_eq!(host, "broker.example.com");
        assert_eq!(port, 8883);

        // Test no scheme (defaults to TCP)
        let (transport, host, port) = MqttClient::parse_address("localhost").unwrap();
        assert!(matches!(transport, ClientTransportType::Tcp));
        assert_eq!(host, "localhost");
        assert_eq!(port, 1883);
    }
}
