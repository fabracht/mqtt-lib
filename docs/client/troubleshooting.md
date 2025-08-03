# Troubleshooting Guide

Common issues and solutions when using the MQTT v5.0 client library.

## Connection Issues

### Cannot Connect to Broker

**Symptoms:**
- Connection times out
- "Connection refused" errors
- "Network unreachable" errors

**Common Causes and Solutions:**

1. **Wrong broker address**
   ```rust
   // ❌ Wrong
   client.connect("localhost:1883").await?;  // Missing protocol
   client.connect("mqtt://localhost").await?; // Missing port
   
   // ✅ Correct
   client.connect("mqtt://localhost:1883").await?;
   client.connect("mqtts://broker.example.com:8883").await?;
   ```

2. **Firewall blocking connection**
   ```bash
   # Test connectivity
   telnet broker.example.com 1883
   
   # Check firewall rules
   sudo iptables -L -n | grep 1883
   ```

3. **Broker not running**
   ```bash
   # Check if broker is listening
   netstat -an | grep 1883
   
   # Check broker logs
   sudo journalctl -u mosquitto -f
   ```

4. **TLS certificate issues**
   ```rust
   // Enable detailed TLS debugging
   std::env::set_var("RUST_LOG", "mqtt_v5=debug,rustls=debug");
   
   // Verify certificates
   openssl s_client -connect broker.example.com:8883 \
     -CAfile ca.crt -cert client.crt -key client.key
   ```

### Connection Rejected by Broker

**Error Codes and Meanings:**

```rust
match error {
    MqttError::ConnectionRejected(code) => {
        match code {
            ConnectReasonCode::BadUserNameOrPassword => {
                // Check credentials
                println!("Invalid username or password");
            }
            ConnectReasonCode::NotAuthorized => {
                // Check ACL permissions
                println!("Client not authorized");
            }
            ConnectReasonCode::ClientIdentifierNotValid => {
                // Client ID rejected
                println!("Invalid client ID format");
            }
            ConnectReasonCode::ServerUnavailable => {
                // Broker is shutting down
                println!("Broker unavailable");
            }
            ConnectReasonCode::Banned => {
                // Client is banned
                println!("Client banned by broker");
            }
            _ => println!("Connection rejected: {:?}", code),
        }
    }
    _ => {}
}
```

### Frequent Disconnections

**Symptoms:**
- Client disconnects every few seconds/minutes
- "Keep alive timeout" messages
- Reconnection loops

**Solutions:**

1. **Adjust keep-alive settings**
   ```rust
   let mut options = ConnectOptions::new("client-id");
   // Increase keep-alive for unreliable networks
   options.keep_alive = Duration::from_secs(60);
   
   // Or decrease for faster detection
   options.keep_alive = Duration::from_secs(15);
   ```

2. **Handle network interruptions**
   ```rust
   // Enable automatic reconnection
   options.reconnect_config.enabled = true;
   options.reconnect_config.max_delay = Duration::from_secs(30);
   
   // Monitor connection events
   client.on_connection_event(|event| {
       tracing::info!("Connection event: {:?}", event);
   }).await?;
   ```

3. **Check for duplicate client IDs**
   ```rust
   // Ensure unique client IDs
   let client_id = format!("device-{}-{}", 
       device_name, 
       uuid::Uuid::new_v4()
   );
   ```

## Message Delivery Issues

### Messages Not Being Received

**Debugging Steps:**

1. **Verify subscription**
   ```rust
   // Add logging to subscription
   let result = client.subscribe("topic/+/data", |msg| {
       tracing::info!("Received on {}: {:?}", msg.topic, msg.payload);
   }).await;
   
   match result {
       Ok(_) => tracing::info!("Subscribed successfully"),
       Err(e) => tracing::error!("Subscription failed: {}", e),
   }
   ```

2. **Check topic matching**
   ```rust
   // Common topic mistakes
   
   // ❌ Wrong - Typos in topic
   client.subscribe("sensers/+/temp", handler).await?;  // "sensers"
   
   // ❌ Wrong - Case sensitive
   client.subscribe("Sensors/+/Temp", handler).await?;  // Wrong case
   
   // ✅ Correct
   client.subscribe("sensors/+/temp", handler).await?;
   ```

3. **Verify ACL permissions**
   ```rust
   // Test with broader permissions first
   client.subscribe("#", |msg| {
       tracing::info!("Any message: {}", msg.topic);
   }).await?;
   ```

### Messages Being Duplicated

**Common Causes:**

1. **QoS 1/2 retry mechanism**
   ```rust
   // Track packet IDs to detect duplicates
   let processed = Arc::new(Mutex::new(HashSet::new()));
   
   client.subscribe("topic", move |msg| {
       if let Some(packet_id) = msg.packet_id {
           let mut set = processed.lock().unwrap();
           if !set.insert(packet_id) {
               tracing::warn!("Duplicate message: {}", packet_id);
               return;
           }
       }
       // Process message
   }).await?;
   ```

