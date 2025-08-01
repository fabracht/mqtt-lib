//! Simple MQTT Client Example
//!
//! This example demonstrates the basic usage of the MQTT v5.0 client library.
//! It shows how to:
//! - Connect to an MQTT broker
//! - Subscribe to topics with callbacks  
//! - Publish messages
//! - Handle connection events
//!
//! This is the recommended starting point for new users.
//!
//! ## Usage
//!
//! ```bash
//! # Run with local mosquitto broker
//! cargo run --example simple_client
//!
//! # Or specify a different broker
//! MQTT_BROKER=mqtt://test.mosquitto.org:1883 cargo run --example simple_client
//! ```

use mqtt_v5::{ConnectOptions, ConnectionEvent, MqttClient};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize basic logging using tracing (already a dependency)
    tracing_subscriber::fmt::init();

    // Get broker URL from environment or use default
    let broker_url =
        std::env::var("MQTT_BROKER").unwrap_or_else(|_| "mqtt://localhost:1883".to_string());

    println!("🚀 Starting simple MQTT client");
    println!("📡 Connecting to broker: {broker_url}");

    // Create a client with basic options
    let options = ConnectOptions::new("simple-client-demo")
        .with_keep_alive(Duration::from_secs(60)) // Send keep-alive every 60s
        .with_clean_start(true) // Start fresh session
        .with_automatic_reconnect(true); // Auto-reconnect on disconnect

    let client = MqttClient::with_options(options);

    // Set up connection event handler
    client
        .on_connection_event(|event| match event {
            ConnectionEvent::Connected { session_present } => {
                println!("✅ Connected to MQTT broker (session_present: {session_present})");
            }
            ConnectionEvent::Disconnected { reason } => {
                println!("❌ Disconnected from broker: {reason:?}");
            }
            ConnectionEvent::Reconnecting { attempt } => {
                println!("🔄 Reconnecting... (attempt {attempt})");
            }
            ConnectionEvent::ReconnectFailed { error } => {
                println!("💥 Reconnection failed: {error}");
            }
        })
        .await?;

    // Connect to the broker
    client.connect(&broker_url).await?;

    // Subscribe to a topic with a callback function
    println!("📬 Subscribing to topic 'demo/messages'");
    let (packet_id, granted_qos) = client
        .subscribe("demo/messages", |message| {
            println!(
                "📨 Received message on '{}': {}",
                message.topic,
                String::from_utf8_lossy(&message.payload)
            );
        })
        .await?;

    println!("✅ Subscribed with packet_id={packet_id}, QoS={granted_qos:?}");

    // Subscribe to another topic for JSON data
    println!("📬 Subscribing to topic 'demo/json'");
    client
        .subscribe("demo/json", |message| {
            println!(
                "📊 Received JSON on '{}': {}",
                message.topic,
                String::from_utf8_lossy(&message.payload)
            );

            // Try to parse as JSON (just for demonstration)
            if let Ok(json_value) = serde_json::from_slice::<serde_json::Value>(&message.payload) {
                println!("   Parsed JSON: {json_value:#}");
            }
        })
        .await?;

    // Publish some example messages
    println!("📤 Publishing messages...");

    // Simple string message (QoS 0)
    client
        .publish("demo/messages", b"Hello from Rust MQTT client!")
        .await?;
    println!("   ✅ Published simple message");

    // QoS 1 message (delivery guaranteed)
    client
        .publish_qos1("demo/messages", b"This is a QoS 1 message")
        .await?;
    println!("   ✅ Published QoS 1 message");

    // JSON message
    let json_data = serde_json::json!({
        "sensor": "temperature",
        "value": 23.5,
        "unit": "celsius",
        "timestamp": chrono::Utc::now().to_rfc3339()
    });
    client
        .publish_qos1("demo/json", json_data.to_string().as_bytes())
        .await?;
    println!("   ✅ Published JSON message");

    // Retained message (will be delivered to future subscribers)
    client
        .publish_retain("demo/status", b"Client is running")
        .await?;
    println!("   ✅ Published retained status message");

    // Wait to receive messages
    println!("⏳ Waiting 30 seconds to receive messages...");
    println!("   (You can publish to 'demo/messages' or 'demo/json' from another client to see them here)");

    for i in 1..=6 {
        sleep(Duration::from_secs(5)).await;

        // Publish a counter message every 5 seconds
        let message = format!("Counter message #{i}");
        client.publish("demo/messages", message.as_bytes()).await?;
        println!("   📤 Published: {message}");
    }

    // Clean disconnect
    println!("👋 Disconnecting...");
    client.disconnect().await?;
    println!("✅ Disconnected successfully");

    Ok(())
}
