# Monitoring and Metrics

Monitoring the MQTT broker for performance, health, and troubleshooting.

## Overview

The MQTT broker provides extensive monitoring capabilities through:

- **$SYS Topics**: Standard MQTT monitoring topics
- **Resource Monitoring**: Connection, memory, and rate limits
- **Performance Metrics**: Throughput, latency, and error rates
- **Health Checks**: Availability and readiness endpoints
- **Integration Options**: Prometheus, Grafana, custom monitoring

## $SYS Topics

### Standard $SYS Topics

The broker publishes statistics to special $SYS topics:

```text
# Static information (published once)
$SYS/broker/version              # Broker version
$SYS/broker/implementation       # "mqtt-v5"
$SYS/broker/protocol_version     # "5.0"

# Dynamic statistics (updated periodically)
$SYS/broker/uptime               # Seconds since start
$SYS/broker/clients/connected    # Current connected clients
$SYS/broker/clients/total        # Total clients ever connected
$SYS/broker/clients/maximum      # Max concurrent clients

# Message statistics
$SYS/broker/messages/sent        # Total messages sent
$SYS/broker/messages/received    # Total messages received
$SYS/broker/publish/sent         # PUBLISH messages sent
$SYS/broker/publish/received     # PUBLISH messages received

# Bandwidth statistics
$SYS/broker/bytes/sent           # Total bytes sent
$SYS/broker/bytes/received       # Total bytes received

# Session and subscription stats
$SYS/broker/sessions/active      # Active sessions
$SYS/broker/subscriptions/count  # Total subscriptions
$SYS/broker/retained/count       # Retained messages
```

### Subscribing to $SYS Topics

```rust
use mqtt_v5::MqttClient;

let monitor_client = MqttClient::new("monitor");
monitor_client.connect("mqtt://localhost:1883").await?;

// Subscribe to all $SYS topics
monitor_client.subscribe("$SYS/#", |msg| {
    println!("{} = {}", msg.topic, String::from_utf8_lossy(&msg.payload));
}).await?;

// Subscribe to specific metrics
monitor_client.subscribe("$SYS/broker/clients/connected", |msg| {
    let count = String::from_utf8_lossy(&msg.payload).parse::<usize>().unwrap();
    println!("Connected clients: {}", count);
}).await?;
```

### Custom Monitoring Client

```rust
use mqtt_v5::MqttClient;
use std::collections::HashMap;
use tokio::sync::Mutex;
use std::sync::Arc;

// Monitoring client that tracks metrics
struct MonitoringClient {
    metrics: Arc<Mutex<HashMap<String, String>>>,
}

impl MonitoringClient {
    async fn start(&self, broker_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let client = MqttClient::new("sys-monitor");
        client.connect(broker_addr).await?;

        let metrics = self.metrics.clone();
        client.subscribe("$SYS/#", move |msg| {
            let topic = msg.topic.clone();
            let value = String::from_utf8_lossy(&msg.payload).to_string();
            
            tokio::spawn(async move {
                metrics.lock().await.insert(topic, value);
            });
        }).await?;

        Ok(())
    }

    async fn get_metric(&self, topic: &str) -> Option<String> {
        self.metrics.lock().await.get(topic).cloned()
    }
}
```

## Resource Monitoring

### Connection Monitoring

Monitor active connections and limits:

```rust
use mqtt_v5::broker::ResourceMonitor;

// Access resource monitor stats
let stats = resource_monitor.get_stats().await;

println!("Connections: {}/{}", 
    stats.current_connections, 
    stats.max_connections
);
println!("Utilization: {:.1}%", stats.connection_utilization());
println!("Unique IPs: {}", stats.unique_ips);
```

### Rate Limiting Metrics

Track rate limit violations:

```text
$SYS/broker/rate_limits/exceeded/messages    # Message rate violations
$SYS/broker/rate_limits/exceeded/bandwidth   # Bandwidth violations
$SYS/broker/rate_limits/exceeded/connections # Connection rate violations
```

### Memory Usage

Monitor memory consumption:

```text
$SYS/broker/memory/used          # Current memory usage
$SYS/broker/memory/limit         # Memory limit
$SYS/broker/memory/percentage    # Usage percentage
```

## Performance Metrics

### Message Throughput

Track message rates:

```rust
// Calculate messages per second
let messages_sent = get_metric("$SYS/broker/messages/sent");
let uptime = get_metric("$SYS/broker/uptime");
let msg_per_sec = messages_sent / uptime;

// Monitor publish rates
let publish_rate = get_metric("$SYS/broker/publish/sent");
```

