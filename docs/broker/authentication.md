# Authentication and Access Control

Securing the MQTT broker with authentication and fine-grained access control.

## Overview

The MQTT broker provides a comprehensive security framework with:

- **Authentication**: Verify client identity using various methods
- **Access Control Lists (ACLs)**: Fine-grained topic-based permissions
- **TLS/SSL Support**: Encrypted connections and certificate-based auth
- **Rate Limiting**: Resource protection against abuse

## Authentication Methods

### Anonymous Access (Development Only)

```rust
use mqtt_v5::broker::{BrokerConfig, AuthConfig, AuthMethod};

let auth_config = AuthConfig {
    allow_anonymous: true,
    password_file: None,
    auth_method: AuthMethod::None,
    auth_data: None,
};

let config = BrokerConfig::new().with_auth(auth_config);
```

**⚠️ Warning**: Only use anonymous access for development and testing. Never in production.

### Username/Password Authentication

#### Basic Setup

```rust
let auth_config = AuthConfig {
    allow_anonymous: false,
    password_file: Some("users.txt".into()),
    auth_method: AuthMethod::Password,
    auth_data: None,
};

let config = BrokerConfig::new().with_auth(auth_config);
```

#### Password File Format

Create a password file with bcrypt-hashed passwords:

```text
# users.txt - Format: username:bcrypt_hash
admin:$2b$12$LQV3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewjjVG1jOYK9.W5.
sensor01:$2b$12$5dpUjVoxLKOLksYhYGolXuNiWM.kq0FYFfNDhAWPLM4ypCgYtOb1S
device02:$2b$12$RnGlmH8qWGZxT5K5YNlHr.7Zg2SbCxoLPNzQpGx9eI1H4nGsqXyj3

# Lines starting with # are comments
# Empty lines are ignored
```

#### Generating Password Hashes

Use bcrypt to generate secure password hashes:

```bash
# Using bcrypt command line tool
echo -n "your_password" | bcrypt-tool

# Using Python
python3 -c "import bcrypt; print(bcrypt.hashpw(b'your_password', bcrypt.gensalt()).decode())"

# Using online bcrypt generator (for testing only)
```

### SCRAM-SHA-256 Authentication

```rust
let auth_config = AuthConfig {
    allow_anonymous: false,
    password_file: Some("scram_users.txt".into()),
    auth_method: AuthMethod::ScramSha256,
    auth_data: None,
};
```


## TLS Client Certificate Authentication

### Mutual TLS Setup

```rust
use mqtt_v5::broker::TlsConfig;

let tls_config = TlsConfig::new("server.crt".into(), "server.key".into())
    .with_ca_file("ca.crt".into())              // CA to verify client certs
    .with_require_client_cert(true)             // Require client certificates
    .with_bind_address("0.0.0.0:8883".parse()?);

let config = BrokerConfig::new().with_tls(tls_config);
```

### Certificate Generation

Generate certificates for testing:

```bash
#!/bin/bash
# Generate CA private key
openssl genrsa -out ca.key 4096

# Generate CA certificate
openssl req -new -x509 -days 365 -key ca.key -out ca.crt \
    -subj "/C=US/ST=CA/L=San Francisco/O=MyOrg/CN=MyCA"

# Generate server private key
openssl genrsa -out server.key 4096

# Generate server certificate signing request
openssl req -new -key server.key -out server.csr \
    -subj "/C=US/ST=CA/L=San Francisco/O=MyOrg/CN=mqtt-broker"

# Sign server certificate with CA
openssl x509 -req -days 365 -in server.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out server.crt

# Generate client private key
openssl genrsa -out client.key 4096

# Generate client certificate signing request
openssl req -new -key client.key -out client.csr \
    -subj "/C=US/ST=CA/L=San Francisco/O=MyOrg/CN=mqtt-client"

# Sign client certificate with CA
openssl x509 -req -days 365 -in client.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out client.crt
```

## Access Control Lists (ACLs)

### ACL Rules

ACL rules control who can publish/subscribe to which topics:

```rust
use mqtt_v5::broker::acl::{AclRule, Permission};

// Allow admin full access to all topics
let admin_rule = AclRule::new(
    "admin".to_string(),
    "#".to_string(),           // All topics
    Permission::ReadWrite
);

// Allow sensors to publish to sensor topics only
let sensor_rule = AclRule::new(
    "sensor01".to_string(),
    "sensors/+/data".to_string(),  // Only sensor data topics
    Permission::Write
);

// Allow dashboard to read all sensor data
let dashboard_rule = AclRule::new(
    "dashboard".to_string(),
    "sensors/#".to_string(),       // All sensor topics
    Permission::Read
);

// Deny access to system topics for regular users
let deny_system_rule = AclRule::new(
    "*".to_string(),              // All users
    "$SYS/#".to_string(),         // System topics
    Permission::Deny
);
```

