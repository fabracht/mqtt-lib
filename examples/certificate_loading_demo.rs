//! Certificate Loading Demonstration
//!
//! This example shows different ways to load certificates for TLS connections:
//! 1. Loading from files (existing method)
//! 2. Loading from bytes in memory (new method)
//! 3. Both PEM and DER formats
//!
//! This is particularly useful for:
//! - Cloud deployments where certificates are stored as secrets
//! - Containerized applications with embedded certificates
//! - Applications that retrieve certificates from secure key stores
//! - Dynamic certificate provisioning systems
//!
//! ## Usage
//!
//! ```bash
//! # Demo with file-based certificates (if available)
//! cargo run --example certificate_loading_demo file
//!
//! # Demo with embedded certificate bytes  
//! cargo run --example certificate_loading_demo bytes
//!
//! # Demo both approaches
//! cargo run --example certificate_loading_demo
//! ```

use mqtt_v5::{client::MqttClient, transport::tls::TlsConfig};
use std::env;
use std::net::SocketAddr;
use tracing::{info, warn};

/// Demo certificate data (self-signed, for demonstration only)
const DEMO_CERT_PEM: &[u8] = b"-----BEGIN CERTIFICATE-----
MIIBkTCB+wIJALRJF4QlQZq2MA0GCSqGSIb3DQEBCwUAMBQxEjAQBgNVBAMMCWxv
Y2FsaG9zdDAeFw0yNDAxMDEwMDAwMDBaFw0yNTAxMDEwMDAwMDBaMBQxEjAQBgNV
BAMMCWxvY2FsaG9zdDBcMA0GCSqGSIb3DQEBAQUAA0sAMEgCQQC7VJTUt9Us8cKB
UmRKvW2aIGMBMExdqGp4ncyf4BzGTOIUtShgP6+u7kj7mBH2q9sP5bOxFKqQSzAD
g8hSN4z8z2o3GYUBj5uEJjh8iVR1OGlmv0iYgzgZWj5Jw7BLG0HMwNfb+H4hTlgc
pZYH8gMxmGQiQmOxSKNJAz5xPJTBGNJjvP+Z3Nd8bQe2qnOz4Hp3s2qs7C4Gq
aPVP5q7LxXIAgIDAQABMA0GCSqGSIb3DQEBCwUAA0EANQfUSRkgFfPb0K9VkbNj
PwX8FnQ+zjqAVHCtjpB+5jdYG3TQmFfQ7EaQdKZGKMWKyGKIQ9fhFvTmI8OU6Y6V
TA==
-----END CERTIFICATE-----";

const DEMO_KEY_PEM: &[u8] = b"-----BEGIN RSA PRIVATE KEY-----
MIIBOgIBAAJBALtUlNS31SzxwoFSZEq9bZogYwEwTF2oanidzJ/gHMZM4hS1KGA/
r67uSPuYEfar2w/ls7EUqpBLMAODyFI3jPzPajcZhQGPm4QmOHyJVHU4aWa/SJiD
OBlaOknDsEsbQczA19v4fiFOWByllgfyAzGYZCJCY7FIo0kDPnE8lMEY0mO8/5nc
13xtB7aqc7PgenezaqzsLgapo9U/mrsvFcgCAgMBAAECQBKmZi7m2J+5nEoM0YKU
wQgRqT2kFz8tJO0Q9r4rQfkbFm8OmVZs9FcX+Z8vCcOqS8nG0z8cRGhX+rKhRrVu
uoECIQDdwJmRZQhCGpX0P8Q6v5B2J7mOZQVg7VK1g4YFcYHyeQIhANJFfHjHgKqJ
x8Z9fQzK8u0FDlq0wGHkL1rCgJzQLHmBAiEA6VjXlZGhF2G8EL4P+7+P6u6W2Qrb
u9W5m0K4kV2sQ2ECIDTqoHEfL2+OzPsQpBxZ5kD6XpGuL6UKYXyF+VZw9uGBAiBm
QK8Q2JGfQtK+7F6vGgR8QKrMgJh6EwZhLl3mPVH+QQ==
-----END RSA PRIVATE KEY-----";

