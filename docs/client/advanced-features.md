# Advanced Client Features

Advanced MQTT v5.0 client features and patterns.

## Quality of Service (QoS)

### Understanding QoS Levels

MQTT provides three Quality of Service levels for message delivery:

1. **QoS 0 (At Most Once)** - Fire and forget
   - No acknowledgment
   - No retry
   - Fastest, least reliable

2. **QoS 1 (At Least Once)** - Acknowledged delivery
   - Sender stores until acknowledged
   - May result in duplicates
   - Good balance of reliability and performance

3. **QoS 2 (Exactly Once)** - Assured delivery
   - Four-way handshake
   - No duplicates
   - Highest overhead

### QoS Usage Examples

```rust
use mqtt_v5::{MqttClient, QoS, PublishOptions};

// QoS 0 - Fire and forget
client.publish("sensors/temp", b"22.5").await?;

// QoS 1 - Acknowledged
let packet_id = client.publish_qos1("alerts/warning", b"High temperature").await?;
println!("Message sent with ID: {}", packet_id);

// QoS 2 - Exactly once
let packet_id = client.publish_qos2("commands/critical", b"Emergency stop").await?;

// Custom QoS
let mut options = PublishOptions::default();
options.qos = QoS::ExactlyOnce;
client.publish_with_options("important/data", b"Critical payload", options).await?;
```

### QoS for Subscriptions

```rust
use mqtt_v5::{SubscribeOptions, QoS};

// Subscribe with specific QoS
let mut options = SubscribeOptions::default();
options.qos = QoS::ExactlyOnce;

client.subscribe_with_options("critical/alerts", options, |msg| {
    // Messages delivered with requested QoS or lower
    println!("Received with QoS {:?}: {}", msg.qos, 
        String::from_utf8_lossy(&msg.payload));
}).await?;
```

## Retained Messages

Retained messages are stored by the broker and delivered to future subscribers.

### Publishing Retained Messages

```rust
use mqtt_v5::PublishOptions;

// Publish retained message
let mut options = PublishOptions::default();
options.retain = true;
options.qos = QoS::AtLeastOnce;

// Last known state - delivered to new subscribers
client.publish_with_options(
    "devices/sensor1/status",
    b"online",
    options
).await?;

// Clear retained message (empty payload)
client.publish_with_options(
    "devices/sensor1/status",
    b"",
    options
).await?;
```

### Handling Retained Messages

```rust
use mqtt_v5::{SubscribeOptions, RetainHandling};

let mut options = SubscribeOptions::default();

// Always receive retained messages (default)
options.retain_handling = RetainHandling::SendAtSubscribe;

// Only if subscription is new
options.retain_handling = RetainHandling::SendAtSubscribeIfNew;

// Never send retained messages
options.retain_handling = RetainHandling::DoNotSend;

client.subscribe_with_options("devices/+/status", options, |msg| {
    if msg.retain {
        println!("Retained message: {}", String::from_utf8_lossy(&msg.payload));
    }
}).await?;
```

## Last Will and Testament (LWT)

LWT messages are sent by the broker when a client disconnects unexpectedly.

### Configuring Last Will

```rust
use mqtt_v5::{ConnectOptions, LastWill, WillProperties, QoS};
use std::time::Duration;

let will = LastWill {
    topic: "devices/device1/status".to_string(),
    payload: b"offline".to_vec(),
    qos: QoS::AtLeastOnce,
    retain: true,
    properties: WillProperties {
        will_delay_interval: Some(30), // Delay 30 seconds
        message_expiry_interval: Some(3600), // Expire after 1 hour
        content_type: Some("text/plain".to_string()),
        ..Default::default()
    },
};

let mut options = ConnectOptions::new("device1");
options.will = Some(will);
options.keep_alive = Duration::from_secs(30);

let client = MqttClient::with_options(options);
```

### Graceful Shutdown

```rust
// Prevent will message on graceful disconnect
client.disconnect().await?;

// Or update will message before disconnect
let updated_will = LastWill {
    topic: "devices/device1/status".to_string(),
    payload: b"maintenance".to_vec(),
    qos: QoS::AtLeastOnce,
    retain: true,
    properties: WillProperties::default(),
};

// Update will (MQTT v5.0 feature)
client.update_will(updated_will).await?;
client.disconnect().await?;
```

## Topic Aliases

Topic aliases reduce bandwidth by replacing topic strings with numbers.

### Using Topic Aliases

```rust
use mqtt_v5::PublishOptions;

// First publish establishes alias
let mut options = PublishOptions::default();
options.properties.topic_alias = Some(1);

client.publish_with_options(
    "very/long/topic/name/for/sensor/data",
    b"value1",
    options.clone()
).await?;

// Subsequent publishes can use empty topic with alias
client.publish_with_options(
    "", // Empty topic
    b"value2",
    options
).await?;
```

### Managing Topic Aliases

