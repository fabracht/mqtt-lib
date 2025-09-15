//! Complete MQTT System Demo
//!
//! This example demonstrates the full capabilities of the MQTT v5.0 platform,
//! showing both broker and client features working together:
//!
//! - Start a full-featured MQTT broker
//! - Connect multiple clients
//! - Demonstrate QoS levels
//! - Show retained messages
//! - Demonstrate last will
//! - Show shared subscriptions
//! - Monitor broker statistics
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example complete_system_demo
//! ```

use mqtt5::{
    broker::{BrokerConfig, MqttBroker},
    ConnectOptions, MqttClient, PublishOptions, QoS,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::{interval, sleep};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 MQTT v5.0 Complete System Demo");
    println!("==================================\n");

    // Configure and start broker
    println!("📡 Starting MQTT broker...");
    let mut config = BrokerConfig {
        bind_address: "127.0.0.1:1883".parse()?,
        max_clients: 1000,
        ..Default::default()
    };
    config.auth_config.allow_anonymous = true; // For demo simplicity

    let mut broker = MqttBroker::with_config(config).await?;

    println!("✅ Broker listening on mqtt://localhost:1883");

    // Run broker in background
    let broker_handle = tokio::spawn(async move {
        if let Err(e) = broker.run().await {
            eprintln!("❌ Broker error: {e}");
        }
    });

    // Give broker time to start
    sleep(Duration::from_millis(100)).await;

    // Create multiple clients for different purposes
    println!("\n📱 Creating clients...");

    // 1. Sensor Publisher Client
    let sensor_client = create_sensor_client().await?;
    println!("✅ Sensor client connected");

    // 2. Data Processor Client (using shared subscription)
    let processor1 = create_processor_client("processor-1").await?;
    let processor2 = create_processor_client("processor-2").await?;
    println!("✅ Data processors connected (shared subscription)");

    // 3. Dashboard Client (monitors everything)
    let dashboard_client = create_dashboard_client().await?;
    println!("✅ Dashboard client connected");

    // 4. Control Client (sends commands)
    let control_client = create_control_client().await?;
    println!("✅ Control client connected");

    // Demonstrate retained messages
    println!("\n📌 Publishing retained status messages...");
    control_client
        .publish_retain("system/status", b"ONLINE")
        .await?;
    control_client
        .publish_retain("system/version", b"1.0.0")
        .await?;

    // Start sensor simulation
    let sensor_handle = tokio::spawn(async move {
        simulate_sensors(sensor_client).await;
    });

    // Start control loop
    let control_handle = tokio::spawn(async move {
        control_loop(control_client).await;
    });

    // Monitor for 30 seconds
    println!("\n⏰ Running demo for 30 seconds...");
    println!("   Watch the messages flow between clients!\n");

    sleep(Duration::from_secs(30)).await;

    // Graceful shutdown
    println!("\n🛑 Shutting down demo...");

    sensor_handle.abort();
    control_handle.abort();

    dashboard_client.disconnect().await?;
    processor1.disconnect().await?;
    processor2.disconnect().await?;

    broker_handle.abort();

    println!("✅ Demo completed successfully!");

    Ok(())
}

async fn create_sensor_client() -> Result<MqttClient, Box<dyn std::error::Error>> {
    let options = ConnectOptions::new("sensor-device-001")
        .with_keep_alive(Duration::from_secs(30))
        .with_clean_start(false); // Persistent session

    let client = MqttClient::with_options(options);
    client.connect("mqtt://localhost:1883").await?;

    Ok(client)
}

async fn create_processor_client(id: &str) -> Result<MqttClient, Box<dyn std::error::Error>> {
    let client = MqttClient::new(id);
    client.connect("mqtt://localhost:1883").await?;

    let id_string = id.to_string();
    // Subscribe to shared subscription for load balancing
    client
        .subscribe("$share/processors/sensors/+/data", move |msg| {
            println!(
                "🔧 [{}] Processing: {} = {}",
                id_string,
                msg.topic,
                String::from_utf8_lossy(&msg.payload)
            );

            // Simulate processing time
            std::thread::sleep(Duration::from_millis(100));
        })
        .await?;

    Ok(client)
}