const DEMO_CA_PEM: &[u8] = b"-----BEGIN CERTIFICATE-----
MIIBkTCB+wIJALRJF4QlQZq2MA0GCSqGSIb3DQEBCwUAMBQxEjAQBgNVBAMMCWxv
Y2FsaG9zdDAeFw0yNDAxMDEwMDAwMDBaFw0yNTAxMDEwMDAwMDBaMBQxEjAQBgNV
BAMMCWxvY2FsaG9zdDBcMA0GCSqGSIb3DQEBAQUAA0sAMEgCQQC7VJTUt9Us8cKB
UmRKvW2aIGMBMExdqGp4ncyf4BzGTOIUtShgP6+u7kj7mBH2q9sP5bOxFKqQSzAD
g8hSN4z8z2o3GYUBj5uEJjh8iVR1OGlmv0iYgzgZWj5Jw7BLG0HMwNfb+H4hTlgc
pZYH8gMxmGQiQmOxSKNJAz5xPJTBGNJjvP+Z3Nd8bQe2qnOz4Hp3s2qs7C4Gq
aPVP5q7LxXIAgIDAQABMA0GCSqGSIb3DQEBCwUAA0EANQfUSRkgFfPb0K9VkbNj
PwX8FnQ+zjqAVHCtjpB+5jdYG3TQmFfQ7EaQdKZGKMWKyGKIQ9fhFvTmI8OU6Y6V
TA==
-----END CERTIFICATE-----";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("certificate_loading_demo=info,mqtt_v5=debug")
        .init();

    let args: Vec<String> = env::args().collect();
    let demo_mode = args.get(1).map(|s| s.as_str()).unwrap_or("both");

    info!("=== Certificate Loading Demonstration ===");

    match demo_mode {
        "file" => demo_file_based_loading().await?,
        "bytes" => demo_byte_based_loading().await?,
        _ => {
            demo_file_based_loading().await?;
            demo_byte_based_loading().await?;
        }
    }

    Ok(())
}

/// Demonstrate loading certificates from files (traditional method)
async fn demo_file_based_loading() -> Result<(), Box<dyn std::error::Error>> {
    info!("--- File-Based Certificate Loading ---");

    let addr: SocketAddr = "127.0.0.1:8883".parse()?;
    let mut tls_config = TlsConfig::new(addr, "localhost");

    // Try to load from actual files if they exist
    match (
        tls_config.load_client_cert_pem("test_certs/client.pem"),
        tls_config.load_client_key_pem("test_certs/client.key"),
        tls_config.load_ca_cert_pem("test_certs/ca.pem"),
    ) {
        (Ok(()), Ok(()), Ok(())) => {
            info!("✅ Successfully loaded certificates from files");
            info!("   - Client certificate: test_certs/client.pem");
            info!("   - Client key: test_certs/client.key");
            info!("   - CA certificate: test_certs/ca.pem");
        }
        _ => {
            warn!("⚠️  Certificate files not found in test_certs/ directory");
            info!("   This is expected if test certificates haven't been generated");
            info!("   Run: ./scripts/generate_test_certs.sh to create them");
        }
    }

    // Demonstrate the configuration
    let _tls_config = tls_config
        .with_system_roots(false)
        .with_verify_server_cert(true);

    info!("TLS configuration ready for file-based certificates");
    Ok(())
}

/// Demonstrate loading certificates from bytes in memory (new method)
async fn demo_byte_based_loading() -> Result<(), Box<dyn std::error::Error>> {
    info!("--- Byte-Based Certificate Loading ---");

    let addr: SocketAddr = "127.0.0.1:8883".parse()?;
    let mut tls_config = TlsConfig::new(addr, "localhost");

    // Load certificates from embedded bytes (PEM format)
    info!("Loading certificates from embedded PEM bytes...");
    tls_config.load_client_cert_pem_bytes(DEMO_CERT_PEM)?;
    tls_config.load_client_key_pem_bytes(DEMO_KEY_PEM)?;
    tls_config.load_ca_cert_pem_bytes(DEMO_CA_PEM)?;

    info!("✅ Successfully loaded certificates from PEM bytes");
    info!("   - Client certificate: {} bytes", DEMO_CERT_PEM.len());
    info!("   - Client key: {} bytes", DEMO_KEY_PEM.len());
    info!("   - CA certificate: {} bytes", DEMO_CA_PEM.len());

    // Configure TLS settings
    let _tls_config = tls_config
        .with_system_roots(false)
        .with_verify_server_cert(true);

    // Demonstrate creating a client with byte-loaded certificates
    info!("Creating MQTT client with byte-loaded certificates...");
    let _client = MqttClient::new("demo_client_bytes");

    // Note: We don't actually connect since we don't have a real broker
    // but this shows how the configuration would be used
    info!("TLS configuration ready for byte-based certificates");

    // Demonstrate DER format loading as well
    demo_der_format_loading().await?;

    Ok(())
}

