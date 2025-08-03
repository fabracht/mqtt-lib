# Transport Configuration

Configuring TCP, TLS/SSL, and WebSocket transports for the MQTT broker.

## Overview

The MQTT broker supports multiple transport protocols simultaneously:

- **TCP**: Standard MQTT over TCP (port 1883)
- **TLS/SSL**: Secure MQTT over TLS (port 8883)
- **WebSocket**: MQTT over WebSocket (port 8080)
- **WebSocket over TLS**: Secure WebSocket (port 8443)

## TCP Transport

### Basic TCP Configuration

TCP is the default transport and requires minimal configuration:

```rust
use mqtt_v5::broker::{BrokerConfig, MqttBroker};

let config = BrokerConfig::new()
    .with_bind_address("0.0.0.0:1883".parse()?);

let mut broker = MqttBroker::with_config(config).await?;
```

### TCP Performance Tuning

```rust
// High-performance TCP settings
let config = BrokerConfig::new()
    .with_bind_address("0.0.0.0:1883".parse()?)
    .with_max_clients(50000)           // Support more connections
    .with_max_packet_size(4 * 1024 * 1024); // 4MB packets
```

### Multiple TCP Interfaces

To listen on specific network interfaces:

```rust
// Listen on specific interface
let config = BrokerConfig::new()
    .with_bind_address("192.168.1.100:1883".parse()?);

// Note: For multiple interfaces, run multiple broker instances
// or use system-level port forwarding
```

## TLS/SSL Transport

### Basic TLS Setup

```rust
use mqtt_v5::broker::{TlsConfig, BrokerConfig};

let tls_config = TlsConfig::new(
    "certs/server.crt".into(),  // Server certificate
    "certs/server.key".into()   // Server private key
);

let config = BrokerConfig::new()
    .with_bind_address("0.0.0.0:1883".parse()?)  // Regular TCP
    .with_tls(tls_config);  // TLS on default port 8883
```

### Custom TLS Port

```rust
let tls_config = TlsConfig::new("server.crt".into(), "server.key".into())
    .with_bind_address("0.0.0.0:8883".parse()?);  // Custom TLS port

let config = BrokerConfig::new()
    .with_tls(tls_config);
```

### Mutual TLS (mTLS)

Require client certificates for enhanced security:

```rust
let tls_config = TlsConfig::new("server.crt".into(), "server.key".into())
    .with_ca_file("ca.crt".into())        // CA for client validation
    .with_require_client_cert(true)       // Require client certificates
    .with_bind_address("0.0.0.0:8883".parse()?);

let config = BrokerConfig::new()
    .with_tls(tls_config);
```

### Certificate Generation

#### Self-Signed Certificates (Development)

```bash
#!/bin/bash
# Generate self-signed certificate for testing
openssl req -x509 -newkey rsa:4096 -keyout server.key -out server.crt \
    -days 365 -nodes -subj "/CN=localhost"
```

#### Production Certificates

```bash
#!/bin/bash
# Generate private key
openssl genrsa -out server.key 4096

# Generate certificate signing request
openssl req -new -key server.key -out server.csr \
    -subj "/C=US/ST=CA/L=San Francisco/O=MyCompany/CN=mqtt.example.com"

# Submit CSR to Certificate Authority and receive server.crt
# Or use Let's Encrypt for free certificates
```

#### Let's Encrypt Integration

```bash
# Use certbot for Let's Encrypt certificates
certbot certonly --standalone -d mqtt.example.com

# Certificates will be in:
# /etc/letsencrypt/live/mqtt.example.com/fullchain.pem
# /etc/letsencrypt/live/mqtt.example.com/privkey.pem
```

### TLS Configuration Options

```rust
// Advanced TLS configuration
let tls_config = TlsConfig::new("server.crt".into(), "server.key".into())
    .with_ca_file("ca.crt".into())           // Client CA certificates
    .with_require_client_cert(true)          // Mutual TLS
    .with_bind_address("0.0.0.0:8883".parse()?);

// The broker uses rustls with secure defaults:
// - TLS 1.2 and 1.3 only
// - Strong cipher suites
// - Certificate validation
```

