# Testing

Testing MQTT client applications.

## Mock Client

The library provides a mock client for unit testing without a real broker.

### Basic Mock Usage

```rust
#[cfg(test)]
mod tests {
    use mqtt_v5::{MockMqttClient, Message, QoS};
    
    #[tokio::test]
    async fn test_publish_subscribe() {
        // Create mock client
        let client = MockMqttClient::new("test-client");
        
        // Connect always succeeds
        client.connect("mqtt://mock.broker:1883").await.unwrap();
        
        // Set up subscription
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();
        
        client.subscribe("test/topic", move |msg| {
            received_clone.lock().unwrap().push(msg);
        }).await.unwrap();
        
        // Publish message
        client.publish("test/topic", b"Hello").await.unwrap();
        
        // Verify message received
        tokio::time::sleep(Duration::from_millis(10)).await;
        let messages = received.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].payload, b"Hello");
    }
}
```

### Simulating Connection Events

```rust
use mqtt_v5::{MockMqttClient, ConnectionEvent, DisconnectReason};

#[tokio::test]
async fn test_connection_handling() {
    let client = MockMqttClient::new("test-client");
    
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();
    
    client.on_connection_event(move |event| {
        events_clone.lock().unwrap().push(event);
    }).await.unwrap();
    
    // Simulate connection
    client.connect("mqtt://mock:1883").await.unwrap();
    
    // Simulate disconnection
    client.simulate_disconnect(DisconnectReason::NetworkError(
        std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Connection lost")
    )).await;
    
    // Verify events
    let recorded_events = events.lock().unwrap();
    assert_eq!(recorded_events.len(), 2);
    
    match &recorded_events[0] {
        ConnectionEvent::Connected { session_present } => {
            assert!(!session_present);
        }
        _ => panic!("Expected Connected event"),
    }
    
    match &recorded_events[1] {
        ConnectionEvent::Disconnected { reason } => {
            match reason {
                DisconnectReason::NetworkError(_) => {},
                _ => panic!("Expected NetworkError"),
            }
        }
        _ => panic!("Expected Disconnected event"),
    }
}
```

### Simulating QoS Behavior

```rust
#[tokio::test]
async fn test_qos_acknowledgment() {
    let client = MockMqttClient::new("test-client");
    client.connect("mqtt://mock:1883").await.unwrap();
    
    // QoS 0 - No packet ID
    let result = client.publish("topic", b"qos0").await;
    assert!(result.is_ok());
    
    // QoS 1 - Returns packet ID
    let packet_id = client.publish_qos1("topic", b"qos1").await.unwrap();
    assert!(packet_id > 0);
    
    // QoS 2 - Returns packet ID
    let packet_id = client.publish_qos2("topic", b"qos2").await.unwrap();
    assert!(packet_id > 0);
    
    // Simulate QoS 1 acknowledgment delay
    client.set_qos1_delay(Duration::from_millis(100)).await;
    
    let start = Instant::now();
    client.publish_qos1("topic", b"delayed").await.unwrap();
    assert!(start.elapsed() >= Duration::from_millis(100));
}
```

### Error Injection

```rust
use mqtt_v5::{MockMqttClient, MqttError};

#[tokio::test]
async fn test_error_handling() {
    let client = MockMqttClient::new("test-client");
    
    // Inject connection error
    client.inject_error(MqttError::Network(
        std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "Broker unavailable")
    )).await;
    
    let result = client.connect("mqtt://mock:1883").await;
    assert!(matches!(result, Err(MqttError::Network(_))));
    
    // Clear error and retry
    client.clear_injected_errors().await;
    assert!(client.connect("mqtt://mock:1883").await.is_ok());
    
    // Inject publish error
    client.inject_publish_error("forbidden/topic", MqttError::NotAuthorized).await;
    
    let result = client.publish("forbidden/topic", b"data").await;
    assert!(matches!(result, Err(MqttError::NotAuthorized)));
}
```

## Integration Testing

### Test Fixtures

```rust
use mqtt_v5::{MqttClient, ConnectOptions};
use std::sync::Once;

static INIT: Once = Once::new();

fn setup_test_env() {
    INIT.call_once(|| {
        env_logger::init();
    });
}

async fn create_test_client(client_id: &str) -> Result<MqttClient> {
    setup_test_env();
    
    let mut options = ConnectOptions::new(client_id);
    options.clean_start = true; // Always start fresh in tests
    
    let client = MqttClient::with_options(options);
    client.connect("mqtt://localhost:1883").await?;
    Ok(client)
}

#[tokio::test]
async fn test_real_broker_communication() {
    let publisher = create_test_client("test-pub").await.unwrap();
    let subscriber = create_test_client("test-sub").await.unwrap();
    
    let received = Arc::new(AtomicBool::new(false));
    let received_clone = received.clone();
    
    subscriber.subscribe("test/integration", move |_msg| {
        received_clone.store(true, Ordering::Relaxed);
    }).await.unwrap();
    
    // Allow subscription to propagate
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    publisher.publish("test/integration", b"test message").await.unwrap();
    
    // Wait for message
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    assert!(received.load(Ordering::Relaxed));
    
    publisher.disconnect().await.unwrap();
    subscriber.disconnect().await.unwrap();
}
```