2. **Multiple subscriptions**
   ```rust
   // Avoid overlapping subscriptions
   // ❌ Wrong - Will receive duplicates
   client.subscribe("sensors/#", handler1).await?;
   client.subscribe("sensors/temp/#", handler2).await?;
   
   // ✅ Better - Non-overlapping
   client.subscribe("sensors/temp/#", temp_handler).await?;
   client.subscribe("sensors/humidity/#", humidity_handler).await?;
   ```

### Messages Lost During Reconnection

**Solution: Use persistent sessions**

```rust
let mut options = ConnectOptions::new("persistent-client");
options.clean_start = false;
options.session_expiry_interval = Some(Duration::from_secs(3600));

// Monitor session state
client.on_connection_event(|event| {
    if let ConnectionEvent::Connected { session_present } = event {
        if !session_present {
            tracing::warn!("Session lost - resubscribing");
            // Resubscribe to topics
        }
    }
}).await?;
```

## Performance Issues

### High Memory Usage

**Diagnosis:**

```rust
// Monitor memory usage
use jemalloc_ctl::{stats, epoch};

epoch::mib().unwrap().advance().unwrap();
let allocated = stats::allocated::mib().unwrap().read().unwrap();
let resident = stats::resident::mib().unwrap().read().unwrap();
tracing::info!("Memory - allocated: {} MB, resident: {} MB", 
    allocated / 1_048_576, 
    resident / 1_048_576
);
```

**Solutions:**

1. **Limit message queue size**
   ```rust
   let mut options = ConnectOptions::new("client");
   options.receive_maximum = Some(100); // Limit in-flight messages
   ```

2. **Process messages quickly**
   ```rust
   // ❌ Wrong - Blocking handler
   client.subscribe("data", |msg| {
       std::thread::sleep(Duration::from_secs(1)); // Blocks!
       process_message(&msg);
   }).await?;
   
   // ✅ Correct - Async processing
   let (tx, mut rx) = mpsc::channel(100);
   
   client.subscribe("data", move |msg| {
       let _ = tx.try_send(msg);
   }).await?;
   
   // Process in separate task
   tokio::spawn(async move {
       while let Some(msg) = rx.recv().await {
           process_message_async(&msg).await;
       }
   });
   ```

### High CPU Usage

**Common Causes:**

1. **Busy loops in callbacks**
   ```rust
   // ❌ Wrong - CPU intensive
   client.subscribe("topic", |msg| {
       while !condition_met() {
           // Busy wait
       }
   }).await?;
   
   // ✅ Correct - Use async primitives
   client.subscribe("topic", |msg| {
       tokio::spawn(async move {
           wait_for_condition().await;
       });
   }).await?;
   ```

2. **Excessive reconnection attempts**
   ```rust
   // Limit reconnection rate
   options.reconnect_config.initial_delay = Duration::from_secs(5);
   options.reconnect_config.max_attempts = Some(10);
   ```

### Slow Message Processing

**Optimization Strategies:**

1. **Batch processing**
   ```rust
   let batch = Arc::new(Mutex::new(Vec::new()));
   let batch_clone = batch.clone();
   
   // Collect messages
   client.subscribe("data/+", move |msg| {
       let mut b = batch_clone.lock().unwrap();
       b.push(msg);
       
       if b.len() >= 100 {
           let messages = std::mem::take(&mut *b);
           tokio::spawn(process_batch(messages));
       }
   }).await?;
   
   // Periodic flush
   tokio::spawn(async move {
       let mut interval = tokio::time::interval(Duration::from_secs(1));
       loop {
           interval.tick().await;
           let mut b = batch.lock().unwrap();
           if !b.is_empty() {
               let messages = std::mem::take(&mut *b);
               process_batch(messages).await;
           }
       }
   });
   ```

2. **Parallel processing**
   ```rust
   use tokio::sync::Semaphore;
   
   let semaphore = Arc::new(Semaphore::new(10)); // Max 10 concurrent
   
   client.subscribe("jobs/+", move |msg| {
       let permit = semaphore.clone().acquire_owned();
       tokio::spawn(async move {
           let _permit = permit.await;
           process_job(msg).await;
       });
   }).await?;
   ```

## Protocol Issues

### Invalid Topic Errors

**MQTT Topic Rules:**

```rust
fn validate_topic(topic: &str) -> Result<()> {
    // Check length
    if topic.is_empty() || topic.len() > 65535 {
        return Err("Invalid topic length");
    }
    
    // Check for null characters
    if topic.contains('\0') {
        return Err("Topic contains null character");
    }
    
    // Check wildcard usage
    if topic.contains('#') && !topic.ends_with('#') {
        return Err("# must be last character");
    }
    
    if topic.contains('+') {
        // + must be whole level
        let valid = topic.split('/').all(|level| {
            level == "+" || !level.contains('+')
        });
        if !valid {
            return Err("+ must occupy entire level");
        }
    }
    
    Ok(())
}
```

