# AWS IoT Integration

Using the MQTT client with AWS IoT Core.

## Overview

The MQTT client is fully compatible with AWS IoT Core, supporting:

- ✅ TLS mutual authentication
- ✅ AWS IoT SDK compatibility (returns packet IDs like Python SDK)
- ✅ Device shadows
- ✅ Jobs
- ✅ Fleet provisioning
- ✅ Custom authorizers

## Prerequisites

1. AWS IoT Core endpoint
2. Device certificate and private key
3. AWS IoT root CA certificate
4. Proper IAM policies attached to certificate

## Basic Connection

### Certificate-Based Authentication

```rust
use mqtt_v5::{MqttClient, ConnectOptions};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // AWS IoT endpoint format: <prefix>.iot.<region>.amazonaws.com
    let endpoint = "a1b2c3d4e5f6g7.iot.us-east-1.amazonaws.com";
    
    // Configure for AWS IoT
    let mut options = ConnectOptions::new("my-thing-001");
    options.clean_start = false;  // Persistent sessions recommended
    options.keep_alive = Duration::from_secs(30);
    
    // AWS IoT specific settings
    options.reconnect_config.enabled = true;
    options.reconnect_config.max_attempts = Some(10);
    
    let client = MqttClient::with_options(options);
    
    // Connect using TLS with certificates
    client.connect(&format!("mqtts://{}:8883", endpoint)).await?;
    
    println!("Connected to AWS IoT!");
    Ok(())
}
```

### Loading Certificates

```rust
use std::fs;
use mqtt_v5::ConnectOptions;

// Load certificates from files
let cert = fs::read("certs/device-certificate.pem.crt")?;
let key = fs::read("certs/private.pem.key")?;
let ca = fs::read("certs/AmazonRootCA1.pem")?;

let mut options = ConnectOptions::new("my-device");
// Configure TLS with loaded certificates
// (TLS configuration details depend on your TLS implementation)
```

## Device Shadows

### Get Shadow State

```rust
// Subscribe to shadow responses
client.subscribe("$aws/things/my-device/shadow/get/accepted", |msg| {
    let shadow = String::from_utf8_lossy(&msg.payload);
    println!("Current shadow state: {}", shadow);
}).await?;

client.subscribe("$aws/things/my-device/shadow/get/rejected", |msg| {
    let error = String::from_utf8_lossy(&msg.payload);
    eprintln!("Shadow get failed: {}", error);
}).await?;

// Request current shadow state
client.publish("$aws/things/my-device/shadow/get", b"").await?;
```

### Update Shadow State

```rust
use serde_json::json;

// Subscribe to update responses
client.subscribe("$aws/things/my-device/shadow/update/accepted", |msg| {
    println!("Shadow updated successfully");
}).await?;

client.subscribe("$aws/things/my-device/shadow/update/rejected", |msg| {
    let error = String::from_utf8_lossy(&msg.payload);
    eprintln!("Shadow update failed: {}", error);
}).await?;

// Update reported state
let update = json!({
    "state": {
        "reported": {
            "temperature": 25.5,
            "humidity": 65,
            "connected": true,
            "firmware_version": "1.2.3"
        }
    }
});

client.publish_qos1(
    "$aws/things/my-device/shadow/update",
    update.to_string().as_bytes()
).await?;
```

### Shadow Delta Handling

```rust
// Subscribe to delta updates (desired != reported)
client.subscribe("$aws/things/my-device/shadow/update/delta", |msg| {
    let delta = String::from_utf8_lossy(&msg.payload);
    println!("Shadow delta: {}", delta);
    
    // Parse and handle desired state changes
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&delta) {
        if let Some(desired) = json["state"].as_object() {
            // Apply desired configuration
            apply_configuration(desired);
        }
    }
}).await?;
```

## AWS IoT Jobs

### Monitor Job Executions

```rust
// Subscribe to job notifications
client.subscribe("$aws/things/my-device/jobs/notify", |msg| {
    let notification = String::from_utf8_lossy(&msg.payload);
    println!("Job notification: {}", notification);
}).await?;

// Get pending jobs
client.subscribe("$aws/things/my-device/jobs/get/accepted", |msg| {
    let jobs = String::from_utf8_lossy(&msg.payload);
    println!("Pending jobs: {}", jobs);
}).await?;

// Request pending jobs
client.publish("$aws/things/my-device/jobs/get", b"").await?;
```