### Latency Monitoring

```rust
use std::time::Instant;

// Measure round-trip latency
async fn measure_latency(client: &MqttClient) -> Duration {
    let start = Instant::now();
    
    let received = Arc::new(Mutex::new(false));
    let received_clone = received.clone();
    
    client.subscribe("latency/response", move |_| {
        *received_clone.lock().unwrap() = true;
    }).await?;
    
    client.publish("latency/ping", b"ping").await?;
    
    // Wait for response
    while !*received.lock().unwrap() {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    
    start.elapsed()
}
```

### Error Rates

Monitor connection and protocol errors:

```text
$SYS/broker/errors/connection    # Connection errors
$SYS/broker/errors/protocol      # Protocol violations
$SYS/broker/errors/auth          # Authentication failures
$SYS/broker/errors/acl           # Authorization failures
```

## Health Checks

### Basic Health Check

```rust
async fn health_check(broker_addr: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let client = MqttClient::new("health-check");
    
    // Try to connect
    match client.connect(broker_addr).await {
        Ok(_) => {
            client.disconnect().await?;
            Ok(true)
        }
        Err(_) => Ok(false)
    }
}
```

### Readiness Check

```rust
async fn readiness_check(broker_addr: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let client = MqttClient::new("readiness-check");
    client.connect(broker_addr).await?;
    
    // Check if broker accepts subscriptions
    let (_, _) = client.subscribe("test/readiness", |_| {}).await?;
    
    // Check if broker accepts publishes
    client.publish("test/readiness", b"test").await?;
    
    client.disconnect().await?;
    Ok(true)
}
```

### HTTP Health Endpoint

If you add an HTTP endpoint:

```rust
use hyper::{Body, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};

async fn health_handler(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    // Check broker health
    let healthy = health_check("mqtt://localhost:1883").await.unwrap_or(false);
    
    if healthy {
        Ok(Response::new(Body::from("OK")))
    } else {
        Ok(Response::builder()
            .status(503)
            .body(Body::from("Service Unavailable"))
            .unwrap())
    }
}
```

## Prometheus Integration

### Exporting Metrics

```rust
use prometheus::{Encoder, TextEncoder, Counter, Gauge, register_counter, register_gauge};

lazy_static! {
    static ref CONNECTED_CLIENTS: Gauge = register_gauge!(
        "mqtt_broker_connected_clients",
        "Number of currently connected clients"
    ).unwrap();
    
    static ref MESSAGES_TOTAL: Counter = register_counter!(
        "mqtt_broker_messages_total",
        "Total number of messages processed"
    ).unwrap();
}

// Update metrics from $SYS topics
async fn update_prometheus_metrics(sys_metrics: &HashMap<String, String>) {
    if let Some(clients) = sys_metrics.get("$SYS/broker/clients/connected") {
        if let Ok(count) = clients.parse::<f64>() {
            CONNECTED_CLIENTS.set(count);
        }
    }
}

// Expose metrics endpoint
async fn metrics_handler(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    
    Ok(Response::new(Body::from(buffer)))
}
```

### Prometheus Configuration

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'mqtt-broker'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 10s
```

## Grafana Dashboard

### Example Dashboard JSON

```json
{
  "dashboard": {
    "title": "MQTT Broker Monitoring",
    "panels": [
      {
        "title": "Connected Clients",
        "targets": [
          {
            "expr": "mqtt_broker_connected_clients"
          }
        ],
        "type": "graph"
      },
      {
        "title": "Message Rate",
        "targets": [
          {
            "expr": "rate(mqtt_broker_messages_total[5m])"
          }
        ],
        "type": "graph"
      },
      {
        "title": "Connection Utilization",
        "targets": [
          {
            "expr": "(mqtt_broker_connected_clients / mqtt_broker_max_clients) * 100"
          }
        ],
        "type": "gauge"
      }
    ]
  }
}
```

## Custom Monitoring Solutions

### Metric Collection Service

```rust
use mqtt_v5::MqttClient;
use std::collections::HashMap;
use tokio::time::{interval, Duration};

struct MetricsCollector {
    client: MqttClient,
    metrics: Arc<Mutex<HashMap<String, f64>>>,
}

