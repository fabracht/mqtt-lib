# Broker-to-Broker Bridging

Connect multiple MQTT brokers together to create distributed messaging architectures.

## Overview

Broker bridging enables:

- **Hierarchical Architectures**: Edge brokers connecting to cloud brokers
- **Geographic Distribution**: Connect brokers across regions
- **Network Segmentation**: Bridge isolated networks securely
- **Load Distribution**: Spread clients across multiple brokers
- **High Availability**: Failover between broker instances

## Basic Bridge Configuration

### Simple One-Way Bridge

```rust
use mqtt_v5::broker::bridge::{BridgeConfig, BridgeDirection};
use mqtt_v5::QoS;

// Forward sensor data from edge to cloud
let bridge_config = BridgeConfig::new("edge-to-cloud", "cloud-broker.example.com:1883")
    .add_topic("sensors/#", BridgeDirection::Out, QoS::AtLeastOnce);

// Add bridge to broker (implementation varies)
// broker.add_bridge(bridge_config).await?;
```

### Bidirectional Bridge

```rust
let bridge_config = BridgeConfig::new("site-bridge", "central-broker:1883")
    // Send local data to central
    .add_topic("site/+/data", BridgeDirection::Out, QoS::AtLeastOnce)
    // Receive commands from central
    .add_topic("commands/site/#", BridgeDirection::In, QoS::AtLeastOnce)
    // Bidirectional status updates
    .add_topic("status/+", BridgeDirection::Both, QoS::AtMostOnce);
```

## Bridge Direction Types

| Direction | Description | Use Case |
|-----------|-------------|----------|
| `Out` | Local → Remote | Send data to upstream broker |
| `In` | Remote → Local | Receive data from upstream broker |
| `Both` | Bidirectional | Synchronize data between brokers |

## Topic Mapping

### Basic Topic Patterns

```rust
// Forward all sensor data
.add_topic("sensors/#", BridgeDirection::Out, QoS::AtLeastOnce)

// Forward specific sensor types
.add_topic("sensors/+/temperature", BridgeDirection::Out, QoS::AtMostOnce)
.add_topic("sensors/+/humidity", BridgeDirection::Out, QoS::AtMostOnce)

// Receive all commands
.add_topic("commands/#", BridgeDirection::In, QoS::AtLeastOnce)
```

### Topic Prefixes

Add prefixes to avoid conflicts:

```rust
use mqtt_v5::broker::bridge::TopicMapping;

let topic_mapping = TopicMapping {
    pattern: "sensors/#".to_string(),
    direction: BridgeDirection::Out,
    qos: QoS::AtLeastOnce,
    local_prefix: Some("site01/".to_string()),    // site01/sensors/# 
    remote_prefix: Some("remote/".to_string()),   // remote/sensors/#
};
```

## Authentication

### Username/Password

```rust
let bridge_config = BridgeConfig::new("secure-bridge", "remote-broker:1883")
    .with_username("bridge-user")
    .with_password("bridge-password")
    .add_topic("data/#", BridgeDirection::Out, QoS::AtLeastOnce);
```

### TLS/SSL

```rust
let bridge_config = BridgeConfig::new("tls-bridge", "remote-broker:8883")
    .with_tls(true)
    .with_tls_server_name("mqtt.example.com")  // For SNI
    .add_topic("secure/#", BridgeDirection::Both, QoS::AtLeastOnce);
```

## Advanced Configuration

### Complete Bridge Setup