### Execute Job

```rust
async fn execute_job(client: &MqttClient, job_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let topic = format!("$aws/things/my-device/jobs/{}/get", job_id);
    
    // Get job details
    client.publish(&topic, b"").await?;
    
    // Update job status
    let update_topic = format!("$aws/things/my-device/jobs/{}/update", job_id);
    
    // Mark as IN_PROGRESS
    let update = json!({
        "status": "IN_PROGRESS",
        "statusDetails": {
            "startedAt": chrono::Utc::now().to_rfc3339()
        }
    });
    
    client.publish_qos1(&update_topic, update.to_string().as_bytes()).await?;
    
    // Execute job logic...
    perform_job_action(job_id).await?;
    
    // Mark as SUCCEEDED
    let update = json!({
        "status": "SUCCEEDED",
        "statusDetails": {
            "completedAt": chrono::Utc::now().to_rfc3339()
        }
    });
    
    client.publish_qos1(&update_topic, update.to_string().as_bytes()).await?;
    
    Ok(())
}
```

## Fleet Provisioning

### Provision Device Using Template

```rust
// Subscribe to provisioning responses
client.subscribe("$aws/provisioning-templates/my-template/provision/json/accepted", |msg| {
    let response = String::from_utf8_lossy(&msg.payload);
    println!("Provisioning successful: {}", response);
    
    // Extract certificates and save them
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response) {
        if let Some(cert) = json["certificatePem"].as_str() {
            fs::write("device-cert.pem", cert)?;
        }
        if let Some(key) = json["privateKey"].as_str() {
            fs::write("device-key.pem", key)?;
        }
    }
}).await?;

// Request provisioning
let request = json!({
    "certificateOwnershipToken": token,
    "parameters": {
        "SerialNumber": "ABC123",
        "DeviceLocation": "Building A"
    }
});

client.publish_qos1(
    "$aws/provisioning-templates/my-template/provision/json",
    request.to_string().as_bytes()
).await?;
```

## Best Practices

### 1. Connection Management

```rust
use mqtt_v5::{ConnectOptions, ConnectionEvent};
use std::time::Duration;

// Optimal AWS IoT settings
let mut options = ConnectOptions::new("my-device");
options.clean_start = false;  // Preserve subscriptions
options.session_expiry_interval = Some(Duration::from_secs(3600));
options.keep_alive = Duration::from_secs(30);  // AWS IoT timeout is 1.5x

// Reconnection strategy
options.reconnect_config.enabled = true;
options.reconnect_config.initial_delay = Duration::from_secs(1);
options.reconnect_config.max_delay = Duration::from_secs(60);
options.reconnect_config.max_attempts = None;  // Keep trying

// Monitor connection health
client.on_connection_event(|event| {
    match event {
        ConnectionEvent::Disconnected { reason } => {
            // Log disconnection for debugging
            log::warn!("Disconnected from AWS IoT: {:?}", reason);
        }
        ConnectionEvent::Connected { session_present } => {
            if !session_present {
                // Re-subscribe if session was lost
                resubscribe_to_topics().await?;
            }
        }
        _ => {}
    }
}).await?;
```

### 2. Message Queuing

```rust
// AWS IoT has a maximum of 100 in-flight messages
// Track in-flight publishes
use std::sync::atomic::{AtomicU32, Ordering};

static IN_FLIGHT: AtomicU32 = AtomicU32::new(0);

async fn publish_with_throttle(
    client: &MqttClient,
    topic: &str,
    payload: &[u8]
) -> Result<(), Box<dyn std::error::Error>> {
    // Wait if too many in-flight
    while IN_FLIGHT.load(Ordering::Relaxed) >= 90 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    
    IN_FLIGHT.fetch_add(1, Ordering::Relaxed);
    
    let result = client.publish_qos1(topic, payload).await;
    
    IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
    
    result?;
    Ok(())
}
```

### 3. Error Handling