impl MetricsCollector {
    async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Subscribe to all metrics
        let metrics = self.metrics.clone();
        self.client.subscribe("$SYS/#", move |msg| {
            if let Ok(value) = String::from_utf8_lossy(&msg.payload).parse::<f64>() {
                let topic = msg.topic.clone();
                tokio::spawn(async move {
                    metrics.lock().await.insert(topic, value);
                });
            }
        }).await?;

        // Periodically process metrics (simple processing loop - not event coordination)
        let mut interval = interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            self.process_metrics().await;
        }
    }

    async fn process_metrics(&self) {
        let metrics = self.metrics.lock().await;
        
        // Calculate derived metrics
        if let (Some(sent), Some(received)) = (
            metrics.get("$SYS/broker/messages/sent"),
            metrics.get("$SYS/broker/messages/received")
        ) {
            let ratio = sent / received;
            info!("Message send/receive ratio: {:.2}", ratio);
        }

        // Check thresholds
        if let Some(clients) = metrics.get("$SYS/broker/clients/connected") {
            if *clients > 9000.0 {
                warn!("High client count: {}", clients);
                // Trigger alert
            }
        }
    }
}
```

### Log Analysis

Enable structured logging for analysis:

```rust
// Configure tracing
tracing_subscriber::fmt()
    .json()  // JSON format for log aggregation
    .with_max_level(tracing::Level::INFO)
    .init();

// Logs will include structured data
// {"timestamp":"2024-01-01T12:00:00Z","level":"INFO","fields":{"client_id":"sensor-01","event":"connected"}}
```

## Alerting

### Basic Alert Rules

```rust
struct AlertManager {
    thresholds: AlertThresholds,
}

struct AlertThresholds {
    max_clients_percent: f64,
    max_error_rate: f64,
    min_uptime_seconds: u64,
}

impl AlertManager {
    async fn check_alerts(&self, metrics: &HashMap<String, f64>) {
        // Connection threshold
        if let (Some(current), Some(max)) = (
            metrics.get("$SYS/broker/clients/connected"),
            metrics.get("$SYS/broker/clients/maximum")
        ) {
            let percent = (current / max) * 100.0;
            if percent > self.thresholds.max_clients_percent {
                self.send_alert(AlertType::HighConnectionCount, percent).await;
            }
        }

        // Error rate
        if let Some(errors) = metrics.get("$SYS/broker/errors/total") {
            let error_rate = errors / metrics.get("$SYS/broker/uptime").unwrap_or(&1.0);
            if error_rate > self.thresholds.max_error_rate {
                self.send_alert(AlertType::HighErrorRate, error_rate).await;
            }
        }
    }

    async fn send_alert(&self, alert_type: AlertType, value: f64) {
        // Send to alerting system (email, Slack, PagerDuty, etc.)
        error!("ALERT: {:?} - Value: {}", alert_type, value);
    }
}
```

## Performance Tuning Based on Metrics

### Automatic Scaling

```rust
async fn auto_scale_broker(metrics: &HashMap<String, f64>) {
    let utilization = metrics.get("$SYS/broker/clients/connected").unwrap_or(&0.0)
        / metrics.get("$SYS/broker/clients/maximum").unwrap_or(&1.0);

    if utilization > 0.8 {
        // Scale up - add more broker instances
        info!("High utilization {:.1}% - scaling up", utilization * 100.0);
    } else if utilization < 0.2 {
        // Scale down - remove broker instances
        info!("Low utilization {:.1}% - scaling down", utilization * 100.0);
    }
}
```

### Resource Adjustment

```rust
async fn adjust_resources(resource_monitor: &ResourceMonitor, metrics: &HashMap<String, f64>) {
    // Adjust rate limits based on current usage
    if let Some(msg_rate) = metrics.get("$SYS/broker/messages/rate") {
        if *msg_rate > 10000.0 {
            // Increase rate limits for high traffic
            resource_monitor.adjust_rate_limits(1.5).await;
        }
    }
}
```

## Monitoring Best Practices

### 1. Essential Metrics to Track

**Availability**:
- Broker uptime
- Connection success rate
- Health check status

**Performance**:
- Message throughput (msgs/sec)
- Connection count
- Message latency
- Error rates

**Resource Usage**:
- Memory consumption
- CPU usage (via system metrics)
- Network bandwidth
- Disk I/O (for persistent storage)

### 2. Alert Thresholds

```rust
const ALERT_THRESHOLDS: AlertThresholds = AlertThresholds {
    max_clients_percent: 90.0,      // Alert at 90% capacity
    max_memory_percent: 85.0,       // Alert at 85% memory
    max_error_rate: 0.01,          // Alert at 1% error rate
    min_message_rate: 100.0,       // Alert if < 100 msgs/sec
    max_latency_ms: 100.0,         // Alert if > 100ms latency
};
```

### 3. Monitoring Architecture

```text
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│ MQTT Broker │────▶│ $SYS Topics  │────▶│  Metrics    │
│             │     │              │     │ Collector   │
└─────────────┘     └──────────────┘     └─────────────┘
                                                │
                    ┌──────────────┐            ▼
                    │  Prometheus  │◀───────────┘
                    └──────────────┘
                            │
                    ┌───────▼──────┐     ┌─────────────┐
                    │   Grafana    │────▶│   Alerts    │
                    └──────────────┘     └─────────────┘
