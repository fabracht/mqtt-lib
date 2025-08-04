# Broker Configuration Reference

Complete reference for configuring the MQTT v5.0 broker.

## Overview

The broker supports comprehensive configuration through the `BrokerConfig` struct, which can be set up programmatically or loaded from configuration files.

## Basic Configuration

### Creating a Configuration

```rust
use mqtt5::broker::{BrokerConfig, MqttBroker};

// Default configuration
let config = BrokerConfig::default();

// Custom configuration
let config = BrokerConfig::new()
    .with_bind_address("0.0.0.0:1883".parse()?)
    .with_max_clients(5000)
    .with_maximum_qos(2);

let mut broker = MqttBroker::with_config(config).await?;
```

## Core Settings

### Network Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `bind_address` | `SocketAddr` | `0.0.0.0:1883` | TCP listener address |
| `max_clients` | `usize` | `10000` | Maximum concurrent connections |
| `max_packet_size` | `usize` | `268435456` (256MB) | Maximum MQTT packet size |

```rust
let config = BrokerConfig::new()
    .with_bind_address("192.168.1.100:1883".parse()?)
    .with_max_clients(5000)
    .with_max_packet_size(1024 * 1024); // 1MB
```

### MQTT Protocol Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `session_expiry_interval` | `Duration` | `3600s` | Session expiry for disconnected clients |
| `maximum_qos` | `u8` | `2` | Maximum QoS level supported (0, 1, or 2) |
| `topic_alias_maximum` | `u16` | `65535` | Maximum topic alias value |
| `retain_available` | `bool` | `true` | Whether retained messages are supported |
| `wildcard_subscription_available` | `bool` | `true` | Wildcard subscriptions allowed |
| `subscription_identifier_available` | `bool` | `true` | Subscription identifiers allowed |
| `shared_subscription_available` | `bool` | `true` | Shared subscriptions allowed |
| `server_keep_alive` | `Option<Duration>` | `None` | Server-assigned keep alive |

```rust
let config = BrokerConfig::new()
    .with_session_expiry(Duration::from_secs(7200)) // 2 hours
    .with_maximum_qos(1) // Only QoS 0 and 1
    .with_retain_available(false); // No retained messages
```

## Authentication Configuration

### Basic Authentication

```rust
use mqtt5::broker::{AuthConfig, AuthMethod};

let auth_config = AuthConfig {
    allow_anonymous: false,
    password_file: Some("users.txt".into()),
    auth_method: AuthMethod::Password,
    auth_data: None,
};

let config = BrokerConfig::new().with_auth(auth_config);
```

### Authentication Methods

| Method | Description | Use Case |
|--------|-------------|----------|
| `AuthMethod::None` | No authentication | Development, internal networks |
| `AuthMethod::Password` | Username/password from file | Basic security |
| `AuthMethod::ScramSha256` | SCRAM-SHA-256 | Enhanced security |

### Password File Format

Create a file with username:password pairs (bcrypt hashed):

```text
# users.txt
admin:$2b$12$LQV3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewjjVG1jOYK9.W5.
sensor01:$2b$12$5dpUjVoxLKOLksYhYGolXuNiWM.kq0FYFfNDhAWPLM4ypCgYtOb1S
device02:$2b$12$RnGlmH8qWGZxT5K5YNlHr.7Zg2SbCxoLPNzQpGx9eI1H4nGsqXyj3
```

### Anonymous Access

```rust
let auth_config = AuthConfig {
    allow_anonymous: true,  // Allow connections without credentials
    password_file: None,
    auth_method: AuthMethod::None,
    auth_data: None,
};
```

## TLS Configuration

### Basic TLS Setup

```rust
use mqtt5::broker::TlsConfig;

let tls_config = TlsConfig::new(
    "certs/server.crt".into(),
    "certs/server.key".into()
);

let config = BrokerConfig::new().with_tls(tls_config);
```

### Advanced TLS Configuration

```rust
let tls_config = TlsConfig::new("server.crt".into(), "server.key".into())
    .with_ca_file("ca.crt".into())           // Client certificate validation
    .with_require_client_cert(true)          // Mutual TLS
    .with_bind_address("0.0.0.0:8883".parse()?); // Custom TLS port

let config = BrokerConfig::new().with_tls(tls_config);
```