```rust
// Check broker's topic alias maximum
let max_aliases = client.get_server_topic_alias_maximum().await?;
println!("Broker supports {} topic aliases", max_aliases);

// Client automatically manages alias allocation
```

## Message Properties

MQTT v5.0 introduces message properties for enhanced functionality.

### Common Properties

```rust
use mqtt_v5::{PublishOptions, PayloadFormatIndicator};

let mut options = PublishOptions::default();

// Indicate UTF-8 text payload
options.properties.payload_format_indicator = Some(PayloadFormatIndicator::Utf8);

// Message expiry (seconds)
options.properties.message_expiry_interval = Some(3600); // 1 hour

// Response topic for request/response pattern
options.properties.response_topic = Some("responses/client1".to_string());

// Correlation data for matching responses
options.properties.correlation_data = Some(b"req-123".to_vec());

// Content type
options.properties.content_type = Some("application/json".to_string());

// User properties (key-value pairs)
options.properties.user_properties = vec![
    ("request-id".to_string(), "12345".to_string()),
    ("client-version".to_string(), "1.0.0".to_string()),
];

client.publish_with_options("requests/service", b"{\"cmd\":\"status\"}", options).await?;
```

### Request/Response Pattern

```rust
use uuid::Uuid;

// Set up response subscription
let response_topic = format!("responses/{}", client_id);
client.subscribe(&response_topic, |msg| {
    if let Some(correlation_data) = &msg.properties.correlation_data {
        let request_id = String::from_utf8_lossy(correlation_data);
        println!("Response for request {}: {}", request_id, 
            String::from_utf8_lossy(&msg.payload));
    }
}).await?;

// Send request
let request_id = Uuid::new_v4().to_string();
let mut options = PublishOptions::default();
options.properties.response_topic = Some(response_topic.clone());
options.properties.correlation_data = Some(request_id.as_bytes().to_vec());

client.publish_with_options(
    "requests/temperature",
    b"get_current",
    options
).await?;
```

## Session Management

### Clean Start vs Persistent Sessions

```rust
use mqtt_v5::ConnectOptions;

// Clean start - no session state preserved
let mut options = ConnectOptions::new("client1");
options.clean_start = true;

// Persistent session
options.clean_start = false;
options.session_expiry_interval = Some(Duration::from_secs(86400)); // 24 hours

// Check if session was restored
client.on_connection_event(|event| {
    if let ConnectionEvent::Connected { session_present } = event {
        if session_present {
            println!("Resumed previous session");
        } else {
            println!("Starting new session");
            // Re-subscribe to topics
        }
    }
}).await?;
```

### Managing Subscriptions

```rust
// Track subscriptions for session recovery
struct SessionManager {
    subscriptions: Arc<Mutex<HashMap<String, QoS>>>,
}

impl SessionManager {
    async fn subscribe(&self, client: &MqttClient, topic: &str, qos: QoS) -> Result<()> {
        // Store subscription
        self.subscriptions.lock().await.insert(topic.to_string(), qos);
        
        // Subscribe
        client.subscribe(topic, |msg| {
            // Handle message
        }).await?;
        
        Ok(())
    }
    
    async fn restore_subscriptions(&self, client: &MqttClient) -> Result<()> {
        let subs = self.subscriptions.lock().await;
        for (topic, qos) in subs.iter() {
            let mut options = SubscribeOptions::default();
            options.qos = *qos;
            
            client.subscribe_with_options(topic, options, |msg| {
                // Handle message
            }).await?;
        }
        Ok(())
    }
}
```

## Flow Control

MQTT v5.0 provides flow control mechanisms.

### Receive Maximum

```rust
use mqtt_v5::ConnectOptions;

let mut options = ConnectOptions::new("client1");
// Limit unacknowledged QoS 1/2 messages
options.receive_maximum = Some(100);

// Monitor flow control
client.on_flow_control_event(|available| {
    println!("Can receive {} more messages", available);
}).await?;
```

### Client-Side Flow Control

```rust
use std::sync::atomic::{AtomicU32, Ordering};

struct FlowController {
    in_flight: AtomicU32,
    max_in_flight: u32,
}

impl FlowController {
    async fn publish_with_flow_control(
        &self,
        client: &MqttClient,
        topic: &str,
        payload: &[u8]
    ) -> Result<()> {
        // Wait if too many in flight
        while self.in_flight.load(Ordering::Relaxed) >= self.max_in_flight {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        
        let result = client.publish_qos1(topic, payload).await;
        
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
        
        result?;
        Ok(())
    }
}
```

## Shared Subscriptions

Share message load across multiple clients.

```rust
// Shared subscription format: $share/group/topic
client.subscribe("$share/workers/tasks/+", |msg| {
    println!("Worker received task: {}", String::from_utf8_lossy(&msg.payload));
    // Process task
}).await?;

// Multiple clients in "workers" group will share messages
```

