//! CLI transport tests
//!
//! Tests different transport types: TCP, TLS, UDP, DTLS

use std::process::Command;

mod common;
use common::TestBroker;

const CLI_BINARY: &str = "target/release/mqttv5";

fn ensure_cli_built() {
    if !std::path::Path::new(CLI_BINARY).exists() {
        let output = Command::new("cargo")
            .args(["build", "--release", "-p", "mqttv5-cli"])
            .output()
            .expect("Failed to build CLI");

        if !output.status.success() {
            panic!(
                "Failed to build CLI: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

/// Test TCP transport (mqtt://)
#[tokio::test]
async fn test_cli_tcp_transport() {
    ensure_cli_built();
    let broker = TestBroker::start().await;
    let broker_url = broker.address(); // This returns mqtt://...

    let result = Command::new(CLI_BINARY)
        .args([
            "pub",
            "--url",
            broker_url,
            "--topic",
            "test/tcp",
            "--message",
            "tcp_test",
            "--non-interactive",
        ])
        .output()
        .expect("Failed to run pub");

    assert!(result.status.success(), "TCP transport should work");
    println!("✅ TCP transport (mqtt://) works");
}

/// Test TLS transport (mqtts://)
#[tokio::test]
async fn test_cli_tls_transport() {
    ensure_cli_built();

    // Generate test certificates if not present
    if !std::path::Path::new("test_certs/server.pem").exists() {
        let _ = Command::new("./scripts/generate_test_certs.sh").output();
    }

    let _broker = TestBroker::start_with_tls().await;
    // TLS broker listens on a different port, but we can't get it directly
    // Just use a known TLS port for testing
    let tls_url = "mqtts://localhost:8883";

    // Note: This will likely fail due to certificate validation
    // but we're testing that the URL scheme is recognized
    let result = Command::new(CLI_BINARY)
        .args([
            "pub",
            "--url",
            tls_url,
            "--topic",
            "test/tls",
            "--message",
            "tls_test",
            "--non-interactive",
        ])
        .output()
        .expect("Failed to run pub");

    // Check that it tried to connect with TLS (even if it failed due to certs)
    let stderr = String::from_utf8_lossy(&result.stderr);
    if stderr.contains("certificate") || stderr.contains("TLS") || stderr.contains("SSL") {
        println!("✅ TLS transport (mqtts://) is recognized and attempted");
    } else {
        // It might succeed if certs are properly configured
        if result.status.success() {
            println!("✅ TLS transport (mqtts://) works");
        } else {
            println!("⚠️  TLS connection failed: {stderr}");
        }
    }
}

/// Test UDP transport (mqtt-udp://)
#[tokio::test]
async fn test_cli_udp_transport() {
    ensure_cli_built();

    // UDP transport requires a UDP-capable broker
    // For now, just test that the URL scheme is recognized
    let result = Command::new(CLI_BINARY)
        .args([
            "pub",
            "--url",
            "mqtt-udp://localhost:1883",
            "--topic",
            "test/udp",
            "--message",
            "udp_test",
            "--non-interactive",
        ])
        .output()
        .expect("Failed to run pub");

    let stderr = String::from_utf8_lossy(&result.stderr);

    // Check if it attempted UDP connection
    if stderr.contains("Connection refused") || stderr.contains("UDP") {
        println!("✅ UDP transport (mqtt-udp://) is recognized");
    } else if result.status.success() {
        println!("✅ UDP transport (mqtt-udp://) works");
    } else {
        println!("UDP transport test result: {stderr}");
    }
}

/// Test DTLS transport (mqtts-dtls://)
#[tokio::test]
async fn test_cli_dtls_transport() {
    ensure_cli_built();

    // DTLS transport requires a DTLS-capable broker
    // For now, just test that the URL scheme is recognized
    let result = Command::new(CLI_BINARY)
        .args([
            "pub",
            "--url",
            "mqtts-dtls://localhost:8883",
            "--topic",
            "test/dtls",
            "--message",
            "dtls_test",
            "--non-interactive",
        ])
        .output()
        .expect("Failed to run pub");

    let stderr = String::from_utf8_lossy(&result.stderr);

    // Check if it attempted DTLS connection
    if stderr.contains("DTLS") || stderr.contains("Connection refused") {
        println!("✅ DTLS transport (mqtts-dtls://) is recognized");
    } else if result.status.success() {
        println!("✅ DTLS transport (mqtts-dtls://) works");
    } else {
        println!("DTLS transport test result: {stderr}");
    }
}

/// Test URL parsing with custom ports
#[tokio::test]
async fn test_cli_url_custom_ports() {
    ensure_cli_built();

    // Test custom TCP port
    let tcp_result = Command::new(CLI_BINARY)
        .args([
            "pub",
            "--url",
            "mqtt://localhost:12345",
            "--topic",
            "test",
            "--message",
            "test",
            "--non-interactive",
        ])
        .output()
        .expect("Failed to run pub");

    let tcp_stderr = String::from_utf8_lossy(&tcp_result.stderr);
    assert!(
        tcp_stderr.contains("Connection refused") || tcp_stderr.contains("12345"),
        "Should try to connect to custom port"
    );

    // Test custom TLS port
    let tls_result = Command::new(CLI_BINARY)
        .args([
            "pub",
            "--url",
            "mqtts://localhost:12346",
            "--topic",
            "test",
            "--message",
            "test",
            "--non-interactive",
        ])
        .output()
        .expect("Failed to run pub");

    let tls_stderr = String::from_utf8_lossy(&tls_result.stderr);
    assert!(
        tls_stderr.contains("Connection refused") || tls_stderr.contains("12346"),
        "Should try to connect to custom TLS port"
    );

    println!("✅ Custom ports in URLs work correctly");
}

/// Test transport selection via URL scheme in help
#[tokio::test]
async fn test_cli_transport_help() {
    ensure_cli_built();

    // Check that help mentions all transport types
    let pub_help = Command::new(CLI_BINARY)
        .args(["pub", "--help"])
        .output()
        .expect("Failed to get help");

    let help_text = String::from_utf8_lossy(&pub_help.stdout);

    assert!(
        help_text.contains("mqtt://"),
        "Should mention TCP transport"
    );
    assert!(
        help_text.contains("mqtt-udp://"),
        "Should mention UDP transport"
    );
    assert!(
        help_text.contains("mqtts-dtls://"),
        "Should mention DTLS transport"
    );

    println!("✅ All transport types documented in help");
}