### Using Test Containers

```rust
use testcontainers::{clients, images::generic::GenericImage, Container};

async fn with_mqtt_broker<F, Fut>(test: F)
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = ()>,
{
    let docker = clients::Cli::default();
    let mqtt_image = GenericImage::new("eclipse-mosquitto", "2.0")
        .with_exposed_port(1883);
    
    let container = docker.run(mqtt_image);
    let port = container.get_host_port_ipv4(1883);
    let broker_url = format!("mqtt://localhost:{}", port);
    
    // Wait for broker to be ready
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    test(broker_url).await;
}

#[tokio::test]
async fn test_with_container() {
    with_mqtt_broker(|broker_url| async move {
        let client = MqttClient::new("container-test");
        client.connect(&broker_url).await.unwrap();
        
        // Run tests...
        
        client.disconnect().await.unwrap();
    }).await;
}
```

### Property-Based Testing

```rust
use proptest::prelude::*;
use mqtt_v5::{QoS, PublishOptions};

proptest! {
    #[test]
    fn test_topic_validation(topic in "[a-zA-Z0-9/+#]{1,65535}") {
        // Property: All valid MQTT topics should be accepted
        let result = validate_topic(&topic);
        
        // Topics with # not at end are invalid
        if topic.contains('#') && !topic.ends_with('#') {
            prop_assert!(result.is_err());
        }
        
        // Topics with + in middle of level are invalid
        if topic.contains("+/") || topic.contains("/+/") || topic.ends_with("/+") {
            // This is valid
        } else if topic.contains('+') {
            prop_assert!(result.is_err());
        }
    }
    
    #[test]
    fn test_qos_handling(
        qos_val in 0u8..=2,
        payload in prop::collection::vec(any::<u8>(), 0..1000)
    ) {
        // Property: All valid QoS values should be handled
        let qos = match qos_val {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            2 => QoS::ExactlyOnce,
            _ => unreachable!(),
        };
        
        let mut options = PublishOptions::default();
        options.qos = qos;
        
        // Should be able to create options with any valid QoS
        prop_assert_eq!(options.qos, qos);
    }
}
```

## Performance Testing

### Benchmark Suite

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mqtt_v5::MqttClient;

async fn publish_benchmark(client: &MqttClient, payload_size: usize) {
    let payload = vec![0u8; payload_size];
    
    for i in 0..1000 {
        client.publish(&format!("bench/topic/{}", i), &payload)
            .await
            .unwrap();
    }
}

fn bench_publish_throughput(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    
    let client = runtime.block_on(async {
        let client = MqttClient::new("bench-client");
        client.connect("mqtt://localhost:1883").await.unwrap();
        client
    });
    
    let mut group = c.benchmark_group("publish_throughput");
    
    for size in [100, 1000, 10000].iter() {
        group.bench_function(format!("{}bytes", size), |b| {
            b.to_async(&runtime).iter(|| {
                publish_benchmark(&client, black_box(*size))
            });
        });
    }
    
    group.finish();
}

criterion_group!(benches, bench_publish_throughput);
criterion_main!(benches);
```

### Load Testing

```rust
use futures::stream::{FuturesUnordered, StreamExt};

async fn load_test(
    num_clients: usize,
    messages_per_client: usize,
    payload_size: usize,
) -> Result<LoadTestResults> {
    let start = Instant::now();
    let mut tasks = FuturesUnordered::new();
    
    for i in 0..num_clients {
        let client_id = format!("load-test-{}", i);
        tasks.push(tokio::spawn(async move {
            let client = MqttClient::new(&client_id);
            client.connect("mqtt://localhost:1883").await?;
            
            let payload = vec![0u8; payload_size];
            let client_start = Instant::now();
            
            for j in 0..messages_per_client {
                client.publish(
                    &format!("load/{}/{}", client_id, j),
                    &payload
                ).await?;
            }
            
            let duration = client_start.elapsed();
            client.disconnect().await?;
            
            Ok::<_, MqttError>(ClientResult {
                client_id,
                messages_sent: messages_per_client,
                duration,
            })
        }));
    }
    
    let mut results = Vec::new();
    while let Some(result) = tasks.next().await {
        results.push(result??);
    }
    
    let total_duration = start.elapsed();
    let total_messages = results.iter().map(|r| r.messages_sent).sum::<usize>();
    let throughput = total_messages as f64 / total_duration.as_secs_f64();
    
    Ok(LoadTestResults {
        total_messages,
        total_duration,
        throughput,
        client_results: results,
    })
}
```

## Testing Patterns

### Test Helpers

```rust
use mqtt_v5::{MqttClient, Message};
use tokio::sync::mpsc;

struct TestHelper {
    client: MqttClient,
    message_rx: mpsc::Receiver<Message>,
}

impl TestHelper {
    async fn new(client_id: &str) -> Result<Self> {
        let client = MqttClient::new(client_id);
        client.connect("mqtt://localhost:1883").await?;
        
        let (tx, rx) = mpsc::channel(100);
        
        Ok(Self {
            client,
            message_rx: rx,
        })
    }
    