```rust
use mqtt_v5::broker::bridge::{BridgeConfig, BridgeDirection, MqttVersion};
use std::time::Duration;

let bridge_config = BridgeConfig {
    name: "production-bridge".to_string(),
    remote_address: "mqtt.central.com:8883".to_string(),
    client_id: "edge-broker-01".to_string(),
    username: Some("bridge-user".to_string()),
    password: Some("secure-password".to_string()),
    use_tls: true,
    tls_server_name: Some("mqtt.central.com".to_string()),
    try_private: true,  // Mosquitto compatibility
    clean_start: false,  // Resume previous session
    keepalive: 60,
    protocol_version: MqttVersion::V50,
    reconnect_delay: Duration::from_secs(5),
    max_reconnect_attempts: None,  // Infinite retries
    backup_brokers: vec![
        "mqtt-backup1.central.com:8883".to_string(),
        "mqtt-backup2.central.com:8883".to_string(),
    ],
    topics: vec![
        TopicMapping {
            pattern: "telemetry/+/data".to_string(),
            direction: BridgeDirection::Out,
            qos: QoS::AtLeastOnce,
            local_prefix: None,
            remote_prefix: Some("edge01/".to_string()),
        },
        TopicMapping {
            pattern: "commands/+".to_string(),
            direction: BridgeDirection::In,
            qos: QoS::ExactlyOnce,
            local_prefix: Some("remote/".to_string()),
            remote_prefix: None,
        },
    ],
};
```

### Failover Configuration

```rust
let bridge_config = BridgeConfig::new("ha-bridge", "primary-broker:1883")
    .add_backup_broker("backup1-broker:1883")
    .add_backup_broker("backup2-broker:1883")
    .with_reconnect_delay(Duration::from_secs(10))
    .with_max_reconnect_attempts(None);  // Keep trying forever
```

## Common Bridge Patterns

### 1. Edge-to-Cloud Pattern

```rust
// Edge broker configuration
let edge_to_cloud = BridgeConfig::new("edge-to-cloud", "cloud.iot.com:8883")
    .with_tls(true)
    .with_username("edge-device-001")
    .with_password("edge-secret")
    // Send sensor data to cloud
    .add_topic("sensors/+/data", BridgeDirection::Out, QoS::AtLeastOnce)
    // Send alerts immediately
    .add_topic("alerts/#", BridgeDirection::Out, QoS::ExactlyOnce)
    // Receive configuration updates
    .add_topic("config/+", BridgeDirection::In, QoS::AtLeastOnce)
    // Receive firmware updates
    .add_topic("firmware/+", BridgeDirection::In, QoS::ExactlyOnce);
```

### 2. Site-to-Site Pattern

```rust
// Connect two office locations
let site_bridge = BridgeConfig::new("office-a-to-b", "office-b.company.com:1883")
    // Share employee presence data
    .add_topic("presence/+/status", BridgeDirection::Both, QoS::AtMostOnce)
    // Share meeting room availability
    .add_topic("rooms/+/available", BridgeDirection::Both, QoS::AtMostOnce)
    // Central announcements
    .add_topic("announcements/#", BridgeDirection::In, QoS::AtLeastOnce);
```

### 3. Data Aggregation Pattern

```rust
// Multiple sites sending to central analytics
let analytics_bridge = BridgeConfig::new("site-to-analytics", "analytics.company.com:8883")
    .with_tls(true)
    // Send all metrics data
    .add_topic("metrics/#", BridgeDirection::Out, QoS::AtLeastOnce)
    // Send logs for analysis
    .add_topic("logs/+/events", BridgeDirection::Out, QoS::AtMostOnce)
    // Don't receive anything back
    .with_clean_start(true);  // Don't need persistent session
```

### 4. Redundant Bridge Pattern

```rust
// Primary bridge
let primary_bridge = BridgeConfig::new("primary-link", "main-broker:1883")
    .add_topic("critical/#", BridgeDirection::Both, QoS::ExactlyOnce)
    .with_client_id("bridge-primary");

// Backup bridge (different client ID)
let backup_bridge = BridgeConfig::new("backup-link", "main-broker:1883")
    .add_topic("critical/#", BridgeDirection::Both, QoS::ExactlyOnce)
    .with_client_id("bridge-backup");

// Run both bridges for redundancy
```

## Loop Prevention

### Automatic Loop Detection

The broker includes loop prevention to avoid infinite message loops:

```rust
use mqtt_v5::broker::bridge::LoopPrevention;

// Messages are tagged with originating broker ID
// Duplicate messages are automatically dropped
```

### Manual Loop Prevention

Design topic hierarchies to prevent loops:

