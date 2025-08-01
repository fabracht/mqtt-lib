//! Simple MQTT v5.0 broker example
//!
//! This demonstrates how to create and run a basic MQTT broker.

use mqtt_v5::broker::{BrokerConfig, MqttBroker};
use mqtt_v5::error::Result;
use std::time::Duration;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting MQTT v5.0 broker");

    // Create broker configuration
    let config = BrokerConfig::new()
        .with_bind_address(([127, 0, 0, 1], 1883))
        .with_max_clients(1000)
        .with_session_expiry(Duration::from_secs(3600)) // 1 hour
        .with_maximum_qos(2)
        .with_retain_available(true);

    // Create and start broker
    let mut broker = MqttBroker::with_config(config).await?;

    info!("MQTT broker listening on 127.0.0.1:1883");
    info!("Press Ctrl+C to stop the broker");

    // Handle shutdown signal
    let shutdown_task = tokio::spawn(async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for ctrl-c");
        info!("Shutdown signal received");
    });

    // Run broker in a separate task
    let _broker_task = tokio::spawn(async move {
        if let Err(e) = broker.run().await {
            error!("Broker error: {}", e);
        }
    });

    // Wait for shutdown signal
    shutdown_task.await.expect("Shutdown task failed");

    // Gracefully shutdown broker
    // Note: In a real implementation, you'd want to signal the broker to stop
    info!("Broker shutting down");

    Ok(())
}