## WebSocket Transport

### Basic WebSocket Configuration

```rust
use mqtt_v5::broker::{WebSocketConfig, BrokerConfig};

let ws_config = WebSocketConfig::default()
    .with_bind_address("0.0.0.0:8080".parse()?)
    .with_path("/mqtt");  // WebSocket endpoint path

let config = BrokerConfig::new()
    .with_websocket(ws_config);
```

### WebSocket Options

```rust
let ws_config = WebSocketConfig::new()
    .with_bind_address("0.0.0.0:8080".parse()?)
    .with_path("/mqtt")                    // URL path for WebSocket
    .with_subprotocol("mqtt".to_string()); // WebSocket subprotocol

let config = BrokerConfig::new()
    .with_websocket(ws_config);
```

### WebSocket over TLS (WSS)

```rust
let ws_config = WebSocketConfig::new()
    .with_bind_address("0.0.0.0:8443".parse()?)
    .with_path("/secure-mqtt")
    .with_tls(true);  // Enable WebSocket over TLS

// Note: Requires TLS configuration to be set
let tls_config = TlsConfig::new("server.crt".into(), "server.key".into());

let config = BrokerConfig::new()
    .with_tls(tls_config)
    .with_websocket(ws_config);
```

### WebSocket Client Example (JavaScript)

```javascript
// Browser WebSocket client
const client = new Paho.MQTT.Client("ws://localhost:8080/mqtt", "web-client-001");

client.onConnectionLost = (responseObject) => {
    console.log("Connection lost:", responseObject.errorMessage);
};

client.onMessageArrived = (message) => {
    console.log("Message:", message.destinationName, message.payloadString);
};

// Connect to broker
client.connect({
    onSuccess: () => {
        console.log("Connected to MQTT broker via WebSocket");
        client.subscribe("sensors/#");
    },
    onFailure: (error) => {
        console.log("Connection failed:", error);
    }
});
```

## Multi-Transport Configuration

### Complete Multi-Transport Setup

```rust
use mqtt_v5::broker::{BrokerConfig, TlsConfig, WebSocketConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TLS configuration
    let tls_config = TlsConfig::new("server.crt".into(), "server.key".into())
        .with_ca_file("ca.crt".into())
        .with_require_client_cert(false)  // Optional client certs
        .with_bind_address("0.0.0.0:8883".parse()?);

    // WebSocket configuration  
    let ws_config = WebSocketConfig::new()
        .with_bind_address("0.0.0.0:8080".parse()?)
        .with_path("/mqtt");

    // Complete broker configuration
    let config = BrokerConfig::new()
        // TCP on port 1883
        .with_bind_address("0.0.0.0:1883".parse()?)
        // TLS on port 8883
        .with_tls(tls_config)
        // WebSocket on port 8080
        .with_websocket(ws_config);

    let mut broker = MqttBroker::with_config(config).await?;
    
    println!("🚀 Multi-transport MQTT broker running:");
    println!("  📡 TCP:       mqtt://localhost:1883");
    println!("  🔒 TLS:       mqtts://localhost:8883");
    println!("  🌐 WebSocket: ws://localhost:8080/mqtt");
    
    broker.run().await?;
    Ok(())
}
```

### Production Multi-Transport Example

