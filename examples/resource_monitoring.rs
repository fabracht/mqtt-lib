//! Example demonstrating MQTT broker resource monitoring
//!
//! This example shows how to monitor broker resources and enforce limits.

use mqtt5::broker::{BrokerConfig, MqttBroker};
use mqtt5::client::MqttClient;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create broker with resource limits
    let config = BrokerConfig::default()
        .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>()?)
        .with_max_clients(5); // Very low limit for demonstration

    let mut broker = MqttBroker::with_config(config).await?;
    let resource_monitor = broker.resource_monitor();
    let broker_addr = broker.local_addr().expect("Failed to get broker address");

    // Start broker in background
    let broker_handle = tokio::spawn(async move { broker.run().await });

    // Give broker time to start
    sleep(Duration::from_millis(100)).await;

    // Create several clients to test limits
    let mut clients = Vec::new();

    info!("Testing connection limits...");

    for i in 0..8 {
        let client_id = format!("client-{i}");
        let client = MqttClient::new(&client_id);

        match client.connect(&broker_addr.to_string()).await {
            Ok(()) => {
                info!("Client {} connected successfully", client_id);
                clients.push(client);

                // Show resource statistics
                let stats = resource_monitor.get_stats().await;
                info!(
                    "Resource stats: {}/{} connections ({:.1}% utilization), {} unique IPs",
                    stats.current_connections,
                    stats.max_connections,
                    stats.connection_utilization(),
                    stats.unique_ips
                );
            }
            Err(e) => {
                warn!("Client {} failed to connect: {}", client_id, e);
            }
        }

        sleep(Duration::from_millis(100)).await;
    }

    info!("Testing message rate limits...");

    // Test message rate limiting with the first client
    if let Some(client) = clients.first() {
        for i in 0..20 {
            match client
                .publish(&format!("test/message/{i}"), "test payload")
                .await
            {
                Ok(_) => {
                    info!("Message {} published successfully", i);
                }
                Err(e) => {
                    warn!("Message {} failed to publish: {}", i, e);
                }
            }

            // Small delay between messages
            sleep(Duration::from_millis(50)).await;
        }
    }

    // Show final statistics
    let final_stats = resource_monitor.get_stats().await;
    info!(
        "Final stats: {} connections, {} messages, {} bytes processed",
        final_stats.current_connections, final_stats.total_messages, final_stats.total_bytes
    );

    // Clean shutdown
    info!("Shutting down...");
    for client in clients {
        let _ = client.disconnect().await;
    }

    broker_handle.abort();

    Ok(())
}
