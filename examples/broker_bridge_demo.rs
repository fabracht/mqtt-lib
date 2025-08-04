//! Example demonstrating broker-to-broker bridging
//!
//! This example sets up two brokers and creates a bridge between them,
//! showing how messages can flow between separate MQTT broker instances.

use mqtt5::broker::bridge::{BridgeConfig, BridgeDirection};
use mqtt5::broker::{BrokerConfig, MqttBroker};
use mqtt5::client::MqttClient;
use mqtt5::QoS;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,mqtt5=debug")),
        )
        .init();

    info!("Starting broker bridge demonstration");

    // Start broker 1 (Edge broker)
    let edge_config = BrokerConfig::default()
        .with_bind_address("127.0.0.1:1883".parse::<std::net::SocketAddr>()?)
        .with_max_clients(100);
    let edge_broker = Arc::new(MqttBroker::with_config(edge_config).await?);

    // Start broker 2 (Cloud broker)
    let cloud_config = BrokerConfig::default()
        .with_bind_address("127.0.0.1:1884".parse::<std::net::SocketAddr>()?)
        .with_max_clients(1000);
    let cloud_broker = Arc::new(MqttBroker::with_config(cloud_config).await?);

    info!("Both brokers started successfully");

    // Give brokers time to start
    sleep(Duration::from_millis(100)).await;

    // Configure bridge from edge to cloud
    let _bridge_config = BridgeConfig::new("edge-to-cloud", "127.0.0.1:1884")
        // Forward sensor data from edge to cloud
        .add_topic("sensors/+/data", BridgeDirection::Out, QoS::AtLeastOnce)
        // Receive commands from cloud to edge
        .add_topic("commands/+/device", BridgeDirection::In, QoS::AtLeastOnce)
        // Share status updates bidirectionally
        .add_topic("status/+", BridgeDirection::Both, QoS::AtMostOnce);

    // Add bridge to edge broker
    info!("Setting up bridge from edge to cloud broker");
    // Note: In a real implementation, we'd add a method to MqttBroker to add bridges
    // For now, this is a demonstration of the configuration

    // Create clients to demonstrate message flow
    let edge_client = MqttClient::new("edge-device");
    let cloud_client = MqttClient::new("cloud-service");

    // Connect clients
    edge_client.connect("mqtt://127.0.0.1:1883").await?;
    cloud_client.connect("mqtt://127.0.0.1:1884").await?;

    info!("Clients connected to their respective brokers");

    // Cloud subscribes to sensor data
    cloud_client
        .subscribe("sensors/+/data", |msg| {
            info!(
                "[CLOUD] Received sensor data on {}: {:?}",
                msg.topic,
                String::from_utf8_lossy(&msg.payload)
            );
        })
        .await?;

    // Edge subscribes to commands
    edge_client
        .subscribe("commands/+/device", |msg| {
            info!(
                "[EDGE] Received command on {}: {:?}",
                msg.topic,
                String::from_utf8_lossy(&msg.payload)
            );
        })
        .await?;

    // Give subscriptions time to establish
    sleep(Duration::from_millis(100)).await;

    info!("Starting message exchange demonstration");

    // Edge device publishes sensor data
    info!("[EDGE] Publishing sensor data");
    edge_client
        .publish_qos1("sensors/temp/data", b"25.5C")
        .await?;
    edge_client
        .publish_qos1("sensors/humidity/data", b"65%")
        .await?;

    // Give time for bridge to forward
    sleep(Duration::from_millis(200)).await;

    // Cloud sends command
    info!("[CLOUD] Sending command");
    cloud_client
        .publish_qos1("commands/hvac/device", b"set_temp:22")
        .await?;

    // Keep running for a bit to see messages
    sleep(Duration::from_secs(2)).await;

    info!("Shutting down demonstration");

    // Disconnect clients
    edge_client.disconnect().await?;
    cloud_client.disconnect().await?;

    // Shutdown brokers
    edge_broker.shutdown().await?;
    cloud_broker.shutdown().await?;

    info!("Broker bridge demonstration completed");
    Ok(())
}

// Note: This example demonstrates the bridge configuration.
// The actual bridge connection would need to be integrated into
// the MqttBroker implementation to work fully.
