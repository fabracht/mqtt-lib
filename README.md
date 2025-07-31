# MQTT v5.0 Client Library

[![Rust CI](https://github.com/fabriciobracht/mqtt-lib/workflows/Rust%20CI/badge.svg)](https://github.com/fabriciobracht/mqtt-lib/actions)
[![Security Audit](https://github.com/fabriciobracht/mqtt-lib/workflows/Security%20Audit/badge.svg)](https://github.com/fabriciobracht/mqtt-lib/actions)

A complete MQTT v5.0 client library that provides full protocol compliance with a simple, callback-based API. This library handles all background tasks internally, providing automatic message routing and connection management.

**🚀 Now with full AWS IoT SDK compatibility!** Subscribe methods return `(packet_id, qos)` tuples and a mockable trait interface enables comprehensive unit testing.

## Features

- **Full MQTT v5.0 protocol compliance** - All MQTT 5.0 features implemented
- **Callback-based message handling** - Simple, intuitive API with automatic message routing
- **AWS IoT SDK Compatible** - Subscribe returns `(packet_id, qos)` like Python paho-mqtt
- **Mockable Client Interface** - `MqttClientTrait` enables testing without real brokers
- **Automatic reconnection** - Built-in exponential backoff and session recovery
- **Client-side message queuing** - Handles offline scenarios gracefully
- **TLS/SSL support** - Secure connections with certificate validation
- **Session persistence** - Survives disconnections with clean_start=false
- **Flow control** - Respects broker receive maximum limits
- **Zero-copy message handling** - Efficient memory usage with BeBytes
- **No event loops** - Direct async/await patterns throughout

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
mqtt-v5 = "0.2"
```

## Quick Start

```rust
use mqtt_v5::{MqttClient, QoS};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client with session persistence
    let client = MqttClient::new("my-device-001");

    // Connect to MQTT broker
    client.connect("mqtt://test.mosquitto.org:1883").await?;

    // Subscribe and get packet_id + granted QoS (NEW!)
    let (packet_id, granted_qos) = client.subscribe("sensors/+/data", |msg| {
        println!("Topic: {} Payload: {:?}", msg.topic, msg.payload);
    }).await?;

    println!("Subscribed with packet_id: {}, QoS: {:?}", packet_id, granted_qos);

    // Publish message
    client.publish_qos1("sensors/temp/data", b"25.5").await?;

    // Keep running to receive messages
    tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

    client.disconnect().await?;
    Ok(())
}
```

## Unit Testing with Mock Client

The library provides a `MockMqttClient` for testing without a real broker:

```rust
use mqtt_v5::{MockMqttClient, MqttClientTrait, PublishResult, QoS};

#[tokio::test]
async fn test_my_iot_function() {
    // Create mock client
    let mock = MockMqttClient::new("test-device");

    // Configure mock responses
    mock.set_connect_response(Ok(())).await;
    mock.set_publish_response(Ok(PublishResult::QoS1Or2 { packet_id: 123 })).await;

    // Test your function that accepts MqttClientTrait
    my_iot_function(&mock).await.unwrap();

    // Verify the calls
    let calls = mock.get_calls().await;
    assert_eq!(calls.len(), 2); // connect + publish
}

// Your production code uses the trait
async fn my_iot_function<T: MqttClientTrait>(client: &T) -> Result<(), Box<dyn std::error::Error>> {
    client.connect("mqtt://broker").await?;
    client.publish_qos1("telemetry", b"data").await?;
    Ok(())
}
```

## AWS IoT Integration

The library is fully compatible with AWS IoT requirements:

```rust
use mqtt_v5::{MqttClient, ConnectOptions};
use std::time::Duration;

// Configure for AWS IoT
let mut options = ConnectOptions::new("aws-iot-device-12345");
options.clean_start = false;  // Persistent session
options.keep_alive = Duration::from_secs(30);
options.reconnect_config.enabled = true;
options.reconnect_config.max_attempts = 10;

let client = MqttClient::with_options(options);

// Connect to AWS IoT endpoint
client.connect("mqtts://abcdef123456.iot.us-east-1.amazonaws.com:8883").await?;

// Subscribe returns (packet_id, qos) tuple like Python SDK
let (packet_id, qos) = client.subscribe("$aws/things/+/shadow/update/accepted", |msg| {
    println!("Shadow update accepted: {:?}", msg.payload);
}).await?;
```

## Advanced Configuration

```rust
use mqtt_v5::{ConnectOptions, WillMessage, QoS};
use std::time::Duration;

let mut options = ConnectOptions::new("my-device");

// Session configuration
options.clean_start = false;
options.properties.session_expiry_interval = Some(3600); // 1 hour

// Will message (LWT)
options.will = Some(WillMessage {
    topic: "devices/my-device/status".to_string(),
    payload: b"offline".to_vec(),
    qos: QoS::AtLeastOnce,
    retain: true,
    properties: Default::default(),
});

// Connection limits
options.properties.receive_maximum = Some(10);
options.properties.maximum_packet_size = Some(1024 * 1024); // 1MB
options.properties.topic_alias_maximum = Some(10);

// Authentication
options.username = Some("user".to_string());
options.password = Some(b"pass".to_vec());

// Automatic reconnection
options.reconnect_config.enabled = true;
options.reconnect_config.initial_delay = Duration::from_secs(1);
options.reconnect_config.max_delay = Duration::from_secs(60);
options.reconnect_config.backoff_multiplier = 2.0;
```

## Development

### Prerequisites

- Rust 1.75 or later
- Docker and Docker Compose (for integration testing)

### Building

```bash
cargo build
```

### Testing

```bash
# Generate test certificates (required for TLS tests)
./scripts/generate_test_certs.sh

# Run all tests including unit and mock tests
cargo test

# Run integration tests with test broker
docker-compose up -d
cargo test
docker-compose down
```

### Linting

```bash
cargo clippy -- -D warnings
```

### Benchmarks

```bash
cargo bench
```

## Testing Infrastructure

The project includes Docker Compose configuration for testing:

```bash
# Start test brokers
docker-compose up -d

# Run tests
cargo test

# Stop test brokers
docker-compose down
```

## License

This project is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