async fn create_dashboard_client() -> Result<MqttClient, Box<dyn std::error::Error>> {
    let stats = Arc::new(RwLock::new(DashboardStats::default()));
    let client = MqttClient::new("dashboard-001");
    client.connect("mqtt://localhost:1883").await?;

    // Subscribe to all sensor data
    let stats_clone = stats.clone();
    client
        .subscribe("sensors/+/data", move |msg| {
            let mut s = stats_clone.blocking_write();
            s.messages_received += 1;

            if let Ok(value) = String::from_utf8_lossy(&msg.payload).parse::<f64>() {
                s.last_value = value;
                s.total_value += value;
            }
        })
        .await?;

    // Subscribe to system status
    client
        .subscribe("system/+", |msg| {
            println!(
                "📊 Dashboard: {} = {}",
                msg.topic,
                String::from_utf8_lossy(&msg.payload)
            );
        })
        .await?;

    // Subscribe to commands
    client
        .subscribe("commands/+", |msg| {
            println!(
                "🎮 Dashboard: Command {} = {}",
                msg.topic,
                String::from_utf8_lossy(&msg.payload)
            );
        })
        .await?;

    // Periodic stats display
    let stats_display = stats.clone();
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(5));
        loop {
            ticker.tick().await;
            let s = stats_display.read().await;
            if s.messages_received > 0 {
                let avg = s.total_value / s.messages_received as f64;
                println!(
                    "📈 Stats: {} messages, avg value: {:.2}, last: {:.2}",
                    s.messages_received, avg, s.last_value
                );
            }
        }
    });

    Ok(client)
}

async fn create_control_client() -> Result<MqttClient, Box<dyn std::error::Error>> {
    let client = MqttClient::new("control-center");
    client.connect("mqtt://localhost:1883").await?;

    // Subscribe to sensor status
    client
        .subscribe("sensors/status", |msg| {
            println!(
                "🎛️  Control: Sensor status = {}",
                String::from_utf8_lossy(&msg.payload)
            );
        })
        .await?;

    Ok(client)
}

async fn simulate_sensors(client: MqttClient) {
    let sensors = vec![
        ("temperature", 20.0, 30.0),
        ("humidity", 40.0, 80.0),
        ("pressure", 1000.0, 1020.0),
    ];

    let mut ticker = interval(Duration::from_secs(2));
    let mut counter = 0;

    loop {
        ticker.tick().await;
        counter += 1;

        for (sensor_type, min, max) in &sensors {
            // Generate random value
            let value = min + (max - min) * rand::random::<f64>();
            let topic = format!("sensors/{sensor_type}/data");

            // Alternate between QoS levels
            let qos = match counter % 3 {
                0 => QoS::AtMostOnce,
                1 => QoS::AtLeastOnce,
                _ => QoS::ExactlyOnce,
            };

            let options = PublishOptions {
                qos,
                ..Default::default()
            };

            if let Err(e) = client
                .publish_with_options(&topic, format!("{value:.2}").as_bytes(), options)
                .await
            {
                eprintln!("Failed to publish sensor data: {e}");
                return;
            }
        }

        // Periodically publish status
        if counter % 5 == 0 {
            let _ = client.publish_qos1("sensors/status", b"ACTIVE").await;
        }
    }
}

async fn control_loop(client: MqttClient) {
    let commands = [
        ("calibrate", "START_CALIBRATION"),
        ("reset", "RESET_COUNTERS"),
        ("report", "GENERATE_REPORT"),
    ];

    let mut ticker = interval(Duration::from_secs(10));
    let mut index = 0;

    loop {
        ticker.tick().await;

        let (cmd_type, cmd_value) = commands[index % commands.len()];
        let topic = format!("commands/{cmd_type}");

        if let Err(e) = client.publish_qos2(&topic, cmd_value.as_bytes()).await {
            eprintln!("Failed to send command: {e}");
            return;
        }

        println!("🎯 Sent command: {topic} = {cmd_value}");
        index += 1;
    }
}

#[derive(Default)]
struct DashboardStats {
    messages_received: u64,
    last_value: f64,
    total_value: f64,
}
