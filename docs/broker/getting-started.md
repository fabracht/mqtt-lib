# MQTT Broker

Running the MQTT v5.0 broker implementation.

## Basic Setup

### Basic Broker

The simplest way to start an MQTT broker:

```rust
use mqtt_v5::broker::MqttBroker;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Start broker on default port 1883
    let mut broker = MqttBroker::bind("0.0.0.0:1883").await?;
    
    println!("🚀 MQTT broker running on port 1883");
    
    // Run until shutdown
    broker.run().await?;
    Ok(())
}
```

### Testing Your Broker

Once your broker is running, you can test it with any MQTT client or use the built-in client:

```rust
use mqtt_v5::MqttClient;

#[tokio::main] 
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = MqttClient::new("test-client");
    
    // Connect to your broker
    client.connect("mqtt://localhost:1883").await?;
    
    // Subscribe to a topic
    client.subscribe("test/topic", |msg| {
        println!("Received: {}", String::from_utf8_lossy(&msg.payload));
    }).await?;
    
    // Publish a message
    client.publish("test/topic", b"Hello from broker!").await?;
    
    // Keep running for a bit
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    Ok(())
}
```

## Reference

- [Configuration Reference](configuration.md) - All configuration options
- [Authentication Setup](authentication.md) - Broker security
- [Transport Configuration](transports.md) - TLS and WebSocket
- [Production Deployment](deployment.md) - Production deployment

## Examples

Check the `examples/` directory for complete working examples:

- `examples/simple_broker.rs` - Basic broker setup
- `examples/broker_with_tls.rs` - TLS-enabled broker
- `examples/broker_with_websocket.rs` - WebSocket support
- `examples/broker_all_transports.rs` - Multi-transport broker
- `examples/broker_with_monitoring.rs` - Monitoring and metrics

## Key Features

✅ **Full MQTT v5.0 compliance** - All packet types, properties, reason codes  
✅ **Multiple transports** - TCP, TLS, WebSocket in one binary  
✅ **High performance** - Handle 10,000+ concurrent connections  
✅ **Resource monitoring** - Built-in connection limits and rate limiting  
✅ **Broker bridging** - Connect multiple broker instances  
✅ **Authentication** - Username/password, TLS certificates  
✅ **Persistence** - Session and message storage  
✅ **Shared subscriptions** - Load balancing across clients  

See the [Configuration Reference](configuration.md) for detailed setup of each feature.