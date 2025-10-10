//! Common test utilities and scenarios

pub mod cli_helpers;

use mqtt5::{ConnectOptions, MqttClient, PublishOptions, PublishProperties, QoS};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use ulid::Ulid;

/// Default test broker address
pub const TEST_BROKER: &str = "mqtt://127.0.0.1:1883";

use mqtt5::broker::server::MqttBroker;
use std::sync::atomic::{AtomicU16, Ordering};
use tokio::task::JoinHandle;

// Global port counter for TLS tests to avoid conflicts
#[allow(dead_code)]
static TLS_PORT_COUNTER: AtomicU16 = AtomicU16::new(0);

/// Test broker instance with cleanup
#[derive(Debug)]
pub struct TestBroker {
    address: String,
    #[allow(dead_code)]
    handle: JoinHandle<()>,
}

impl TestBroker {
    /// Start a test broker on a random port
    #[allow(dead_code)]
    pub async fn start() -> Self {
        use mqtt5::broker::config::{BrokerConfig, StorageBackend, StorageConfig};

        // Create broker config with in-memory storage (persistence enabled but in-memory)
        let storage_config = StorageConfig {
            backend: StorageBackend::Memory, // Use memory instead of files
            enable_persistence: true,        // Keep persistence for session tests
            ..Default::default()
        };

        let config = BrokerConfig::default()
            .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
            .with_storage(storage_config);

        // Create broker with config
        let mut broker = MqttBroker::with_config(config)
            .await
            .expect("Failed to create test broker");

        let addr = broker.local_addr().expect("Failed to get broker address");
        let address = format!("mqtt://{addr}");

        // Start broker in background
        let handle = tokio::spawn(async move {
            let _ = broker.run().await;
        });

        // Give broker time to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        Self { address, handle }
    }

    /// Start a test broker with TLS support on a random port
    #[allow(dead_code)]
    pub async fn start_with_tls() -> Self {
        use mqtt5::broker::config::{
            BrokerConfig, StorageBackend, StorageConfig, TlsConfig as BrokerTlsConfig,
        };
        use std::path::PathBuf;

        // Initialize the crypto provider for rustls (required for TLS)
        let _ = rustls::crypto::ring::default_provider().install_default();

        // Create broker config with in-memory storage
        let storage_config = StorageConfig {
            backend: StorageBackend::Memory,
            enable_persistence: true,
            ..Default::default()
        };

        // Configure TLS - use an atomic counter to ensure unique ports
        let tls_port = 20000 + TLS_PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
        let tls_config = BrokerTlsConfig::new(
            PathBuf::from("test_certs/server.pem"),
            PathBuf::from("test_certs/server.key"),
        )
        // Don't set CA file to avoid requiring client certs
        .with_require_client_cert(false)
        .with_bind_address(
            format!("127.0.0.1:{tls_port}")
                .parse::<std::net::SocketAddr>()
                .unwrap(),
        );

        let config = BrokerConfig::default()
            .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
            .with_storage(storage_config)
            .with_tls(tls_config);

        // Create broker with config
        let mut broker = MqttBroker::with_config(config)
            .await
            .expect("Failed to create test broker with TLS");

        // Use the TLS port we configured
        let address = format!("mqtts://127.0.0.1:{tls_port}");

        // Start broker in background
        let handle = tokio::spawn(async move {
            let _ = broker.run().await;
        });

        // Give broker time to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        Self { address, handle }
    }

