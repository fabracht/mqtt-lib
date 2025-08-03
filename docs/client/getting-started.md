# MQTT Client

Using the MQTT v5.0 client library.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
mqtt-v5 = "0.3.0"
tokio = { version = "1", features = ["full"] }
```

## Basic Connection

### Simple Client

```rust
use mqtt_v5::MqttClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client with unique ID
    let client = MqttClient::new("my-device-001");
    
    // Connect to broker
    client.connect("mqtt://test.mosquitto.org:1883").await?;
    
    println!("Connected to broker!");
    
    // Disconnect when done
    client.disconnect().await?;
    Ok(())
}
```

### Connection with Options

```rust
use mqtt_v5::{MqttClient, ConnectOptions};
use std::time::Duration;

let mut options = ConnectOptions::new("my-device-001");
options.keep_alive = Duration::from_secs(30);
options.clean_start = false;  // Resume previous session
options.username = Some("user".to_string());
options.password = Some("pass".to_string());

let client = MqttClient::with_options(options);
client.connect("mqtt://broker.example.com:1883").await?;
```

## Publishing Messages

### Basic Publish (QoS 0)

```rust
// Fire and forget
client.publish("sensors/temperature", b"25.5").await?;
```

### QoS 1 - At Least Once

```rust
// Get packet ID for tracking
let packet_id = client.publish_qos1("sensors/humidity", b"65%").await?;
println!("Published with packet ID: {}", packet_id);
```

### QoS 2 - Exactly Once

```rust
// Guaranteed exactly-once delivery
let packet_id = client.publish_qos2("alerts/critical", b"System failure").await?;
```

### Publish with Options

```rust
use mqtt_v5::{PublishOptions, QoS};

let mut options = PublishOptions::default();
options.qos = QoS::AtLeastOnce;
options.retain = true;
options.properties.message_expiry_interval = Some(3600); // 1 hour

client.publish_with_options(
    "status/device001",
    b"online",
    options
).await?;
```

## Subscribing to Topics

### Basic Subscribe

```rust
// Subscribe with callback
client.subscribe("sensors/+/data", |msg| {
    println!("Topic: {}", msg.topic);
    println!("Payload: {}", String::from_utf8_lossy(&msg.payload));
    println!("QoS: {:?}", msg.qos);
    println!("Retained: {}", msg.retain);
}).await?;
```

### Multiple Subscriptions

```rust
// Subscribe to multiple topics
client.subscribe("sensors/temperature", |msg| {
    let temp = String::from_utf8_lossy(&msg.payload);
    println!("Temperature: {}", temp);
}).await?;

client.subscribe("sensors/humidity", |msg| {
    let humidity = String::from_utf8_lossy(&msg.payload);
    println!("Humidity: {}", humidity);
}).await?;

client.subscribe("alerts/#", |msg| {
    println!("Alert: {} - {}", msg.topic, String::from_utf8_lossy(&msg.payload));
}).await?;
```

### Processing Messages

```rust
use serde_json::Value;

client.subscribe("sensors/json/data", |msg| {
    // Parse JSON payload
    if let Ok(json) = serde_json::from_slice::<Value>(&msg.payload) {
        println!("Sensor data: {:?}", json);
        
        if let Some(temp) = json["temperature"].as_f64() {
            if temp > 30.0 {
                println!("High temperature alert: {}°C", temp);
            }
        }
    }
}).await?;
```

## Connection Events

### Monitor Connection Status

```rust
use mqtt_v5::ConnectionEvent;

client.on_connection_event(|event| {
    match event {
        ConnectionEvent::Connected { session_present } => {
            println!("Connected! Session present: {}", session_present);
        }
        ConnectionEvent::Disconnected { reason } => {
            println!("Disconnected: {:?}", reason);
        }
        ConnectionEvent::Reconnecting { attempt } => {
            println!("Reconnecting... attempt #{}", attempt);
        }
        ConnectionEvent::ReconnectFailed { error } => {
            println!("Reconnection failed: {}", error);
        }
    }
}).await?;
```

## Transport Options

### TLS/SSL Connection

```rust
// Connect with TLS
client.connect("mqtts://broker.example.com:8883").await?;

// With client certificates
use mqtt_v5::ConnectOptions;

let mut options = ConnectOptions::new("secure-client");
// Configure TLS options...

let client = MqttClient::with_options(options);
client.connect("mqtts://broker.example.com:8883").await?;
```

### WebSocket Connection

```rust
// Plain WebSocket
client.connect("ws://broker.example.com:8080/mqtt").await?;

// Secure WebSocket
client.connect("wss://broker.example.com:8443/mqtt").await?;
```

## Error Handling

### Connection Errors

```rust
match client.connect("mqtt://broker.example.com:1883").await {
    Ok(_) => println!("Connected successfully"),
    Err(mqtt_v5::MqttError::Network(e)) => {
        eprintln!("Network error: {}", e);
    }
    Err(mqtt_v5::MqttError::Protocol(code)) => {
        eprintln!("Protocol error: {:?}", code);
    }
    Err(e) => {
        eprintln!("Connection failed: {}", e);
    }
}
```

### Automatic Reconnection

```rust
use mqtt_v5::ConnectOptions;
use std::time::Duration;

let mut options = ConnectOptions::new("resilient-client");
options.reconnect_config.enabled = true;
options.reconnect_config.initial_delay = Duration::from_secs(1);
options.reconnect_config.max_delay = Duration::from_secs(60);
options.reconnect_config.max_attempts = None; // Unlimited

let client = MqttClient::with_options(options);
```

## Complete Example

```rust
use mqtt_v5::{MqttClient, ConnectionEvent, QoS};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    // Create client
    let client = MqttClient::new("weather-station-001");
    
    // Monitor connection
    client.on_connection_event(|event| {
        match event {
            ConnectionEvent::Connected { .. } => {
                println!("✅ Connected to broker");
            }
            ConnectionEvent::Disconnected { reason } => {
                println!("❌ Disconnected: {:?}", reason);
            }
            _ => {}
        }
    }).await?;
    
    // Connect
    println!("Connecting to broker...");
    client.connect("mqtt://test.mosquitto.org:1883").await?;
    
    // Subscribe to commands
    client.subscribe("weather/station001/commands", |msg| {
        let command = String::from_utf8_lossy(&msg.payload);
        println!("Received command: {}", command);
    }).await?;
    
    // Publish sensor data periodically
    for i in 0..10 {
        let temperature = 20.0 + (i as f32) * 0.5;
        let payload = format!("{{\"temp\": {}, \"unit\": \"C\"}}", temperature);
        
        client.publish_qos1(
            "weather/station001/temperature",
            payload.as_bytes()
        ).await?;
        
        println!("Published temperature: {}°C", temperature);
        sleep(Duration::from_secs(5)).await;
    }
    
    // Graceful shutdown
    client.disconnect().await?;
    println!("Disconnected gracefully");
    
    Ok(())
}
```

## Next Steps

- [API Reference](api-reference.md) - Complete API documentation
- [Advanced Features](advanced-features.md) - QoS, retained messages, will messages
- [AWS IoT Integration](aws-iot.md) - Connect to AWS IoT Core
- [Testing Guide](testing.md) - Unit testing with mock client