    async fn subscribe(&self, topic: &str, tx: mpsc::Sender<Message>) -> Result<()> {
        self.client.subscribe(topic, move |msg| {
            let _ = tx.try_send(msg);
        }).await
    }
    
    async fn wait_for_message(&mut self, timeout: Duration) -> Option<Message> {
        tokio::time::timeout(timeout, self.message_rx.recv()).await.ok()?
    }
    
    async fn publish_and_wait(
        &mut self,
        topic: &str,
        payload: &[u8],
        timeout: Duration
    ) -> Result<Option<Message>> {
        self.client.publish(topic, payload).await?;
        Ok(self.wait_for_message(timeout).await)
    }
}
```

### Timeout Utilities

```rust
use tokio::time::{timeout, Duration};

async fn with_timeout<F, T>(duration: Duration, future: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    match timeout(duration, future).await {
        Ok(result) => result,
        Err(_) => Err(MqttError::Timeout),
    }
}

#[tokio::test]
async fn test_subscription_with_timeout() {
    let client = create_test_client("timeout-test").await.unwrap();
    
    let result = with_timeout(Duration::from_secs(5), async {
        client.subscribe("test/timeout", |_| {}).await
    }).await;
    
    assert!(result.is_ok());
}
```

### State Verification

```rust
struct TestState {
    connections: AtomicU32,
    messages_received: AtomicU32,
    last_message: RwLock<Option<Message>>,
}

impl TestState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            connections: AtomicU32::new(0),
            messages_received: AtomicU32::new(0),
            last_message: RwLock::new(None),
        })
    }
    
    async fn verify_message_received(&self, expected_payload: &[u8]) -> bool {
        let last = self.last_message.read().await;
        match &*last {
            Some(msg) => msg.payload == expected_payload,
            None => false,
        }
    }
    
    fn get_stats(&self) -> (u32, u32) {
        (
            self.connections.load(Ordering::Relaxed),
            self.messages_received.load(Ordering::Relaxed),
        )
    }
}
```

## Testing Best Practices

### 1. Isolate Tests

```rust
#[tokio::test]
async fn test_isolation() {
    // Use unique topics per test
    let test_id = uuid::Uuid::new_v4();
    let topic = format!("test/{}/data", test_id);
    
    // Use unique client IDs
    let client_id = format!("test-client-{}", test_id);
    
    // Clean up after test
    let client = MqttClient::new(&client_id);
    defer! {
        let _ = client.disconnect().await;
    }
    
    // Run test...
}
```

### 2. Test Error Conditions

```rust
#[tokio::test]
async fn test_reconnection_behavior() {
    let client = create_test_client("reconnect-test").await.unwrap();
    
    // Force disconnection
    drop(client.transport.write().await.take());
    
    // Verify client detects disconnection
    assert!(!client.is_connected());
    
    // Verify reconnection
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(client.is_connected());
}
```

### 3. Deterministic Testing

```rust
use tokio::time::{pause, advance};

#[tokio::test]
async fn test_with_time_control() {
    pause(); // Pause time
    
    let client = MockMqttClient::new("time-test");
    
    // Set up delayed operation
    let start = Instant::now();
    let future = client.publish_with_delay("topic", b"data", Duration::from_secs(5));
    
    // Advance time
    advance(Duration::from_secs(5)).await;
    
    // Operation should complete
    future.await.unwrap();
    
    // Verify timing
    assert_eq!(start.elapsed(), Duration::from_secs(5));
}
```

### 4. CI/CD Integration

```yaml
# .github/workflows/test.yml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    
    services:
      mosquitto:
        image: eclipse-mosquitto:2.0
        ports:
          - 1883:1883
        options: >-
          --health-cmd "mqttv5 sub --topic '$$SYS/#' --count 1 --non-interactive"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
    
    steps:
      - uses: actions/checkout@v2
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      
      - name: Run tests
        run: |
          cargo make test
          cargo test --doc
      
      - name: Run integration tests
        env:
          MQTT_BROKER_URL: mqtt://localhost:1883
        run: cargo test --test '*' -- --test-threads=1
```

## Debugging Tests

### Enable Tracing

```rust
#[tokio::test]
async fn test_with_tracing() {
    // Initialize tracing for test
    let _ = tracing_subscriber::fmt()
        .with_env_filter("mqtt_v5=debug")
        .with_test_writer()
        .try_init();
    
    let client = MqttClient::new("debug-test");
    
    tracing::info!("Starting test");
    client.connect("mqtt://localhost:1883").await.unwrap();
    
    // Test operations will show debug output
}
```

### Capture Logs

```rust
use tracing_subscriber::fmt::format::FmtSpan;

#[tokio::test]
async fn test_with_log_capture() {
    let (writer, guard) = tracing_test::traced_test();
    
    let client = MqttClient::new("log-test");
    client.connect("mqtt://localhost:1883").await.unwrap();
    
    // Get captured logs
    let logs = writer.as_string();
    assert!(logs.contains("Connected to broker"));
}
```