### TLS Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `cert_file` | `PathBuf` | Required | Server certificate file |
| `key_file` | `PathBuf` | Required | Server private key file |
| `ca_file` | `Option<PathBuf>` | `None` | CA certificate for client validation |
| `require_client_cert` | `bool` | `false` | Require client certificates |
| `bind_address` | `Option<SocketAddr>` | `None` | TLS listener address (if different) |

## WebSocket Configuration

### Basic WebSocket Setup

```rust
use mqtt5::broker::WebSocketConfig;

let ws_config = WebSocketConfig::default()
    .with_bind_address("0.0.0.0:8080".parse()?)
    .with_path("/mqtt");

let config = BrokerConfig::new().with_websocket(ws_config);
```

### WebSocket Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `bind_address` | `SocketAddr` | `0.0.0.0:8080` | WebSocket listener address |
| `path` | `String` | `/mqtt` | WebSocket endpoint path |
| `subprotocol` | `String` | `mqtt` | WebSocket subprotocol |
| `use_tls` | `bool` | `false` | Enable WebSocket over TLS |

### WebSocket over TLS

```rust
let ws_config = WebSocketConfig::new()
    .with_bind_address("0.0.0.0:8443".parse()?)
    .with_path("/secure-mqtt")
    .with_tls(true);
```

## Storage Configuration

### File-Based Storage (Default)

```rust
use mqtt5::broker::{StorageConfig, StorageBackend};

let storage_config = StorageConfig::new()
    .with_backend(StorageBackend::File)
    .with_base_dir("/var/lib/mqtt".into())
    .with_cleanup_interval(Duration::from_secs(3600))
    .with_persistence(true);

let config = BrokerConfig::new().with_storage(storage_config);
```

### In-Memory Storage (Testing)

```rust
let storage_config = StorageConfig::new()
    .with_backend(StorageBackend::Memory)
    .with_persistence(false);
```

### Storage Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `backend` | `StorageBackend` | `File` | Storage backend type |
| `base_dir` | `PathBuf` | `./mqtt_storage` | Base directory for file storage |
| `cleanup_interval` | `Duration` | `3600s` | Cleanup interval for expired entries |
| `enable_persistence` | `bool` | `true` | Enable persistent storage |

## Resource Monitoring & Limits

### Resource Limits Configuration

```rust
use mqtt5::broker::ResourceLimits;

let limits = ResourceLimits {
    max_connections: 5000,
    max_connections_per_ip: 50,
    max_memory_bytes: 512 * 1024 * 1024, // 512MB
    max_message_rate_per_client: 500,    // 500 msg/sec
    max_bandwidth_per_client: 5 * 1024 * 1024, // 5MB/sec
    max_connection_rate: 50,             // 50 conn/sec
    rate_limit_window: Duration::from_secs(60),
};

// Apply limits through broker configuration
// (Note: ResourceLimits integration with BrokerConfig may vary)
```

### Resource Monitoring Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `max_connections` | `usize` | `10000` | Maximum concurrent connections |
| `max_connections_per_ip` | `usize` | `100` | Maximum connections per IP |
| `max_memory_bytes` | `u64` | `1GB` | Maximum memory usage (0 = unlimited) |
| `max_message_rate_per_client` | `u32` | `1000` | Messages per second per client |
| `max_bandwidth_per_client` | `u64` | `10MB` | Bytes per second per client |
| `max_connection_rate` | `u32` | `100` | New connections per second |
| `rate_limit_window` | `Duration` | `60s` | Time window for rate limiting |

## Complete Multi-Transport Example

```rust
use mqtt5::broker::{BrokerConfig, TlsConfig, WebSocketConfig, AuthConfig, AuthMethod};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Authentication
    let auth_config = AuthConfig {
        allow_anonymous: false,
        password_file: Some("users.txt".into()),
        auth_method: AuthMethod::Password,
        auth_data: None,
    };

    // TLS configuration
    let tls_config = TlsConfig::new("server.crt".into(), "server.key".into())
        .with_ca_file("ca.crt".into())
        .with_require_client_cert(true)
        .with_bind_address("0.0.0.0:8883".parse()?);

    // WebSocket configuration
    let ws_config = WebSocketConfig::new()
        .with_bind_address("0.0.0.0:8080".parse()?)
        .with_path("/mqtt");

    // Complete broker configuration
    let config = BrokerConfig::new()
        // Core settings
        .with_bind_address("0.0.0.0:1883".parse()?)
        .with_max_clients(5000)
        .with_session_expiry(Duration::from_secs(7200))
        .with_maximum_qos(2)
        
        // Transport configuration
        .with_tls(tls_config)
        .with_websocket(ws_config)
        
        // Security
        .with_auth(auth_config);

    // Validate configuration
    config.validate()?;

    // Start broker
    let mut broker = MqttBroker::with_config(config).await?;
    
    println!("🚀 Multi-transport MQTT broker running:");
    println!("  📡 TCP:       mqtt://localhost:1883");
    println!("  🔒 TLS:       mqtts://localhost:8883");
    println!("  🌐 WebSocket: ws://localhost:8080/mqtt");
    
    broker.run().await?;
    Ok(())
}
```

