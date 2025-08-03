# MQTT v5.0 Client Documentation

Comprehensive documentation for the MQTT v5.0 client library.

## Quick Navigation

- [Getting Started](getting-started.md) - Quick start guide and basic examples
- [API Reference](api-reference.md) - Complete API documentation
- [AWS IoT Integration](aws-iot.md) - Using the client with AWS IoT Core
- [Advanced Features](advanced-features.md) - QoS, retained messages, will messages
- [Testing Guide](testing.md) - Unit testing with mock client
- [Troubleshooting](troubleshooting.md) - Common issues and solutions

## Client Features

### Core MQTT v5.0 Features
- ✅ Full MQTT v5.0 protocol compliance
- ✅ All QoS levels (0, 1, 2)
- ✅ Retained messages
- ✅ Will messages (LWT)
- ✅ Clean start and session persistence
- ✅ Topic aliases
- ✅ Message properties
- ✅ Flow control

### Transport Support
- ✅ TCP connections
- ✅ TLS/SSL with certificate support
- ✅ WebSocket (WS and WSS)
- ✅ Custom transport implementations

### Advanced Features
- ✅ Automatic reconnection with exponential backoff
- ✅ Callback-based message handling
- ✅ Connection event notifications
- ✅ AWS IoT SDK compatibility
- ✅ Mock client for testing
- ✅ Zero-copy message handling

## Example Usage

```rust
use mqtt_v5::{MqttClient, QoS};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client
    let client = MqttClient::new("my-device");
    
    // Connect to broker
    client.connect("mqtt://broker.example.com:1883").await?;
    
    // Subscribe with callback
    client.subscribe("sensors/+/data", |msg| {
        println!("Received: {} on {}", 
            String::from_utf8_lossy(&msg.payload),
            msg.topic
        );
    }).await?;
    
    // Publish message
    client.publish_qos1("sensors/temp/data", b"25.5°C").await?;
    
    Ok(())
}
```

## Architecture

The client follows a **NO EVENT LOOPS** architecture:

- Direct async/await for all operations
- No command channels or actor patterns
- Efficient background tasks for packet reading and keep-alive
- Shared state management with Arc<RwLock<T>>

See [ARCHITECTURE.md](../../ARCHITECTURE.md) for detailed architectural principles.

## Next Steps

- [Getting Started Guide](getting-started.md) - Start building with the client
- [API Reference](api-reference.md) - Detailed API documentation
- [Examples](../../examples/) - Complete working examples