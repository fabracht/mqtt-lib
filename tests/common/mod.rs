//! Common test utilities and scenarios

use mqtt_v5::{
    ConnectOptions, MessageProperties, MqttClient, PublishOptions, PublishProperties, QoS,
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use ulid::Ulid;

/// Default test broker address
pub const TEST_BROKER: &str = "mqtt://127.0.0.1:1883";

/// Default timeout for test operations
#[allow(dead_code)]
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Generate a unique test client ID
pub fn test_client_id(test_name: &str) -> String {
    format!("test-{}-{}", test_name, Ulid::new())
}

/// Create a connected test client with default settings
pub async fn create_test_client(name: &str) -> MqttClient {
    let client = MqttClient::new(test_client_id(name));
    client
        .connect(TEST_BROKER)
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
    pub fn callback(&self) -> impl Fn(mqtt_v5::types::Message) + Send + Sync + 'static {
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

/// Counter for tracking events
pub struct EventCounter {
    count: Arc<AtomicU32>,
}

impl EventCounter {
    pub fn new() -> Self {
        Self {
            count: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Get a callback that increments the counter
    pub fn callback(&self) -> impl Fn(mqtt_v5::types::Message) + Send + Sync + 'static {
        let count = self.count.clone();
        move |_| {
            count.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Get current count
    pub fn get(&self) -> u32 {
        self.count.load(Ordering::SeqCst)
    }

    /// Reset counter to zero
    pub fn reset(&self) {
        self.count.store(0, Ordering::SeqCst);
    }

    /// Wait for count to reach target
    pub async fn wait_for(&self, target: u32, timeout: Duration) -> bool {
        let start = tokio::time::Instant::now();
        while start.elapsed() < timeout {
            if self.get() >= target {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
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
    let sub_opts = mqtt_v5::SubscribeOptions {
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
    use super::*;

    #[tokio::test]
    async fn test_message_collector() {
        let collector = MessageCollector::new();

        // Simulate receiving messages
        let callback = collector.callback();
        let message = mqtt_v5::types::Message {
            topic: "test/topic".to_string(),
            payload: b"test".to_vec(),
            qos: QoS::AtMostOnce,
            retain: false,
            properties: MessageProperties::default(),
        };
        callback(message);

        // Wait a bit for async processing
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert_eq!(collector.count().await, 1);
        let messages = collector.get_messages().await;
        assert_eq!(messages[0].topic, "test/topic");
    }

    #[tokio::test]
    async fn test_event_counter() {
        let counter = EventCounter::new();
        assert_eq!(counter.get(), 0);

        let callback = counter.callback();
        // Simulate events
        for _ in 0..5 {
            callback(mqtt_v5::types::Message {
                topic: "test".to_string(),
                payload: vec![],
                qos: QoS::AtMostOnce,
                retain: false,
                properties: MessageProperties::default(),
            });
        }

        assert_eq!(counter.get(), 5);
        counter.reset();
        assert_eq!(counter.get(), 0);
    }
}
