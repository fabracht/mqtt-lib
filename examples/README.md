# MQTT v5.0 Platform Examples

This directory contains examples demonstrating various features of the MQTT v5.0 platform, including both client and broker functionality.

## Getting Started

### Basic Examples

Start with these examples to understand the fundamentals:

1. **simple_broker.rs** - Basic MQTT broker
   ```bash
   cargo run --example simple_broker
   ```
   Starts a broker on localhost:1883 that accepts anonymous connections.

2. **simple_client.rs** - Basic MQTT client with embedded broker
   ```bash
   cargo run --example simple_client
   ```
   Demonstrates pub/sub with an embedded broker (or external if MQTT_BROKER env is set).

3. **complete_system_demo.rs** - Full platform demonstration
   ```bash
   cargo run --example complete_system_demo
   ```
   Shows broker and multiple clients working together with various MQTT features.

## Client Examples

### Basic Client Operations

- **simple_client.rs** - Basic publish/subscribe operations
- **certificate_loading_demo.rs** - TLS certificate authentication
- **shared_subscription_demo.rs** - Load balancing with shared subscriptions

### Advanced Client Features

- **iot_device_simulator.rs** - Simulates IoT devices with telemetry
- **smart_home_hub.rs** - Home automation scenario
- **industrial_sensor_network.rs** - Industrial IoT use case

## Broker Examples

### Basic Broker Setup

- **simple_broker.rs** - Minimal broker setup
- **broker_with_tls.rs** - Secure broker with TLS
- **broker_with_websocket.rs** - WebSocket support
- **broker_all_transports.rs** - TCP, TLS, and WebSocket together

### Advanced Broker Features

- **broker_with_monitoring.rs** - Metrics and monitoring
- **broker_bridge_demo.rs** - Broker-to-broker bridging
- **resource_monitoring.rs** - Resource usage tracking
- **observability_dashboard.rs** - Complete monitoring solution

## Running Examples

### Prerequisites

1. Install Rust (1.70+)
2. Clone the repository
3. Build the project:
   ```bash
   cargo make build
   ```

### Basic Usage

Run any example with:
```bash
cargo run --example <example_name>
```

### Environment Variables

Some examples support configuration via environment variables:

- `MQTT_BROKER` - External broker URL (default: uses embedded broker)
- `RUST_LOG` - Logging level (e.g., `mqtt_v5=debug`)

### Testing Client-Broker Communication

1. Start a broker in one terminal:
   ```bash
   cargo run --example simple_broker
   ```

2. Run clients in other terminals:
   ```bash
   cargo run --example simple_client
   # or
   cargo run --example iot_device_simulator
   ```

### Using External Tools

Test with standard MQTT tools:

```bash
# Subscribe to all topics
mosquitto_sub -h localhost -t '#' -v

# Publish a message
mosquitto_pub -h localhost -t 'test/topic' -m 'Hello!'

# Monitor with MQTT Explorer
# Download from: http://mqtt-explorer.com/
```

## Example Descriptions

### Client Examples

#### simple_client.rs
- Basic connection and pub/sub
- QoS levels demonstration
- Retained messages
- JSON payload handling

#### iot_device_simulator.rs
- Simulates multiple IoT devices
- Periodic telemetry publishing
- Last Will and Testament
- Connection resilience

#### smart_home_hub.rs
- Home automation scenarios
- Device control and monitoring
- Rule-based automation
- State management

#### industrial_sensor_network.rs
- High-frequency sensor data
- Alarm and threshold monitoring
- Data aggregation
- Historical data tracking

#### shared_subscription_demo.rs
- Load balancing across workers
- Shared subscription groups
- Work distribution patterns

#### certificate_loading_demo.rs
- TLS client certificates
- Mutual TLS authentication
- Certificate management

### Broker Examples

#### simple_broker.rs
- Minimal broker setup
- Anonymous connections
- Basic configuration

#### broker_with_tls.rs
- TLS encryption
- Certificate configuration
- Secure connections

#### broker_with_websocket.rs
- WebSocket transport
- Browser client support
- HTTP upgrade handling

#### broker_all_transports.rs
- Multiple transports simultaneously
- TCP, TLS, and WebSocket
- Port configuration

#### broker_with_monitoring.rs
- $SYS topics
- Client statistics
- Message counters
- Performance metrics

#### broker_bridge_demo.rs
- Connect multiple brokers
- Topic mapping
- Message routing
- Bridge configuration

#### resource_monitoring.rs
- Memory usage tracking
- Connection limits
- Bandwidth monitoring
- Resource quotas

#### observability_dashboard.rs
- Prometheus metrics
- Grafana integration
- Real-time dashboards
- Alert configuration

## Common Patterns

### Error Handling

All examples use proper error handling:
```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Example code
    Ok(())
}
```

### Logging

Examples use the `tracing` crate:
```rust
// Enable debug logging
RUST_LOG=mqtt_v5=debug cargo run --example simple_client
```

### Graceful Shutdown

Examples handle Ctrl+C properly:
```rust
tokio::signal::ctrl_c().await?;
println!("Shutting down...");
```

## Next Steps

1. Start with `simple_broker` and `simple_client`
2. Explore `complete_system_demo` for a full overview
3. Try specific examples based on your use case
4. Modify examples for your own applications

## Contributing

Feel free to submit new examples demonstrating:
- Specific use cases
- Integration patterns
- Performance optimizations
- Security configurations

## License

All examples are part of the mqtt-v5 project and follow the same license.