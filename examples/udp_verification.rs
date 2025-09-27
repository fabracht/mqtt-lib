//! UDP transport verification example
//!
//! This example connects to a broker via UDP and performs pub/sub operations
//! to verify UDP transport functionality.

use mqtt5::client::MqttClient;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    info!("=== UDP Transport Verification Test ===");

    let udp_url = "mqtt-udp://localhost:1884";
    info!("Creating UDP clients...");

    let publisher = MqttClient::new("udp_publisher_demo");
    let subscriber = MqttClient::new("udp_subscriber_demo");

    info!("Connecting publisher to {}", udp_url);
    match publisher.connect(udp_url).await {
        Ok(_) => info!("✓ Publisher connected successfully"),
        Err(e) => {
            error!("✗ Publisher connection failed: {}", e);
            return Err(e.into());
        }
    }

    info!("Connecting subscriber to {}", udp_url);
    match subscriber.connect(udp_url).await {
        Ok(_) => info!("✓ Subscriber connected successfully"),
        Err(e) => {
            error!("✗ Subscriber connection failed: {}", e);
            publisher.disconnect().await?;
            return Err(e.into());
        }
    }

    let (tx, mut rx) = mpsc::channel(10);
    let received_messages = Arc::new(Mutex::new(Vec::new()));
    let received_clone = Arc::clone(&received_messages);

    info!("Setting up subscription to 'test/udp/verification'");
    subscriber
        .subscribe("test/udp/verification", move |msg| {
            let tx = tx.clone();
            let received = received_clone.clone();
            tokio::spawn(async move {
                info!("Received message on topic: {}", msg.topic);
                received.lock().await.push(msg.clone());
                let _ = tx.send(msg).await;
            });
        })
        .await?;
    info!("✓ Subscription setup complete");

    tokio::time::sleep(Duration::from_millis(100)).await;

    info!("Publishing test messages...");
    let test_messages = [
        ("Hello UDP!", "test/udp/verification"),
        ("UDP Transport Working", "test/udp/verification"),
        ("🚀 MQTT over UDP", "test/udp/verification"),
    ];

    for (payload, topic) in test_messages.iter() {
        info!("Publishing '{}' to '{}'", payload, topic);
        match publisher.publish(*topic, payload.as_bytes()).await {
            Ok(_) => info!("✓ Published successfully"),
            Err(e) => error!("✗ Publish failed: {}", e),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    info!("Waiting for messages...");
    let mut received_count = 0;
    let timeout = Duration::from_secs(3);
    let start = tokio::time::Instant::now();

    while received_count < test_messages.len() && start.elapsed() < timeout {
        match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Some(msg)) => {
                received_count += 1;
                info!(
                    "✓ Message {}/{} received: topic='{}', payload='{}'",
                    received_count,
                    test_messages.len(),
                    msg.topic,
                    String::from_utf8_lossy(&msg.payload)
                );
            }
            Ok(None) => break,
            Err(_) => {}
        }
    }

    info!("\n=== Test Results ===");
    info!("Messages sent: {}", test_messages.len());
    info!("Messages received: {}", received_count);

    if received_count == test_messages.len() {
        info!("✅ UDP TRANSPORT VERIFICATION: SUCCESS");
        info!("All messages were successfully published and received via UDP");
    } else {
        error!("❌ UDP TRANSPORT VERIFICATION: PARTIAL SUCCESS");
        error!(
            "Only {}/{} messages were received",
            received_count,
            test_messages.len()
        );
    }

    info!("\nTesting QoS 1 publish over UDP...");
    match publisher
        .publish_qos("test/udp/qos1", b"QoS 1 test", mqtt5::QoS::AtLeastOnce)
        .await
    {
        Ok(result) => {
            if let Some(packet_id) = result.packet_id() {
                info!("✓ QoS 1 publish acknowledged with packet_id: {}", packet_id);
            } else {
                error!("✗ QoS 1 publish didn't return packet_id");
            }
        }
        Err(e) => error!("✗ QoS 1 publish failed: {}", e),
    }

    info!("\nDisconnecting clients...");
    publisher.disconnect().await?;
    subscriber.disconnect().await?;
    info!("✓ Clients disconnected");

    info!("\n=== UDP Transport Verification Complete ===");

    if received_count == test_messages.len() {
        Ok(())
    } else {
        Err("Not all messages were received".into())
    }
}