```rust
use mqtt_v5::broker::{BrokerConfig, TlsConfig, WebSocketConfig, AuthConfig, AuthMethod};

// Production configuration with all transports
let auth_config = AuthConfig {
    allow_anonymous: false,
    password_file: Some("users.txt".into()),
    auth_method: AuthMethod::Password,
    auth_data: None,
};

// TLS with Let's Encrypt certificates
let tls_config = TlsConfig::new(
    "/etc/letsencrypt/live/mqtt.example.com/fullchain.pem".into(),
    "/etc/letsencrypt/live/mqtt.example.com/privkey.pem".into()
)
.with_bind_address("0.0.0.0:8883".parse()?);

// WebSocket for web clients
let ws_config = WebSocketConfig::new()
    .with_bind_address("0.0.0.0:8080".parse()?)
    .with_path("/mqtt");

// Secure WebSocket
let wss_config = WebSocketConfig::new()
    .with_bind_address("0.0.0.0:8443".parse()?)
    .with_path("/secure-mqtt")
    .with_tls(true);

let config = BrokerConfig::new()
    .with_bind_address("0.0.0.0:1883".parse()?)
    .with_auth(auth_config)
    .with_tls(tls_config)
    .with_websocket(ws_config);
    // Note: Currently only one WebSocket config supported
```

## Transport-Specific Client Connections

### TCP Client

```rust
use mqtt_v5::MqttClient;

let client = MqttClient::new("tcp-client");
client.connect("mqtt://localhost:1883").await?;
```

### TLS Client

```rust
use mqtt_v5::{MqttClient, ConnectOptions};

// Simple TLS connection (server certificate validation)
let client = MqttClient::new("tls-client");
client.connect("mqtts://localhost:8883").await?;

// Mutual TLS with client certificate
let mut options = ConnectOptions::new("mtls-client");
// Configure client certificate in options.tls_config
let client = MqttClient::with_options(options);
client.connect("mqtts://localhost:8883").await?;
```

### WebSocket Client

```rust
let client = MqttClient::new("ws-client");
client.connect("ws://localhost:8080/mqtt").await?;

// Secure WebSocket
client.connect("wss://localhost:8443/secure-mqtt").await?;
```

## Performance Considerations

### TCP Performance

TCP provides the best performance for MQTT:

| Metric | Typical Performance |
|--------|-------------------|
| Latency | < 1ms local, 10-50ms WAN |
| Throughput | 10,000+ msg/sec |
| Connections | 50,000+ concurrent |
| CPU Usage | Lowest |

**Best for**: IoT devices, high-frequency messaging, internal networks

### TLS Performance

TLS adds encryption overhead:

| Metric | Typical Performance |
|--------|-------------------|
| Latency | +1-5ms handshake |
| Throughput | 8,000+ msg/sec |
| Connections | 40,000+ concurrent |
| CPU Usage | +20-30% vs TCP |

**Best for**: Internet-facing brokers, sensitive data, compliance requirements

### WebSocket Performance

WebSocket adds protocol overhead:

| Metric | Typical Performance |
|--------|-------------------|
| Latency | +2-10ms |
| Throughput | 5,000+ msg/sec |
| Connections | 20,000+ concurrent |
| CPU Usage | +30-40% vs TCP |

**Best for**: Web browsers, firewall traversal, HTTP infrastructure

## Security Best Practices

### TLS Security

1. **Use Valid Certificates**
   ```rust
   // ✅ Good: Valid certificate from trusted CA
   let tls_config = TlsConfig::new(
       "certs/mqtt.example.com.crt".into(),
       "certs/mqtt.example.com.key".into()
   );
   
   // ❌ Bad: Self-signed certificate in production
   ```

2. **Enable Client Certificates**
   ```rust
   // High security with mutual TLS
   let tls_config = TlsConfig::new("server.crt".into(), "server.key".into())
       .with_ca_file("client-ca.crt".into())
       .with_require_client_cert(true);
   ```

3. **Regular Certificate Updates**
   ```bash
   # Automate certificate renewal with Let's Encrypt
   0 0 1 * * certbot renew --quiet --post-hook "systemctl reload mqtt-broker"
   ```

### WebSocket Security

1. **Use HTTPS for WebSocket Pages**
   ```html
   <!-- Serve WebSocket client over HTTPS -->
   <script src="https://example.com/mqtt-client.js"></script>
   ```