    /// Start a test broker with WebSocket support on a random port
    #[allow(dead_code)]
    pub async fn start_with_websocket() -> Self {
        use mqtt5::broker::config::{BrokerConfig, StorageBackend, StorageConfig, WebSocketConfig};

        let storage_config = StorageConfig {
            backend: StorageBackend::Memory,
            enable_persistence: true,
            ..Default::default()
        };

        let ws_port = 20000 + TLS_PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
        let ws_config = WebSocketConfig::new()
            .with_bind_addresses(vec![format!("127.0.0.1:{ws_port}")
                .parse::<std::net::SocketAddr>()
                .unwrap()])
            .with_path("/mqtt".to_string());

        let config = BrokerConfig::default()
            .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
            .with_storage(storage_config)
            .with_websocket(ws_config);

        let mut broker = MqttBroker::with_config(config)
            .await
            .expect("Failed to create test broker with WebSocket");

        let address = format!("ws://127.0.0.1:{ws_port}/mqtt");

        let handle = tokio::spawn(async move {
            let _ = broker.run().await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        Self { address, handle }
    }

    /// Start a test broker with authentication on a random port
    #[allow(dead_code)]
    pub async fn start_with_authentication() -> Self {
        use mqtt5::broker::config::{
            AuthConfig, AuthMethod, BrokerConfig, StorageBackend, StorageConfig,
        };
        use std::path::PathBuf;
        use std::process::Command;

        let password_file = PathBuf::from("test_passwords.txt");

        let _ = std::fs::remove_file(&password_file);

        let status = Command::new("./target/release/mqttv5")
            .args([
                "passwd",
                "-c",
                "-b",
                "testpass",
                "testuser",
                password_file.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to create password file");

        assert!(status.success(), "Failed to create password file");

        for _ in 0..10 {
            if password_file.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let storage_config = StorageConfig {
            backend: StorageBackend::Memory,
            enable_persistence: true,
            ..Default::default()
        };

        let auth_config = AuthConfig {
            allow_anonymous: false,
            password_file: Some(password_file.clone()),
            auth_method: AuthMethod::Password,
            auth_data: Some(std::fs::read(&password_file).expect("Failed to read password file")),
        };

        let config = BrokerConfig::default()
            .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
            .with_storage(storage_config)
            .with_auth(auth_config);

        let mut broker = MqttBroker::with_config(config)
            .await
            .expect("Failed to create test broker with authentication");

        let addr = broker.local_addr().expect("Failed to get broker address");
        let address = format!("mqtt://{addr}");

        let handle = tokio::spawn(async move {
            let _ = broker.run().await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        Self { address, handle }
    }

    /// Get the broker address
    #[allow(dead_code)]
    pub fn address(&self) -> &str {
        &self.address
    }
}

impl Drop for TestBroker {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Default timeout for test operations
#[allow(dead_code)]
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Generate a unique test client ID
pub fn test_client_id(test_name: &str) -> String {
    format!("test-{test_name}-{}", Ulid::new())
}

/// Create a connected test client with default settings
pub async fn create_test_client(name: &str) -> MqttClient {
    create_test_client_with_broker(name, TEST_BROKER).await
}

/// Create a connected test client with specific broker address
pub async fn create_test_client_with_broker(name: &str, broker_addr: &str) -> MqttClient {
    let client = MqttClient::new(test_client_id(name));
    client
        .connect(broker_addr)
        .await
        .expect("Failed to connect");
    client
}

/// Create a test client with custom options
#[allow(dead_code)]
pub async fn create_test_client_with_options(_name: &str, options: ConnectOptions) -> MqttClient {
    let client = MqttClient::new(options.client_id.clone());
    client
        .connect_with_options(TEST_BROKER, options)
        .await
        .expect("Failed to connect");
    client
}

/// Message collector for testing subscriptions
#[derive(Clone)]
pub struct MessageCollector {
    messages: Arc<RwLock<Vec<ReceivedMessage>>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ReceivedMessage {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: QoS,
    pub retain: bool,
}

impl MessageCollector {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Get a callback function for subscriptions
    pub fn callback(&self) -> impl Fn(mqtt5::types::Message) + Send + Sync + 'static {
        let messages = self.messages.clone();
        move |msg| {
            let received = ReceivedMessage {
                topic: msg.topic.clone(),
                payload: msg.payload.clone(),
                qos: msg.qos,
                retain: msg.retain,
            };

            // Use spawn to avoid blocking the callback
            let messages = messages.clone();
            tokio::spawn(async move {
                messages.write().await.push(received);
            });
        }
    }

    /// Wait for a specific number of messages
    #[allow(dead_code)]
    pub async fn wait_for_messages(&self, count: usize, timeout: Duration) -> bool {
        let start = tokio::time::Instant::now();
        while start.elapsed() < timeout {
            if self.messages.read().await.len() >= count {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    /// Get all received messages
    pub async fn get_messages(&self) -> Vec<ReceivedMessage> {
        self.messages.read().await.clone()
    }

    /// Clear all messages
    #[allow(dead_code)]
    pub async fn clear(&self) {
        self.messages.write().await.clear();
    }

    /// Get message count
    pub async fn count(&self) -> usize {
        self.messages.read().await.len()
    }
}

/// Test scenario: Basic publish/subscribe
#[allow(dead_code)]
pub async fn test_basic_pubsub(
    client: &MqttClient,
    topic: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let collector = MessageCollector::new();

    // Subscribe
    client.subscribe(topic, collector.callback()).await?;

    // Publish test message
    let test_payload = b"test message";
    client.publish(topic, test_payload).await?;

    // Wait for message
    if !collector.wait_for_messages(1, Duration::from_secs(1)).await {
        return Err("Timeout waiting for message".into());
    }

    // Verify message
    let messages = collector.get_messages().await;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].topic, topic);
    assert_eq!(messages[0].payload, test_payload);

    Ok(())
}

/// Test scenario: `QoS` flow validation
#[allow(dead_code)]
pub async fn test_qos_flow(
    client: &MqttClient,
    topic: &str,
    qos: QoS,
) -> Result<(), Box<dyn std::error::Error>> {
    let collector = MessageCollector::new();

    // Subscribe with specified QoS
    let sub_opts = mqtt5::SubscribeOptions {
        qos,
        ..Default::default()
    };
    client
        .subscribe_with_options(topic, sub_opts, collector.callback())
        .await?;

    // Publish with specified QoS
    let pub_opts = PublishOptions {
        qos,
        retain: false,
        properties: PublishProperties::default(),
    };
    let test_payload = format!("QoS {} test", qos as u8);
    client
        .publish_with_options(topic, test_payload.as_bytes(), pub_opts)
        .await?;

    // Wait and verify
    if !collector.wait_for_messages(1, Duration::from_secs(2)).await {
        return Err("Timeout waiting for QoS message".into());
    }

    let messages = collector.get_messages().await;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].qos, qos);

    Ok(())
}

/// Test scenario: Multiple subscriptions
#[allow(dead_code)]
pub async fn test_multiple_subscriptions(
    client: &MqttClient,
) -> Result<(), Box<dyn std::error::Error>> {
    let collector1 = MessageCollector::new();
    let collector2 = MessageCollector::new();
    let collector3 = MessageCollector::new();

    // Subscribe to different topics
    client
        .subscribe("test/topic1", collector1.callback())
        .await?;
    client
        .subscribe("test/topic2", collector2.callback())
        .await?;
    client.subscribe("test/+", collector3.callback()).await?; // Wildcard

    // Publish to both topics
    client.publish("test/topic1", b"message1").await?;
    client.publish("test/topic2", b"message2").await?;

    // Wait for messages
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify
    assert_eq!(collector1.count().await, 1);
    assert_eq!(collector2.count().await, 1);
    assert_eq!(collector3.count().await, 2); // Should receive both

    Ok(())
}

/// Test scenario: Retained message handling
#[allow(dead_code)]
pub async fn test_retained_messages(
    client: &MqttClient,
    topic: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // First, publish a retained message
    client.publish_retain(topic, b"retained message").await?;

    // Create new subscriber
    let collector = MessageCollector::new();
    client.subscribe(topic, collector.callback()).await?;

    // Should immediately receive the retained message
    if !collector
        .wait_for_messages(1, Duration::from_millis(500))
        .await
    {
        return Err("Timeout waiting for retained message".into());
    }

    let messages = collector.get_messages().await;
    assert_eq!(messages.len(), 1);
    assert!(messages[0].retain);
    assert_eq!(messages[0].payload, b"retained message");

    // Clear retained message
    client.publish_retain(topic, b"").await?;

    Ok(())
}

/// Wait helper with custom message
#[allow(dead_code)]
pub async fn wait_with_message(duration: Duration, message: &str) {
    println!("{message}");
    tokio::time::sleep(duration).await;
}

/// Ensure a topic is clean (no retained messages)
#[allow(dead_code)]
pub async fn cleanup_topic(
    client: &MqttClient,
    topic: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Clear any retained messages
    client.publish_retain(topic, b"").await?;
    Ok(())
}

/// Create multiple test clients
#[allow(dead_code)]
pub async fn create_test_clients(names: &[&str]) -> Vec<MqttClient> {
    let mut clients = Vec::new();
    for name in names {
        clients.push(create_test_client(name).await);
    }
    clients
}

/// Disconnect and cleanup multiple clients
#[allow(dead_code)]
pub async fn cleanup_clients(clients: Vec<MqttClient>) {
    for client in clients {
        let _ = client.disconnect().await;
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_message_collector() {
        let collector = super::MessageCollector::new();

        let callback = collector.callback();
        let message = mqtt5::types::Message {
            topic: "test/topic".to_string(),
            payload: b"test".to_vec(),
            qos: mqtt5::QoS::AtMostOnce,
            retain: false,
            properties: mqtt5::MessageProperties::default(),
        };
        callback(message);

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(collector.count().await, 1);
        let messages = collector.get_messages().await;
        assert_eq!(messages[0].topic, "test/topic");
    }
}
