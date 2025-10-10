# Complete MQTT v5.0 Platform

[![Crates.io](https://img.shields.io/crates/v/mqtt5.svg)](https://crates.io/crates/mqtt5)
[![Documentation](https://docs.rs/mqtt5/badge.svg)](https://docs.rs/mqtt5)
[![Rust CI](https://github.com/fabriciobracht/mqtt-lib/workflows/Rust%20CI/badge.svg)](https://github.com/fabriciobracht/mqtt-lib/actions)
[![Security Audit](https://github.com/fabriciobracht/mqtt-lib/workflows/Security%20Audit/badge.svg)](https://github.com/fabriciobracht/mqtt-lib/actions)
[![License](https://img.shields.io/crates/l/mqtt5.svg)](https://github.com/fabriciobracht/mqtt-lib#license)

🚀 **A complete MQTT v5.0 platform featuring both high-performance client library AND full-featured broker implementation - pure Rust, zero unsafe code**

This project provides everything you need for MQTT v5.0 development:
- **Production-ready MQTT v5.0 broker** (Mosquitto replacement)
- **High-performance client library** with AWS IoT compatibility
- **Multiple transport support** (TCP, TLS, WebSocket)
- **Comprehensive testing** with network simulation and property-based testing

## 🏗️ Dual Architecture: Client + Broker

| Component | Use Case | Key Features |
|-----------|----------|--------------|
| **MQTT Broker** | Run your own MQTT infrastructure | TLS, WebSocket, Authentication, Bridging, Monitoring |
| **MQTT Client** | Connect to any MQTT broker | AWS IoT compatible, Auto-reconnect, Mock testing |

## 📦 Installation

### Library Crate
```toml
[dependencies]
mqtt5 = "0.4.1"
```

### CLI Tool
```bash
cargo install mqttv5-cli
```

## 🚀 Quick Start

### Start an MQTT Broker

```rust
use mqtt5::broker::{BrokerConfig, MqttBroker};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create broker with default configuration
    let mut broker = MqttBroker::bind("0.0.0.0:1883").await?;
    
    println!("🚀 MQTT broker running on port 1883");
    
    // Run until shutdown
    broker.run().await?;
    Ok(())
}
```

### Connect a Client

```rust
use mqtt5::{MqttClient, QoS};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = MqttClient::new("my-device-001");

    // Connect to your broker (supports multiple transports)
    client.connect("mqtt://localhost:1883").await?;       // TCP
    // client.connect("mqtts://localhost:8883").await?;   // TLS
    // client.connect("ws://localhost:8080/mqtt").await?; // WebSocket
    
    // Subscribe with callback
    client.subscribe("sensors/+/data", |msg| {
        println!("📧 {}: {}", msg.topic, String::from_utf8_lossy(&msg.payload));
    }).await?;
    
    // Publish a message
    client.publish("sensors/temp/data", b"25.5°C").await?;
    
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    Ok(())
}
```

## 🚀 Command Line Interface (mqttv5)

**Superior CLI tool that replaces mosquitto_pub, mosquitto_sub, and mosquitto with unified ergonomics:**

### Installation

```bash
# Install from crates.io
cargo install mqttv5-cli

# Or build from source
git clone https://github.com/fabriciobracht/mqtt-lib
cd mqtt-lib
cargo build --release -p mqttv5-cli
```

### Usage Examples

```bash
# Start a broker (replaces mosquitto daemon)
mqttv5 broker --host 0.0.0.0:1883

# Publish a message (replaces mosquitto_pub)
mqttv5 pub --topic "sensors/temperature" --message "23.5"

# Subscribe to topics (replaces mosquitto_sub)
mqttv5 sub --topic "sensors/+" --verbose

# Use different transports with --url flag
mqttv5 pub --url mqtt://localhost:1883 --topic test --message "TCP"
mqttv5 pub --url mqtts://localhost:8883 --topic test --message "TLS"
mqttv5 pub --url ws://localhost:8080/mqtt --topic test --message "WebSocket"

# Smart prompting when arguments are missing
mqttv5 pub
# ? MQTT topic › sensors/
# ? Message content › Hello World!
# ? Quality of Service level › ● 0 (At most once)
```

### Key CLI Advantages

- **🎯 Unified interface** - One command instead of mosquitto_pub/mosquitto_sub/mosquitto
- **🧠 Smart prompting** - Guides users instead of showing walls of help text  
- **✅ Input validation** - Catches errors early with helpful suggestions
- **📝 Descriptive flags** - `--topic` instead of `-t`, with short aliases available
- **🔄 Interactive & non-interactive** - Works great for both humans and scripts

## 🎯 Why This Platform?

### ✅ Production-Ready Broker
- **Mosquitto replacement** with better performance and memory usage
- **Multiple transports**: TCP, TLS, WebSocket in a single binary
- **Built-in authentication**: Username/password, file-based, bcrypt
- **Resource monitoring**: Connection limits, rate limiting, memory tracking
- **Self-contained**: No external dependencies (Redis, PostgreSQL, etc.)

### ✅ High-Performance Client
- **Pure Rust implementation**: No FFI, no unsafe code
- **AWS IoT compatibility**: Works seamlessly with AWS IoT Core
- **Zero-copy operations**: Efficient memory usage with BeBytes
- **Direct async/await**: Clean Rust async patterns
- **Comprehensive testing**: Property-based tests and network simulation

## 📦 Broker Features

### Core MQTT v5.0 Broker
- ✅ **Full MQTT v5.0 compliance** - All packet types, properties, reason codes
- ✅ **Multiple QoS levels** - QoS 0, 1, 2 with proper flow control
- ✅ **Session persistence** - Clean start, session expiry, message queuing
- ✅ **Retained messages** - Persistent message storage and retrieval
- ✅ **Shared subscriptions** - Load balancing across multiple clients
- ✅ **Will messages** - Last Will and Testament (LWT) support

### Transport & Security
- ✅ **TCP transport** - Standard MQTT over TCP on port 1883
- ✅ **TLS/SSL transport** - Secure MQTT over TLS on port 8883
- ✅ **WebSocket transport** - MQTT over WebSocket for browsers/firewalls
- ✅ **Certificate authentication** - Client certificate validation
- ✅ **Username/password authentication** - File-based user management

### Advanced Features
- ✅ **Broker-to-broker bridging** - Connect multiple broker instances
- ✅ **Resource monitoring** - $SYS topics, connection metrics, rate limiting
- ✅ **Hot configuration reload** - Update settings without restart
- ✅ **Storage backends** - File-based or in-memory persistence
- ✅ **ACL (Access Control Lists)** - Fine-grained topic permissions

### Performance & Scalability
- ✅ **High concurrency** - Handle 10,000+ concurrent connections
- ✅ **Connection pooling** - Efficient resource reuse
- ✅ **Optimized routing** - Fast topic matching and message delivery
- ✅ **Memory monitoring** - Prevent resource exhaustion attacks
- ✅ **Rate limiting** - Per-client message and bandwidth limits

## 📦 Client Features

### Core MQTT v5.0 Client
- ✅ **Full MQTT v5.0 protocol compliance** - All MQTT 5.0 features implemented
- ✅ **Callback-based message handling** - Simple, intuitive API with automatic message routing
- ✅ **AWS IoT SDK Compatible** - Subscribe returns `(packet_id, qos)` like Python paho-mqtt
- ✅ **Automatic reconnection** - Built-in exponential backoff and session recovery
- ✅ **Client-side message queuing** - Handles offline scenarios gracefully

### Transport & Connectivity
- ✅ **Certificate loading from bytes** - Load TLS certificates from memory (PEM/DER formats)
- ✅ **WebSocket transport** - MQTT over WebSocket for browsers and firewall-restricted environments
- ✅ **TLS/SSL support** - Secure connections with certificate validation
- ✅ **Session persistence** - Survives disconnections with clean_start=false

### Testing & Development
- ✅ **Mockable Client Interface** - `MqttClientTrait` enables testing without real brokers
- ✅ **Comprehensive property testing** - 29 property-based tests ensuring robustness
- ✅ **CLI Integration Testing** - End-to-end tests with real broker verification
- ✅ **Flow control** - Respects broker receive maximum limits
- ✅ **Zero-copy message handling** - Efficient memory usage with BeBytes

## 🚦 Advanced Broker Configuration

### Multi-Transport Broker

```rust
use mqtt_v5::broker::{BrokerConfig, TlsConfig, WebSocketConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = BrokerConfig::default()
        // TCP on port 1883
        .with_bind_address("0.0.0.0:1883".parse()?)
        // TLS on port 8883
        .with_tls(
            TlsConfig::new("certs/server.crt".into(), "certs/server.key".into())
                .with_ca_file("certs/ca.crt".into())
                .with_bind_address("0.0.0.0:8883".parse()?)
        )
        // WebSocket on port 8080
        .with_websocket(
            WebSocketConfig::default()
                .with_bind_address("0.0.0.0:8080".parse()?)
                .with_path("/mqtt")
        )
        .with_max_clients(10_000);

    let mut broker = MqttBroker::with_config(config).await?;

    println!("🚀 Multi-transport MQTT broker running:");
    println!("  📡 TCP:       mqtt://localhost:1883");
    println!("  🔒 TLS:       mqtts://localhost:8883");
    println!("  🌐 WebSocket: ws://localhost:8080/mqtt");

    broker.run().await?;
    Ok(())
}
```

### Broker with Authentication

```rust
use mqtt_v5::broker::{BrokerConfig, AuthConfig, AuthMethod};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let auth_config = AuthConfig {
        allow_anonymous: false,
        password_file: Some("users.txt".into()),
        auth_method: AuthMethod::Password,
        auth_data: None,
    };
    
    let config = BrokerConfig::default()
        .with_bind_address("0.0.0.0:1883".parse()?)
        .with_auth(auth_config);

    let mut broker = MqttBroker::with_config(config).await?;
    broker.run().await?;
    Ok(())
}
```

### Broker Bridging

```rust
use mqtt_v5::broker::bridge::{BridgeConfig, BridgeDirection};
use mqtt_v5::QoS;

// Connect two brokers together
let bridge_config = BridgeConfig::new("edge-to-cloud", "cloud-broker:1883")
    // Forward sensor data from edge to cloud
    .add_topic("sensors/+/data", BridgeDirection::Out, QoS::AtLeastOnce)
    // Receive commands from cloud to edge  
    .add_topic("commands/+/device", BridgeDirection::In, QoS::AtLeastOnce)
    // Bidirectional health monitoring
    .add_topic("health/+/status", BridgeDirection::Both, QoS::AtMostOnce);

// Add bridge to broker (broker handles connection management)
// broker.add_bridge(bridge_config).await?;
```

## 🧪 Testing Support

### Unit Testing with Mock Client

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

## ☁️ AWS IoT Support

The client library includes AWS IoT compatibility features:

```rust
use mqtt_v5::{MqttClient, ConnectOptions};
use std::time::Duration;

// AWS IoT endpoint detection and connection handling
let client = MqttClient::new("aws-iot-device-12345");

// Connect to AWS IoT endpoint (automatically detects AWS IoT and optimizes connection)
client.connect("mqtts://abcdef123456.iot.us-east-1.amazonaws.com:8883").await?;

// Subscribe returns (packet_id, qos) tuple for compatibility
let (packet_id, qos) = client.subscribe("$aws/things/device-123/shadow/update/accepted", |msg| {
    println!("Shadow update accepted: {:?}", msg.payload);
}).await?;

// AWS IoT topic validation prevents publishing to reserved topics
use mqtt_v5::validation::namespace::NamespaceValidator;

let validator = NamespaceValidator::aws_iot().with_device_id("device-123");

// This will succeed - device can update its own shadow
client.publish("$aws/things/device-123/shadow/update", shadow_data).await?;

// This will be rejected - device cannot publish to shadow response topics
// client.publish("$aws/things/device-123/shadow/update/accepted", data).await?; // Error!
```

Key AWS IoT features:
- **Endpoint detection**: Automatically detects AWS IoT endpoints and optimizes connection behavior
- **Topic validation**: Built-in validation for AWS IoT topic restrictions and limits
- **ALPN support**: TLS configuration with AWS IoT ALPN protocol support
- **Certificate loading**: Load client certificates from bytes (PEM/DER formats)
- **SDK compatibility**: Subscribe method returns `(packet_id, qos)` tuple like other AWS SDKs

## 🛠️ Development & Building

### Prerequisites

- Rust 1.82 or later
- cargo-make (`cargo install cargo-make`)

### Quick Setup

```bash
# Clone the repository
git clone https://github.com/fabriciobracht/mqtt-lib.git
cd mqtt-lib

# Install development tools and git hooks
./scripts/install-hooks.sh

# Run all CI checks locally (MUST pass before pushing)
cargo make ci-verify
```

### Available Commands

```bash
# Development
cargo make build          # Build the project  
cargo make build-release  # Build optimized release version
cargo make test           # Run all tests
cargo make fmt            # Format code
cargo make clippy         # Run linter

# CI/CD  
cargo make ci-verify      # Run ALL CI checks (must pass before push)
cargo make pre-commit     # Run before committing (fmt + clippy + test)

# Examples (use raw cargo for specific targets)
cargo run --example simple_broker           # Start basic broker
cargo run --example broker_with_tls         # TLS-enabled broker
cargo run --example broker_with_websocket   # WebSocket-enabled broker
cargo run --example broker_all_transports   # Broker with all transports (TCP/TLS/WebSocket)
cargo run --example broker_bridge_demo      # Broker bridging demo

# Benchmarks (use raw cargo for specific targets)
cargo bench --bench broker_performance      # Broker performance tests
cargo bench --bench mqtt_benchmarks         # Core MQTT benchmarks
```

### Testing

```bash
# Generate test certificates (required for TLS tests)
./scripts/generate_test_certs.sh

# Run unit tests (fast)
cargo make test-fast

# Run all tests including integration tests
cargo make test

# Run specific test suites (use raw cargo for specific targets)
cargo test --test broker_performance_tests
cargo test --test connection_pool_performance
```

## 🏗️ Architecture

This project follows modern Rust async patterns:

### Design Principles
- **Direct async methods** for all operations (no indirection)
- **Shared state** via `Arc<RwLock<T>>` (no message passing)
- **Zero-copy operations** where possible
- **Resource efficiency** with connection pooling and buffer reuse

## 📊 Performance

The broker is designed for high performance:

- **10,000+ concurrent connections** on modest hardware
- **Low memory footprint** with connection pooling
- **Fast topic matching** with optimized routing algorithms
- **Zero-copy message processing** where possible
- **Comprehensive benchmarking** suite for performance validation

## 🔐 Security

Security is built-in, not bolted-on:

- **TLS 1.2+ support** with certificate validation
- **Username/password authentication** with bcrypt hashing
- **Access Control Lists (ACL)** for fine-grained permissions
- **Rate limiting** to prevent DoS attacks
- **Resource monitoring** to prevent resource exhaustion
- **Client certificate authentication** for mutual TLS

## 📄 License

This project is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## 📚 Documentation

- [Architecture Overview](ARCHITECTURE.md) - System design and principles
- [Broker Configuration](docs/broker/configuration.md) - Complete config reference
- [Authentication Guide](docs/broker/authentication.md) - Security setup
- [Deployment Guide](docs/broker/deployment.md) - Production deployment
- [API Documentation](https://docs.rs/mqtt-v5) - Complete API reference

---

**Built with ❤️ in Rust. One reliable state machine.**