### Packet Too Large

**Error: PacketTooLarge**

```rust
// Check broker's maximum packet size
let max_packet_size = client.get_server_max_packet_size().await?;
tracing::info!("Broker max packet size: {} bytes", max_packet_size);

// Split large messages
async fn publish_large_data(
    client: &MqttClient,
    topic: &str,
    data: &[u8],
    chunk_size: usize
) -> Result<()> {
    for (i, chunk) in data.chunks(chunk_size).enumerate() {
        let chunk_topic = format!("{}/chunk/{}", topic, i);
        client.publish_qos1(&chunk_topic, chunk).await?;
    }
    
    // Send completion marker
    let complete_topic = format!("{}/complete", topic);
    let total_chunks = (data.len() + chunk_size - 1) / chunk_size;
    client.publish_qos1(&complete_topic, 
        total_chunks.to_string().as_bytes()
    ).await?;
    
    Ok(())
}
```

## Debugging Techniques

### Enable Detailed Logging

```rust
// Set up tracing
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

tracing_subscriber::registry()
    .with(tracing_subscriber::EnvFilter::new(
        std::env::var("RUST_LOG").unwrap_or_else(|_| 
            "mqtt_v5=debug,mqtt_lib=debug".into()
        )
    ))
    .with(tracing_subscriber::fmt::layer())
    .init();
```

### Packet-Level Debugging

```rust
// Create debugging wrapper
struct DebugClient {
    inner: MqttClient,
}

impl DebugClient {
    async fn publish_debug(&self, topic: &str, payload: &[u8]) -> Result<()> {
        let start = Instant::now();
        tracing::debug!("Publishing to {}: {} bytes", topic, payload.len());
        
        let result = self.inner.publish(topic, payload).await;
        
        let elapsed = start.elapsed();
        match &result {
            Ok(_) => tracing::debug!("Publish completed in {:?}", elapsed),
            Err(e) => tracing::error!("Publish failed: {} ({:?})", e, elapsed),
        }
        
        result
    }
}
```

### Network Debugging

```bash
# Capture MQTT traffic
sudo tcpdump -i any -w mqtt.pcap 'tcp port 1883'

# Analyze with Wireshark
wireshark mqtt.pcap

# Monitor in real-time
sudo tcpdump -i any -A 'tcp port 1883' | grep -E 'MQTT|CONNECT|PUBLISH'
```

### Connection State Monitoring

```rust
struct ConnectionMonitor {
    connected: Arc<AtomicBool>,
    last_activity: Arc<Mutex<Instant>>,
    error_count: Arc<AtomicU32>,
}

impl ConnectionMonitor {
    fn new(client: &MqttClient) -> Self {
        let monitor = Self {
            connected: Arc::new(AtomicBool::new(false)),
            last_activity: Arc::new(Mutex::new(Instant::now())),
            error_count: Arc::new(AtomicU32::new(0)),
        };
        
        let m = monitor.clone();
        client.on_connection_event(move |event| {
            match event {
                ConnectionEvent::Connected { .. } => {
                    m.connected.store(true, Ordering::Relaxed);
                    m.error_count.store(0, Ordering::Relaxed);
                    *m.last_activity.lock().unwrap() = Instant::now();
                }
                ConnectionEvent::Disconnected { .. } => {
                    m.connected.store(false, Ordering::Relaxed);
                    m.error_count.fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
            }
        }).await.unwrap();
        
        monitor
    }
    
    fn get_status(&self) -> ConnectionStatus {
        ConnectionStatus {
            connected: self.connected.load(Ordering::Relaxed),
            last_activity: *self.last_activity.lock().unwrap(),
            error_count: self.error_count.load(Ordering::Relaxed),
        }
    }
}
```

## Common Error Messages

### "Not Connected"
- Client disconnected from broker
- Check connection state before operations
- Enable auto-reconnect

### "Operation Timed Out"
- Network latency too high
- Broker not responding
- Increase timeout values

### "Topic Name Invalid"
- Contains null characters
- Exceeds length limit
- Invalid wildcard usage

### "Payload Format Invalid"
- UTF-8 validation failed
- Check PayloadFormatIndicator

### "Quota Exceeded"
- Too many connections
- Message rate limit hit
- Storage quota exceeded

## Getting Help

1. **Enable debug logging**
   ```bash
   RUST_LOG=mqtt_v5=trace cargo run
   ```

2. **Collect diagnostics**
   - Client version
   - Broker type and version
   - Network configuration
   - Error messages with stack traces

3. **Create minimal reproduction**
   ```rust
   // Minimal example that shows the issue
   #[tokio::main]
   async fn main() -> Result<(), Box<dyn std::error::Error>> {
       let client = MqttClient::new("debug-client");
       
       // Add only code needed to reproduce issue
       
       Ok(())
   }
   ```

4. **Check GitHub issues**
   - Search existing issues
   - Create new issue with reproduction steps
   - Include logs and configuration