```

### 4. Retention Policies

```yaml
# Metric retention recommendations
- Raw metrics: 24 hours
- 5-minute aggregates: 7 days  
- Hourly aggregates: 30 days
- Daily aggregates: 1 year
```

## Troubleshooting with Metrics

### High Memory Usage

```bash
# Check message queue sizes
mosquitto_sub -h localhost -t '$SYS/broker/queues/+/size' -v

# Check retained message count
mosquitto_sub -h localhost -t '$SYS/broker/retained/count' -v

# Check session count
mosquitto_sub -h localhost -t '$SYS/broker/sessions/active' -v
```

### Connection Issues

```bash
# Monitor connection rate
watch -n 1 'mosquitto_sub -h localhost -t "$SYS/broker/clients/connected" -C 1'

# Check connection errors
mosquitto_sub -h localhost -t '$SYS/broker/errors/connection' -v

# Monitor specific client connections
mosquitto_sub -h localhost -t '$SYS/broker/clients/+/connected' -v
```

### Performance Degradation

```bash
# Check message rates
mosquitto_sub -h localhost -t '$SYS/broker/messages/+' -v

# Monitor error rates
mosquitto_sub -h localhost -t '$SYS/broker/errors/+' -v

# Check resource limits
mosquitto_sub -h localhost -t '$SYS/broker/rate_limits/+' -v
```

## Complete Monitoring Example

```rust
use mqtt_v5::broker::{MqttBroker, BrokerConfig};
use mqtt_v5::MqttClient;
use prometheus::{Encoder, TextEncoder};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Start broker with monitoring
    let config = BrokerConfig::default()
        .with_bind_address("0.0.0.0:1883".parse()?);
    
    let mut broker = MqttBroker::with_config(config).await?;
    
    // Start metrics collection
    let metrics = Arc::new(Mutex::new(HashMap::new()));
    let metrics_clone = metrics.clone();
    
    tokio::spawn(async move {
        let client = MqttClient::new("metrics-collector");
        client.connect("mqtt://localhost:1883").await.unwrap();
        
        client.subscribe("$SYS/#", move |msg| {
            let topic = msg.topic.clone();
            if let Ok(value) = String::from_utf8_lossy(&msg.payload).parse::<f64>() {
                let metrics = metrics_clone.clone();
                tokio::spawn(async move {
                    metrics.lock().await.insert(topic, value);
                });
            }
        }).await.unwrap();
    });

    // Start HTTP metrics endpoint
    tokio::spawn(async move {
        let make_svc = make_service_fn(move |_conn| {
            let metrics = metrics.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |_req| {
                    let metrics = metrics.clone();
                    async move {
                        let metrics = metrics.lock().await;
                        let response = format_metrics(&metrics);
                        Ok::<_, hyper::Error>(Response::new(Body::from(response)))
                    }
                }))
            }
        });

        let addr = ([127, 0, 0, 1], 9090).into();
        let server = Server::bind(&addr).serve(make_svc);
        server.await.unwrap();
    });

    println!("🚀 MQTT Broker with monitoring running");
    println!("  📡 MQTT: mqtt://localhost:1883");
    println!("  📊 Metrics: http://localhost:9090/metrics");
    
    broker.run().await?;
    Ok(())
}

fn format_metrics(metrics: &HashMap<String, f64>) -> String {
    let mut output = String::new();
    
    // Convert $SYS topics to Prometheus format
    for (topic, value) in metrics {
        let metric_name = topic
            .replace("$SYS/broker/", "mqtt_broker_")
            .replace("/", "_");
        output.push_str(&format!("{} {}\n", metric_name, value));
    }
    
    output
}
```