/// Demonstrate loading DER format certificates
async fn demo_der_format_loading() -> Result<(), Box<dyn std::error::Error>> {
    info!("--- DER Format Certificate Loading ---");

    let addr: SocketAddr = "127.0.0.1:8883".parse()?;
    let mut tls_config = TlsConfig::new(addr, "localhost");

    // For demo purposes, we'll show how to use the DER methods
    // In practice, you'd have actual DER-encoded bytes
    let dummy_cert_der = vec![0x30, 0x82, 0x01, 0x00]; // Basic DER structure
    let dummy_key_der = vec![0x30, 0x48, 0x02, 0x01]; // Basic DER structure for key
    let dummy_ca_der = vec![0x30, 0x82, 0x01, 0x00]; // Basic DER structure

    info!("Loading certificates from DER bytes...");
    tls_config.load_client_cert_der_bytes(&dummy_cert_der)?;
    tls_config.load_client_key_der_bytes(&dummy_key_der)?;
    tls_config.load_ca_cert_der_bytes(&dummy_ca_der)?;

    info!("✅ Successfully loaded certificates from DER bytes");
    info!(
        "   - Client certificate: {} bytes (DER format)",
        dummy_cert_der.len()
    );
    info!(
        "   - Client key: {} bytes (DER format)",
        dummy_key_der.len()
    );
    info!(
        "   - CA certificate: {} bytes (DER format)",
        dummy_ca_der.len()
    );

    info!("TLS configuration ready for DER-based certificates");
    Ok(())
}

/// Example of how certificates might be loaded in different deployment scenarios
#[allow(dead_code)]
async fn deployment_scenarios() -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Real-World Deployment Scenarios ===");

    // Scenario 1: Kubernetes Secrets
    info!("Scenario 1: Loading from Kubernetes secrets mounted as files");
    let addr: SocketAddr = "broker.company.com:8883".parse()?;
    let mut k8s_config = TlsConfig::new(addr, "broker.company.com");

    // In Kubernetes, secrets are often mounted as files
    if k8s_config
        .load_client_cert_pem("/var/secrets/tls/tls.crt")
        .is_ok()
        && k8s_config
            .load_client_key_pem("/var/secrets/tls/tls.key")
            .is_ok()
        && k8s_config
            .load_ca_cert_pem("/var/secrets/ca/ca.crt")
            .is_ok()
    {
        info!("✅ Loaded certificates from Kubernetes secret mounts");
    }

    // Scenario 2: Environment Variables (raw PEM in env vars)
    info!("Scenario 2: Loading from environment variables");
    let mut env_config = TlsConfig::new(addr, "broker.company.com");

    if let (Ok(cert_pem), Ok(key_pem), Ok(ca_pem)) = (
        env::var("TLS_CERT_PEM"),
        env::var("TLS_KEY_PEM"),
        env::var("TLS_CA_PEM"),
    ) {
        // Load directly from PEM strings in environment variables
        env_config.load_client_cert_pem_bytes(cert_pem.as_bytes())?;
        env_config.load_client_key_pem_bytes(key_pem.as_bytes())?;
        env_config.load_ca_cert_pem_bytes(ca_pem.as_bytes())?;

        info!("✅ Loaded certificates from PEM environment variables");
    }

    // Scenario 3: HashiCorp Vault or similar secret store
    info!("Scenario 3: Loading from secret management system");
    let mut vault_config = TlsConfig::new(addr, "broker.company.com");

    // Example: retrieve from a hypothetical secret store
    let cert_bytes = retrieve_secret("pki/cert/mqtt-client").await?;
    let key_bytes = retrieve_secret("pki/key/mqtt-client").await?;
    let ca_bytes = retrieve_secret("pki/ca/root").await?;

    vault_config.load_client_cert_pem_bytes(&cert_bytes)?;
    vault_config.load_client_key_pem_bytes(&key_bytes)?;
    vault_config.load_ca_cert_pem_bytes(&ca_bytes)?;

    info!("✅ Loaded certificates from secret management system");

    Ok(())
}

/// Mock function representing secret retrieval from a secret store
#[allow(dead_code)]
async fn retrieve_secret(path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // In a real implementation, this would make API calls to your secret store
    info!("Retrieving secret from path: {}", path);
    Ok(DEMO_CERT_PEM.to_vec()) // Return demo cert for illustration
}