```rust
use mqtt_v5::MqttError;

match client.connect(&aws_iot_endpoint).await {
    Ok(_) => println!("Connected to AWS IoT"),
    Err(MqttError::Network(e)) => {
        eprintln!("Network error - check endpoint: {}", e);
    }
    Err(MqttError::Protocol(code)) => {
        eprintln!("Protocol error - check certificates: {:?}", code);
    }
    Err(e) => {
        eprintln!("Connection failed: {}", e);
    }
}
```

### 4. Topic Best Practices

```rust
// Use AWS IoT reserved topics correctly
const SHADOW_PREFIX: &str = "$aws/things";
const JOBS_PREFIX: &str = "$aws/things";

// Organize custom topics hierarchically
fn device_topic(device_id: &str, data_type: &str) -> String {
    format!("devices/{}/data/{}", device_id, data_type)
}

// Use topic aliases for frequently used topics
let mut options = PublishOptions::default();
options.topic_alias = Some(1);  // Save bandwidth
```

## Troubleshooting

### Common Issues

**Connection Rejected**
- Verify endpoint URL is correct
- Check certificate is activated in AWS IoT
- Ensure IAM policy allows iot:Connect

**Subscribe Failed**
- Check IAM policy allows iot:Subscribe for topic
- Verify topic name is correct (case-sensitive)

**Publish Failed**
- Check IAM policy allows iot:Publish
- Ensure payload size < 128KB (AWS IoT limit)
- Verify QoS settings (AWS IoT supports all QoS levels)

### Debug Connection

```rust
// Enable debug logging
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

// Test connectivity
async fn test_aws_iot_connection(endpoint: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = MqttClient::new("connection-test");
    
    println!("Connecting to {}...", endpoint);
    client.connect(&format!("mqtts://{}:8883", endpoint)).await?;
    
    println!("✅ Connected successfully");
    
    // Test publish
    client.publish("test/connection", b"hello").await?;
    println!("✅ Publish successful");
    
    // Test subscribe
    client.subscribe("test/response", |_| {
        println!("✅ Subscribe successful");
    }).await?;
    
    client.disconnect().await?;
    Ok(())
}
```

## Complete AWS IoT Example

```rust
use mqtt_v5::{MqttClient, ConnectOptions, ConnectionEvent};
use serde_json::json;
use std::time::Duration;
use tokio::time::interval;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    // AWS IoT configuration
    let thing_name = "weather-sensor-001";
    let endpoint = "your-endpoint.iot.us-east-1.amazonaws.com";
    
    // Setup client
    let mut options = ConnectOptions::new(thing_name);
    options.clean_start = false;
    options.keep_alive = Duration::from_secs(30);
    options.reconnect_config.enabled = true;
    
    let client = MqttClient::with_options(options);
    
    // Monitor connection
    client.on_connection_event(|event| {
        log::info!("Connection event: {:?}", event);
    }).await?;
    
    // Connect to AWS IoT
    log::info!("Connecting to AWS IoT...");
    client.connect(&format!("mqtts://{}:8883", endpoint)).await?;
    
    // Subscribe to shadow delta
    client.subscribe(
        &format!("$aws/things/{}/shadow/update/delta", thing_name),
        |msg| {
            log::info!("Shadow delta: {}", String::from_utf8_lossy(&msg.payload));
        }
    ).await?;
    
    // Report telemetry periodically
    let mut interval = interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        
        // Simulate sensor reading
        let telemetry = json!({
            "temperature": 20.0 + rand::random::<f32>() * 10.0,
            "humidity": 40.0 + rand::random::<f32>() * 20.0,
            "timestamp": chrono::Utc::now().to_rfc3339()
        });
        
        // Update shadow
        let shadow_update = json!({
            "state": {
                "reported": telemetry
            }
        });
        
        client.publish_qos1(
            &format!("$aws/things/{}/shadow/update", thing_name),
            shadow_update.to_string().as_bytes()
        ).await?;
        
        // Publish to custom topic
        client.publish_qos1(
            &format!("devices/{}/telemetry", thing_name),
            telemetry.to_string().as_bytes()
        ).await?;
        
        log::info!("Published telemetry: {:?}", telemetry);
    }
}
```