### ACL File Format

Create an ACL file for easy management:

```text
# acl.txt - Format: username topic_pattern permission

# Admin has full access
admin # readwrite

# Sensors can only publish their data  
sensor01 sensors/temp/+ write
sensor02 sensors/humidity/+ write
sensor03 sensors/pressure/+ write

# Dashboard can read all sensor data
dashboard sensors/# read

# Applications can read/write app topics
app_server applications/# readwrite

# Anonymous users denied access to system topics
* $SYS/# deny

# Allow anonymous read access to public topics
* public/# read

# Specific device permissions
device_001 devices/001/+ readwrite
device_002 devices/002/+ readwrite
```

### Loading ACLs from File

```rust
use mqtt_v5::broker::acl::AclManager;

// Load ACL configuration
let acl_manager = AclManager::from_file("acl.txt").await?;

// The broker will automatically use ACL rules for authorization
```

## Permission Types

| Permission | Description | Subscribe | Publish |
|------------|-------------|-----------|---------|
| `Read` | Subscribe only | ✅ | ❌ |
| `Write` | Publish only | ❌ | ✅ |
| `ReadWrite` | Full access | ✅ | ✅ |
| `Deny` | Explicitly deny | ❌ | ❌ |

## Topic Pattern Matching

ACL rules support MQTT wildcards:

### Single-level Wildcard (+)

```text
# Matches any single topic level
sensors/+/temperature    # Matches: sensors/room1/temperature, sensors/room2/temperature
devices/+/status         # Matches: devices/device1/status, devices/device2/status
```

### Multi-level Wildcard (#)

```text
# Matches any number of topic levels
sensors/#               # Matches: sensors/temp, sensors/room1/temp, sensors/room1/humidity/current
devices/room1/#         # Matches: devices/room1/light, devices/room1/sensor/temp
```

### Exact Topic Match

```text
# Exact topic name (no wildcards)
system/status           # Matches only: system/status
alerts/critical         # Matches only: alerts/critical
```

## Security Best Practices

### 1. Use Strong Authentication

```rust
// ✅ Good: Disable anonymous access
let auth_config = AuthConfig {
    allow_anonymous: false,
    password_file: Some("users.txt".into()),
    auth_method: AuthMethod::Password,
    auth_data: None,
};

// ❌ Bad: Anonymous access in production
let bad_config = AuthConfig {
    allow_anonymous: true,  // Never do this in production!
    ..Default::default()
};
```

### 2. Implement Principle of Least Privilege

```text
# Give users only the permissions they need

# Sensor devices - write only to their topics
sensor01 sensors/temperature/room1 write
sensor02 sensors/humidity/room2 write

# Monitoring dashboard - read only
dashboard sensors/# read

# Admin - full access (use sparingly)  
admin # readwrite
```

### 3. Protect System Topics

```text
# Deny access to system topics for regular users
* $SYS/# deny

# Only admin can access system information
admin $SYS/# read
```

### 4. Use TLS in Production

```rust
let tls_config = TlsConfig::new("server.crt".into(), "server.key".into())
    .with_ca_file("ca.crt".into())
    .with_require_client_cert(true);  // Mutual TLS for highest security

let config = BrokerConfig::new()
    .with_tls(tls_config)
    .with_auth(secure_auth_config);
```

### 5. Regular Security Audits

```bash
# Review password file permissions
chmod 600 users.txt
chown mqtt:mqtt users.txt

# Review ACL rules regularly
# Check for overly permissive rules
grep "# readwrite" acl.txt

# Monitor authentication failures
# Check broker logs for failed authentication attempts
```

## Complete Security Example

```rust
use mqtt_v5::broker::{BrokerConfig, AuthConfig, AuthMethod, TlsConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Authentication configuration
    let auth_config = AuthConfig {
        allow_anonymous: false,
        password_file: Some("users.txt".into()),
        auth_method: AuthMethod::Password,
        auth_data: None,
    };

    // TLS configuration with mutual authentication
    let tls_config = TlsConfig::new("server.crt".into(), "server.key".into())
        .with_ca_file("ca.crt".into())
        .with_require_client_cert(true)
        .with_bind_address("0.0.0.0:8883".parse()?);

    // Secure broker configuration
    let config = BrokerConfig::new()
        .with_bind_address("0.0.0.0:1883".parse()?)
        .with_max_clients(1000)
        .with_auth(auth_config)
        .with_tls(tls_config);

    // Validate configuration
    config.validate()?;

    let mut broker = MqttBroker::with_config(config).await?;

    println!("🔒 Secure MQTT broker running:");
    println!("  📡 TCP (auth required): mqtt://localhost:1883");  
    println!("  🔐 TLS (mutual auth):   mqtts://localhost:8883");

    broker.run().await?;
    Ok(())
}
```