```rust
// Good: Clear directional flow
let bridge = BridgeConfig::new("directional", "remote:1883")
    .add_topic("local/out/#", BridgeDirection::Out, QoS::AtLeastOnce)
    .add_topic("remote/in/#", BridgeDirection::In, QoS::AtLeastOnce);

// Bad: Can create loops
let bad_bridge = BridgeConfig::new("loop-risk", "remote:1883")
    .add_topic("#", BridgeDirection::Both, QoS::AtLeastOnce);  // Danger!
```

## Monitoring Bridges

### Bridge Status Topics

Monitor bridge health via system topics:

```text
$SYS/broker/bridges/+/state          # connected/disconnected
$SYS/broker/bridges/+/messages/sent   # Messages sent to remote
$SYS/broker/bridges/+/messages/received # Messages from remote
$SYS/broker/bridges/+/bytes/sent      # Bandwidth usage
$SYS/broker/bridges/+/bytes/received  # Bandwidth usage
```

### Bridge Events

```rust
// Subscribe to bridge events
client.subscribe("$SYS/broker/bridges/+/state", |msg| {
    let bridge_name = extract_bridge_name(&msg.topic);
    let state = String::from_utf8_lossy(&msg.payload);
    info!("Bridge {} is now {}", bridge_name, state);
}).await?;
```

## Security Considerations

### 1. Authentication

Always use authentication for bridges:

```rust
// ✅ Good: Authenticated bridge
let secure_bridge = BridgeConfig::new("secure", "remote:8883")
    .with_username("bridge-account")
    .with_password("strong-password")
    .with_tls(true);

// ❌ Bad: Anonymous bridge
let insecure_bridge = BridgeConfig::new("insecure", "remote:1883");
```

### 2. Topic Restrictions

Limit bridge topics to necessary data:

```rust
// ✅ Good: Specific topics only
let limited_bridge = BridgeConfig::new("limited", "remote:1883")
    .add_topic("public/weather", BridgeDirection::In, QoS::AtMostOnce)
    .add_topic("sensors/temperature", BridgeDirection::Out, QoS::AtLeastOnce);

// ❌ Bad: Bridge everything
let unlimited_bridge = BridgeConfig::new("unlimited", "remote:1883")
    .add_topic("#", BridgeDirection::Both, QoS::AtLeastOnce);
```

### 3. Network Security

Use TLS for internet bridges:

```rust
// For bridges over public internet
let internet_bridge = BridgeConfig::new("internet", "cloud.example.com:8883")
    .with_tls(true)
    .with_tls_server_name("cloud.example.com");

// For internal network bridges, TLS optional
let internal_bridge = BridgeConfig::new("internal", "broker.local:1883");
```

## Example: Complete Edge-to-Cloud Setup

```rust
use mqtt_v5::broker::{BrokerConfig, MqttBroker};
use mqtt_v5::broker::bridge::{BridgeConfig, BridgeDirection};
use mqtt_v5::QoS;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure edge broker
    let edge_config = BrokerConfig::default()
        .with_bind_address("0.0.0.0:1883".parse()?)
        .with_max_clients(100);

    // Start edge broker
    let mut edge_broker = MqttBroker::with_config(edge_config).await?;

    // Configure bridge to cloud
    let cloud_bridge = BridgeConfig::new("edge-001-to-cloud", "cloud.iot-platform.com:8883")
        // Authentication
        .with_username("edge-device-001")
        .with_password("device-secret")
        .with_tls(true)
        
        // Connection settings
        .with_client_id("edge-broker-001")
        .with_clean_start(false)  // Resume session
        .with_keepalive(30)
        
        // Failover
        .add_backup_broker("cloud-backup1.iot-platform.com:8883")
        .add_backup_broker("cloud-backup2.iot-platform.com:8883")
        .with_reconnect_delay(Duration::from_secs(5))
        
        // Topic mappings
        // Send telemetry data to cloud
        .add_topic("telemetry/+/data", BridgeDirection::Out, QoS::AtLeastOnce)
        // Send critical alerts immediately  
        .add_topic("alerts/critical", BridgeDirection::Out, QoS::ExactlyOnce)
        // Receive configuration updates
        .add_topic("config/edge-001/+", BridgeDirection::In, QoS::AtLeastOnce)
        // Receive commands
        .add_topic("commands/edge-001/+", BridgeDirection::In, QoS::ExactlyOnce);

    // Add bridge to broker
    // edge_broker.add_bridge(cloud_bridge).await?;

    println!("🌉 Edge broker with cloud bridge running");
    println!("  Local clients connect to: mqtt://localhost:1883");
    println!("  Bridged to cloud at: mqtts://cloud.iot-platform.com:8883");

    // Run broker
    edge_broker.run().await?;
    Ok(())
}
```