2. **Implement CORS Headers**
   ```rust
   // WebSocket endpoints should validate origins
   // (Handled by the WebSocket implementation)
   ```

3. **Rate Limiting**
   ```rust
   // Configure connection limits per IP
   let config = BrokerConfig::new()
       .with_max_clients(1000);
   ```

## Firewall Configuration

### Required Ports

| Transport | Default Port | Protocol | Direction |
|-----------|-------------|----------|-----------|
| MQTT/TCP | 1883 | TCP | Inbound |
| MQTT/TLS | 8883 | TCP | Inbound |
| WebSocket | 8080 | TCP | Inbound |
| WSS | 8443 | TCP | Inbound |

### Firewall Rules

```bash
# UFW (Ubuntu Firewall)
sudo ufw allow 1883/tcp comment "MQTT"
sudo ufw allow 8883/tcp comment "MQTT/TLS"
sudo ufw allow 8080/tcp comment "MQTT WebSocket"
sudo ufw allow 8443/tcp comment "MQTT WSS"

# iptables
sudo iptables -A INPUT -p tcp --dport 1883 -j ACCEPT -m comment --comment "MQTT"
sudo iptables -A INPUT -p tcp --dport 8883 -j ACCEPT -m comment --comment "MQTT/TLS"
sudo iptables -A INPUT -p tcp --dport 8080 -j ACCEPT -m comment --comment "MQTT WS"
sudo iptables -A INPUT -p tcp --dport 8443 -j ACCEPT -m comment --comment "MQTT WSS"
```

## Load Balancing

### TCP/TLS Load Balancing

Use HAProxy or NGINX for TCP load balancing:

```nginx
# NGINX TCP load balancing
stream {
    upstream mqtt_backend {
        server mqtt1.internal:1883 max_fails=3 fail_timeout=30s;
        server mqtt2.internal:1883 max_fails=3 fail_timeout=30s;
        server mqtt3.internal:1883 max_fails=3 fail_timeout=30s;
    }
    
    server {
        listen 1883;
        proxy_pass mqtt_backend;
        proxy_connect_timeout 5s;
    }
}
```

### WebSocket Load Balancing

```nginx
# NGINX WebSocket load balancing
http {
    upstream mqtt_ws {
        server mqtt1.internal:8080;
        server mqtt2.internal:8080;
        server mqtt3.internal:8080;
    }
    
    server {
        listen 80;
        location /mqtt {
            proxy_pass http://mqtt_ws;
            proxy_http_version 1.1;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
        }
    }
}
```

## Monitoring Transports

### Connection Metrics

Monitor transport-specific metrics:

```rust
// Access broker statistics
// $SYS/broker/clients/connected - Total connected clients
// $SYS/broker/clients/connected/tcp - TCP connections
// $SYS/broker/clients/connected/tls - TLS connections  
// $SYS/broker/clients/connected/ws - WebSocket connections
```

### Transport Health Checks

```bash
# TCP health check
nc -zv localhost 1883

# TLS health check
openssl s_client -connect localhost:8883 -brief

# WebSocket health check
curl -i -N \
  -H "Connection: Upgrade" \
  -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" \
  -H "Sec-WebSocket-Key: SGVsbG8sIHdvcmxkIQ==" \
  http://localhost:8080/mqtt
```

## Troubleshooting

### Common TCP Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| "Connection refused" | Broker not running or wrong port | Check broker status and port |
| "Address already in use" | Port already bound | Stop other services or change port |
| "Too many open files" | File descriptor limit | Increase ulimit: `ulimit -n 65536` |

### Common TLS Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| "Certificate verification failed" | Invalid or expired certificate | Check certificate validity |
| "No cipher suites in common" | TLS version mismatch | Update client TLS library |
| "Certificate CN mismatch" | Wrong hostname in certificate | Use correct hostname or update cert |
| "Client certificate required" | mTLS enabled but no client cert | Provide client certificate |