## Configuration Validation

The broker automatically validates configuration on startup:

```rust
// This will return an error if configuration is invalid
let config = BrokerConfig::new()
    .with_max_clients(0) // Invalid: must be > 0
    .with_max_packet_size(512) // Invalid: must be >= 1024
    .with_maximum_qos(3); // Invalid: must be 0, 1, or 2

// Explicit validation
match config.validate() {
    Ok(_) => println!("Configuration is valid"),
    Err(e) => eprintln!("Configuration error: {}", e),
}
```

## Environment Variables

While not directly supported in the configuration struct, you can use environment variables in your application:

```rust
use std::env;

let bind_addr = env::var("MQTT_BIND_ADDR")
    .unwrap_or_else(|_| "0.0.0.0:1883".to_string());

let max_clients: usize = env::var("MQTT_MAX_CLIENTS")
    .unwrap_or_else(|_| "10000".to_string())
    .parse()
    .unwrap_or(10000);

let config = BrokerConfig::new()
    .with_bind_address(bind_addr.parse()?)
    .with_max_clients(max_clients);
```

## Configuration Files

### TOML Configuration Example

```toml
# broker.toml
bind_address = "0.0.0.0:1883"
max_clients = 5000
session_expiry_interval = 7200
max_packet_size = 1048576
maximum_qos = 2
retain_available = true

[auth_config]
allow_anonymous = false
password_file = "users.txt"
auth_method = "Password"

[tls_config]
cert_file = "server.crt"
key_file = "server.key"
ca_file = "ca.crt"
require_client_cert = true
bind_address = "0.0.0.0:8883"

[websocket_config]
bind_address = "0.0.0.0:8080"
path = "/mqtt"
subprotocol = "mqtt"
use_tls = false

[storage_config]
backend = "File"
base_dir = "/var/lib/mqtt"
cleanup_interval = 3600
enable_persistence = true
```

### Loading from TOML

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct ConfigFile {
    #[serde(flatten)]
    broker: BrokerConfig,
}

let config_str = std::fs::read_to_string("broker.toml")?;
let config_file: ConfigFile = toml::from_str(&config_str)?;
let config = config_file.broker;
```

## Performance Tuning

### High-Performance Settings

```rust
let config = BrokerConfig::new()
    .with_max_clients(50000)                    // Scale for more clients
    .with_max_packet_size(4 * 1024 * 1024)     // 4MB packets
    .with_session_expiry(Duration::from_secs(300)); // Shorter sessions
```

### Low-Resource Settings

```rust  
let config = BrokerConfig::new()
    .with_max_clients(100)                      // Limit concurrent clients
    .with_max_packet_size(64 * 1024)           // 64KB packets
    .with_session_expiry(Duration::from_secs(60)); // Short sessions
```

## Security Recommendations

1. **Disable Anonymous Access**: Set `allow_anonymous = false`
2. **Use Strong Passwords**: Use bcrypt with high work factor  
3. **Enable TLS**: Always use TLS in production
4. **Client Certificates**: Enable mutual TLS for high security
5. **Rate Limiting**: Configure appropriate resource limits
6. **Regular Updates**: Keep certificates updated

## Troubleshooting

### Common Configuration Errors

| Error | Cause | Solution |
|-------|-------|----------|
| "max_clients must be greater than 0" | `max_clients = 0` | Set to positive value |
| "max_packet_size must be at least 1024 bytes" | Packet size too small | Increase to >= 1024 |
| "maximum_qos must be 0, 1, or 2" | Invalid QoS value | Use valid QoS level |
| "Certificate file not found" | Invalid TLS paths | Check certificate file paths |
| "Permission denied" | Invalid bind address | Check port availability and permissions |

### Debug Configuration

```rust
// Enable debug logging
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();

// Validate and debug configuration
let config = BrokerConfig::new();
println!("Configuration: {:#?}", config);
config.validate()?;
```