## Client Authentication Examples

### Username/Password Client

```rust
use mqtt_v5::{MqttClient, ConnectOptions};

let mut options = ConnectOptions::new("sensor01");
options.username = Some("sensor01".to_string());
options.password = Some("sensor_password".to_string());

let client = MqttClient::with_options(options);
client.connect("mqtt://localhost:1883").await?;
```

### TLS Client Certificate

```rust
let mut options = ConnectOptions::new("secure_client");
// Load client certificate and key
options.tls_config = Some(TlsConfig::from_files(
    "client.crt",
    "client.key", 
    Some("ca.crt")
)?);

let client = MqttClient::with_options(options);
client.connect("mqtts://localhost:8883").await?;
```

## Monitoring and Logging

### Authentication Events

The broker logs authentication events:

```rust
// Enable authentication logging
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::INFO)
    .init();

// Authentication success logs:
// INFO mqtt_broker::auth: Client authenticated successfully client_id="sensor01" username="sensor01"

// Authentication failure logs:
// WARN mqtt_broker::auth: Authentication failed client_id="unknown" reason="invalid credentials"
```

### ACL Authorization Events

```rust
// Authorization success:
// DEBUG mqtt_broker::acl: Authorization granted client_id="sensor01" topic="sensors/temp/room1" action="publish"

// Authorization denied:
// WARN mqtt_broker::acl: Authorization denied client_id="sensor01" topic="system/config" action="publish"
```

## Testing Authentication

### Test Authentication Setup

```rust
use mqtt_v5::broker::auth::{PasswordAuthProvider, AuthResult};

#[tokio::test]
async fn test_password_auth() {
    let provider = PasswordAuthProvider::from_file("test_users.txt").await?;
    
    // Test valid credentials
    let connect_packet = create_connect_packet("user1", "password1");
    let result = provider.authenticate(&connect_packet, "127.0.0.1:1234").await?;
    assert!(result.authenticated);
    
    // Test invalid credentials
    let connect_packet = create_connect_packet("user1", "wrong_password");
    let result = provider.authenticate(&connect_packet, "127.0.0.1:1234").await?;
    assert!(!result.authenticated);
}
```

### Test ACL Rules

```rust
use mqtt_v5::broker::acl::{AclRule, Permission};

#[tokio::test]
async fn test_acl_rules() {
    let rule = AclRule::new(
        "sensor01".to_string(),
        "sensors/+/data".to_string(),
        Permission::Write
    );
    
    // Should match sensor topic
    assert!(rule.matches(Some("sensor01"), "sensors/temp/data"));
    
    // Should not match different user
    assert!(!rule.matches(Some("sensor02"), "sensors/temp/data"));
    
    // Should not match different topic pattern
    assert!(!rule.matches(Some("sensor01"), "alerts/critical"));
}
```

## Troubleshooting

### Common Authentication Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| "Authentication failed" | Wrong username/password | Check credentials and password file |
| "Connection refused" | Anonymous not allowed | Enable anonymous or provide credentials |
| "Certificate verification failed" | Invalid client certificate | Check certificate validity and CA chain |
| "Password file not found" | Missing password file | Create password file at specified path |

### Common ACL Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| "Publish denied" | No write permission | Add write/readwrite rule for user/topic |
| "Subscribe denied" | No read permission | Add read/readwrite rule for user/topic |
| "ACL file format error" | Invalid ACL syntax | Check file format: username topic permission |
| "Permission denied" | Explicit deny rule | Remove or modify deny rule |

### Debug Commands

```bash
# Test password hash
echo -n "password" | bcrypt-tool

# Verify certificate chain
openssl verify -CAfile ca.crt client.crt

# Check file permissions
ls -la users.txt acl.txt

# Test MQTT connection
mosquitto_pub -h localhost -p 1883 -u sensor01 -P password -t "test/topic" -m "test"
```

## Security Checklist

- [ ] Disable anonymous access in production
- [ ] Use strong passwords with bcrypt hashing
- [ ] Implement proper ACL rules with least privilege
- [ ] Enable TLS/SSL for encrypted connections
- [ ] Use client certificates for highest security
- [ ] Protect password and certificate files (chmod 600)
- [ ] Regular security audits and password updates
- [ ] Monitor authentication failures
- [ ] Keep certificates up to date
- [ ] Use rate limiting to prevent abuse