## Advanced Reconnection

### Custom Reconnection Logic

```rust
use mqtt_v5::{ConnectOptions, ConnectionEvent};

let mut options = ConnectOptions::new("client1");
options.reconnect_config.enabled = false; // Disable auto-reconnect

let client = MqttClient::with_options(options);

// Manual reconnection with custom logic
client.on_connection_event(|event| {
    match event {
        ConnectionEvent::Disconnected { reason } => {
            tokio::spawn(async move {
                let mut attempts = 0;
                loop {
                    attempts += 1;
                    println!("Reconnection attempt #{}", attempts);
                    
                    // Custom backoff
                    let delay = Duration::from_secs(attempts.min(60));
                    tokio::time::sleep(delay).await;
                    
                    // Try alternative brokers
                    let brokers = ["primary.example.com", "backup.example.com"];
                    let broker = brokers[attempts % brokers.len()];
                    
                    match client.connect(&format!("mqtt://{}:1883", broker)).await {
                        Ok(_) => {
                            println!("Reconnected to {}", broker);
                            break;
                        }
                        Err(e) => {
                            println!("Failed to connect to {}: {}", broker, e);
                        }
                    }
                }
            });
        }
        _ => {}
    }
}).await?;
```

## Performance Optimization

### Batch Publishing

```rust
use mqtt_v5::{MqttClient, PublishOptions};
use futures::future::join_all;

async fn batch_publish(
    client: &MqttClient,
    messages: Vec<(String, Vec<u8>)>
) -> Result<Vec<u16>> {
    let futures = messages.into_iter().map(|(topic, payload)| {
        client.publish_qos1(&topic, &payload)
    });
    
    let results = join_all(futures).await;
    
    results.into_iter().collect::<Result<Vec<_>>>()
}
```

### Message Pooling

```rust
use bytes::BytesMut;

struct MessagePool {
    pool: Vec<BytesMut>,
}

impl MessagePool {
    fn get(&mut self) -> BytesMut {
        self.pool.pop().unwrap_or_else(|| BytesMut::with_capacity(1024))
    }
    
    fn return_buffer(&mut self, mut buf: BytesMut) {
        buf.clear();
        self.pool.push(buf);
    }
}
```

## Security Patterns

### Certificate Pinning

```rust
use mqtt_v5::ConnectOptions;

let mut options = ConnectOptions::new("secure-client");

// Pin specific certificate
options.tls_config = Some(TlsConfig {
    ca_cert: Some(include_bytes!("../certs/ca.crt").to_vec()),
    client_cert: Some(include_bytes!("../certs/client.crt").to_vec()),
    client_key: Some(include_bytes!("../certs/client.key").to_vec()),
    verify_hostname: true,
    alpn_protocols: vec!["mqtt".to_string()],
});
```

### Token-Based Authentication

```rust
async fn connect_with_token(token: &str) -> Result<MqttClient> {
    let mut options = ConnectOptions::new("token-client");
    options.username = Some("token".to_string());
    options.password = Some(token.to_string());
    
    // Token refresh on reconnect
    options.on_before_connect = Some(Box::new(|| {
        let new_token = refresh_token().await?;
        options.password = Some(new_token);
        Ok(())
    }));
    
    let client = MqttClient::with_options(options);
    client.connect("mqtts://secure.broker.com:8883").await?;
    Ok(client)
}
```

## Debugging and Monitoring

### Packet Inspection

```rust
// Enable packet logging
std::env::set_var("RUST_LOG", "mqtt_v5=trace");

// Custom packet interceptor
client.on_packet_sent(|packet| {
    tracing::debug!("Sending packet: {:?}", packet);
}).await?;

client.on_packet_received(|packet| {
    tracing::debug!("Received packet: {:?}", packet);
}).await?;
```

### Performance Metrics

```rust
use std::time::Instant;

struct ClientMetrics {
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    connection_time: Option<Instant>,
}

impl ClientMetrics {
    fn record_publish(&self, size: usize) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(size as u64, Ordering::Relaxed);
    }
    
    fn get_stats(&self) -> Stats {
        Stats {
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            uptime: self.connection_time.map(|t| t.elapsed()),
        }
    }
}
```

## Best Practices

1. **Choose appropriate QoS**: Use QoS 0 for telemetry, QoS 1 for commands, QoS 2 only when necessary
2. **Use retained messages wisely**: Only for state that should persist
3. **Implement proper error handling**: Network operations can fail
4. **Monitor connection state**: React to disconnections appropriately
5. **Limit message size**: MQTT has a maximum packet size
6. **Use topic hierarchies**: Organize topics logically for efficient filtering
7. **Implement backpressure**: Don't overwhelm the broker or network
8. **Secure connections**: Always use TLS in production
9. **Handle session state**: Plan for client restarts
10. **Test edge cases**: Network failures, broker restarts, etc.