### Common WebSocket Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| "404 Not Found" | Wrong WebSocket path | Check path configuration |
| "Upgrade required" | Not a WebSocket request | Use WebSocket client library |
| "403 Forbidden" | CORS or authentication issue | Check origins and credentials |
| "Connection timeout" | Firewall blocking WebSocket | Allow WebSocket ports |

### Debug Commands

```bash
# Test TCP connection
# Using our mqttv5 CLI (recommended)
mqttv5 sub --host localhost --port 1883 --topic "test/#" --verbose
# Or with mosquitto
mosquitto_sub -h localhost -p 1883 -t "test/#" -v

# Test TLS connection  
# Using our mqttv5 CLI (recommended - TLS auto-configured)
mqttv5 sub --host localhost --port 8883 --topic "test/#" --verbose
# Or with mosquitto
mosquitto_sub -h localhost -p 8883 -t "test/#" -v \
  --cafile ca.crt --cert client.crt --key client.key

# Test WebSocket with curl
curl -i -N \
  -H "Connection: Upgrade" \
  -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" \
  -H "Sec-WebSocket-Key: x3JJHMbDL1EzLkh9GBhXDw==" \
  -H "Sec-WebSocket-Protocol: mqtt" \
  http://localhost:8080/mqtt

# Check certificate details
openssl x509 -in server.crt -text -noout

# Test TLS handshake
openssl s_client -connect localhost:8883 -showcerts
```

## Transport Selection Guide

Choose the right transport for your use case:

| Use Case | Recommended Transport | Reason |
|----------|---------------------|---------|
| IoT sensors (internal) | TCP | Best performance, low overhead |
| IoT sensors (internet) | TLS | Security for public networks |
| Web dashboards | WebSocket | Browser compatibility |
| Mobile apps | TLS or WSS | Security and firewall traversal |
| High-frequency trading | TCP | Lowest latency |
| Corporate environment | WSS | Firewall-friendly, encrypted |
| Mixed clients | All transports | Maximum compatibility |

## Example: Complete Production Setup

```rust
use mqtt_v5::broker::{BrokerConfig, TlsConfig, WebSocketConfig, AuthConfig, AuthMethod};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Authentication required
    let auth_config = AuthConfig {
        allow_anonymous: false,
        password_file: Some("/etc/mqtt/users.txt".into()),
        auth_method: AuthMethod::Password,
        auth_data: None,
    };

    // TLS with production certificates
    let tls_config = TlsConfig::new(
        "/etc/letsencrypt/live/mqtt.example.com/fullchain.pem".into(),
        "/etc/letsencrypt/live/mqtt.example.com/privkey.pem".into()
    )
    .with_bind_address("0.0.0.0:8883".parse()?);

    // WebSocket for web clients
    let ws_config = WebSocketConfig::new()
        .with_bind_address("0.0.0.0:8080".parse()?)
        .with_path("/mqtt");

    // Production broker configuration
    let config = BrokerConfig::new()
        // TCP on standard port
        .with_bind_address("0.0.0.0:1883".parse()?)
        // Secure settings
        .with_auth(auth_config)
        .with_tls(tls_config)
        .with_websocket(ws_config)
        // Performance settings
        .with_max_clients(10000)
        .with_max_packet_size(1024 * 1024)  // 1MB
        .with_session_expiry(Duration::from_secs(3600));

    // Validate configuration
    config.validate()?;

    let mut broker = MqttBroker::with_config(config).await?;
    
    println!("🚀 Production MQTT broker running:");
    println!("  📡 TCP (auth):     mqtt://mqtt.example.com:1883");
    println!("  🔒 TLS:            mqtts://mqtt.example.com:8883");
    println!("  🌐 WebSocket:      ws://mqtt.example.com:8080/mqtt");
    println!("  🔐 Secure WS:      wss://mqtt.example.com:8443/mqtt");
    
    broker.run().await?;
    Ok(())
}
```