## Testing Bridges

### Local Bridge Testing

```rust
#[tokio::test]
async fn test_bridge_message_flow() {
    // Start two brokers
    let broker1 = MqttBroker::bind("127.0.0.1:0").await?;
    let broker2 = MqttBroker::bind("127.0.0.1:0").await?;
    
    // Get dynamic ports
    let addr1 = broker1.local_addr()?;
    let addr2 = broker2.local_addr()?;
    
    // Create bridge from broker1 to broker2
    let bridge = BridgeConfig::new("test-bridge", addr2.to_string())
        .add_topic("test/#", BridgeDirection::Out, QoS::AtLeastOnce);
    
    // broker1.add_bridge(bridge).await?;
    
    // Test message flow
    let client1 = MqttClient::new("pub-client");
    client1.connect(&format!("mqtt://{}", addr1)).await?;
    
    let client2 = MqttClient::new("sub-client");
    client2.connect(&format!("mqtt://{}", addr2)).await?;
    
    // Subscribe on broker2
    let received = Arc::new(Mutex::new(false));
    let received_clone = received.clone();
    client2.subscribe("test/data", move |_| {
        *received_clone.lock().unwrap() = true;
    }).await?;
    
    // Publish on broker1
    client1.publish("test/data", b"bridged message").await?;
    
    // Verify message was bridged
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(*received.lock().unwrap());
}
```

## Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| "Bridge won't connect" | Network/firewall | Check connectivity with telnet/nc |
| "Authentication failed" | Wrong credentials | Verify username/password |
| "Messages not flowing" | Topic mismatch | Check topic patterns and direction |
| "Message loops" | Bidirectional wildcards | Use specific topics or prefixes |
| "High latency" | Network distance | Consider regional brokers |

### Debug Bridge Connections

```bash
# Check bridge status
# Using our mqttv5 CLI (recommended)
mqttv5 sub --host localhost --topic '$SYS/broker/bridges/+/state' --verbose
# Or with mosquitto
mosquitto_sub -h localhost -t '$SYS/broker/bridges/+/state' -v

# Monitor bridge traffic
mqttv5 sub --host localhost --topic '$SYS/broker/bridges/+/messages/+' --verbose  
# mosquitto_sub -h localhost -t '$SYS/broker/bridges/+/messages/+' -v

# Test connectivity to remote broker
nc -zv remote-broker 1883

# Test TLS connection
openssl s_client -connect remote-broker:8883 -servername mqtt.example.com
```

## Best Practices

1. **Use Specific Topics**: Avoid wildcards that could create loops
2. **Implement Authentication**: Always secure bridge connections
3. **Monitor Bridge Health**: Subscribe to $SYS topics for status
4. **Plan Topic Hierarchy**: Design to prevent conflicts and loops
5. **Use Appropriate QoS**: Balance reliability with performance
6. **Configure Failover**: Add backup brokers for high availability
7. **Regular Testing**: Verify bridge functionality periodically
8. **Document Topology**: Maintain diagrams of bridge connections

## Performance Tuning

### Bridge Buffer Sizes

```rust
// For high-throughput bridges
let high_throughput_bridge = BridgeConfig::new("ht-bridge", "remote:1883")
    .with_max_queued_messages(10000)  // Buffer more messages
    .with_batch_size(100);            // Send in batches
```

### QoS Selection

- **QoS 0**: Best performance, no delivery guarantee
- **QoS 1**: Good balance, at least once delivery
- **QoS 2**: Highest reliability, exactly once delivery

Choose based on data criticality and network reliability.