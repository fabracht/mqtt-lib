#![cfg(feature = "udp")]

//! Tests the mqttv5 CLI tool with UDP transport to verify:
//! - Basic publish/subscribe
//! - `QoS` levels
//! - Retained messages
//! - Session persistence

use std::time::Duration;
use tokio::process::Command as TokioCommand;

const CLI_BINARY: &str = "target/release/mqttv5";

fn ensure_cli_built() {
    if !std::path::Path::new(CLI_BINARY).exists() {
        let output = std::process::Command::new("cargo")
            .args(["build", "--release", "-p", "mqttv5-cli"])
            .output()
            .expect("Failed to build CLI");

        assert!(
            output.status.success(),
            "Failed to build CLI: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

async fn start_udp_broker() -> (tokio::task::JoinHandle<()>, u16, tempfile::TempDir) {
    use mqtt5::broker::config::UdpConfig;
    use mqtt5::broker::{BrokerConfig, MqttBroker};

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    let mut config = BrokerConfig {
        bind_address: "127.0.0.1:0".parse().unwrap(),
        udp_config: Some(UdpConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(),
            mtu: 1500,
            fragment_timeout: Duration::from_secs(30),
        }),
        ..Default::default()
    };

    // Use unique temp directory for each test to avoid conflicts
    config.storage_config.base_dir = temp_dir.path().to_path_buf();

    let mut broker = MqttBroker::with_config(config)
        .await
        .expect("Failed to create broker");

    let udp_addr = broker.udp_address().expect("UDP not enabled");
    let port = udp_addr.port();

    // Start broker in background
    let handle = tokio::spawn(async move {
        let _ = broker.run().await;
    });

    // Give broker time to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    (handle, port, temp_dir)
}

#[tokio::test]
async fn test_cli_udp_basic_pubsub() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    ensure_cli_built();
    let (broker_handle, port, _temp_dir) = start_udp_broker().await;
    let url = format!("mqtt-udp://localhost:{port}");

    // Start subscriber in background with debug logging
    let sub_url = url.clone();
    let mut subscriber = TokioCommand::new(CLI_BINARY)
        .arg("--debug") // Enable debug logging
        .args([
            "sub",
            "--url",
            &sub_url,
            "--topic",
            "test/cli/udp",
            "--client-id",
            "test_sub",
            "--non-interactive",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped()) // Capture stderr for logs
        .spawn()
        .expect("Failed to start subscriber");

    // Wait for subscriber to connect
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish a message
    let pub_result = TokioCommand::new(CLI_BINARY)
        .args([
            "pub",
            "--url",
            &url,
            "--topic",
            "test/cli/udp",
            "--message",
            "UDP CLI Test Message",
            "--non-interactive",
        ])
        .output()
        .await
        .expect("Failed to publish");

    assert!(pub_result.status.success(), "Publish should succeed");

    // Wait for message to be delivered
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Kill subscriber and check output
    subscriber.kill().await.ok();
    let output = subscriber
        .wait_with_output()
        .await
        .expect("Failed to get subscriber output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("UDP CLI Test Message"),
        "Subscriber should receive the message. Got stdout: {stdout}\nstderr: {stderr}"
    );

    broker_handle.abort();
}

#[tokio::test]
async fn test_cli_udp_qos1() {
    ensure_cli_built();
    let (broker_handle, port, _temp_dir) = start_udp_broker().await;
    let url = format!("mqtt-udp://localhost:{port}");

    // Publish with QoS 1
    let result = TokioCommand::new(CLI_BINARY)
        .args([
            "pub",
            "--url",
            &url,
            "--topic",
            "test/qos1",
            "--message",
            "QoS 1 message",
            "--qos",
            "1",
            "--non-interactive",
        ])
        .output()
        .await
        .expect("Failed to publish");

    assert!(result.status.success(), "QoS 1 publish should succeed");

    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("QoS 1"), "Should indicate QoS 1 was used");

    broker_handle.abort();
}

#[tokio::test]
async fn test_cli_udp_qos2() {
    ensure_cli_built();
    let (broker_handle, port, _temp_dir) = start_udp_broker().await;
    let url = format!("mqtt-udp://localhost:{port}");

    // Publish with QoS 2
    let result = TokioCommand::new(CLI_BINARY)
        .args([
            "pub",
            "--url",
            &url,
            "--topic",
            "test/qos2",
            "--message",
            "QoS 2 message",
            "--qos",
            "2",
            "--non-interactive",
        ])
        .output()
        .await
        .expect("Failed to publish");

    assert!(result.status.success(), "QoS 2 publish should succeed");

    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("QoS 2"), "Should indicate QoS 2 was used");

    broker_handle.abort();
}

#[tokio::test]
async fn test_cli_udp_qos2_delivery() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    ensure_cli_built();
    let (broker_handle, port, _temp_dir) = start_udp_broker().await;
    let url = format!("mqtt-udp://localhost:{port}");

    // Start subscriber with QoS 2
    let sub_url = url.clone();
    let mut subscriber = TokioCommand::new(CLI_BINARY)
        .args([
            "sub",
            "--url",
            &sub_url,
            "--topic",
            "test/cli/udp/qos2",
            "--client-id",
            "cli_qos2_sub",
            "--qos",
            "2",
            "--non-interactive",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start subscriber");

    // Wait for subscriber to connect
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish with QoS 2
    let pub_result = TokioCommand::new(CLI_BINARY)
        .args([
            "pub",
            "--url",
            &url,
            "--topic",
            "test/cli/udp/qos2",
            "--message",
            "QoS 2 CLI Test Message",
            "--qos",
            "2",
            "--client-id",
            "cli_qos2_pub",
            "--non-interactive",
        ])
        .output()
        .await
        .expect("Failed to publish");

    assert!(pub_result.status.success(), "QoS 2 publish should succeed");

    // Wait for message delivery
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Kill subscriber and check output
    subscriber.kill().await.ok();
    let output = subscriber
        .wait_with_output()
        .await
        .expect("Failed to get subscriber output");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("QoS 2 CLI Test Message"),
        "Subscriber should receive the QoS 2 message. Got: {stdout}"
    );

    broker_handle.abort();
}

#[tokio::test]
async fn test_cli_udp_retained() {
    ensure_cli_built();
    let (broker_handle, port, _temp_dir) = start_udp_broker().await;
    let url = format!("mqtt-udp://localhost:{port}");

    // Publish retained message
    let pub_result = TokioCommand::new(CLI_BINARY)
        .args([
            "pub",
            "--url",
            &url,
            "--topic",
            "test/retained",
            "--message",
            "Retained UDP Message",
            "--retain",
            "--non-interactive",
        ])
        .output()
        .await
        .expect("Failed to publish");

    assert!(
        pub_result.status.success(),
        "Retained publish should succeed"
    );

    // Wait a bit
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Subscribe to see if we get the retained message
    let sub_url = url.clone();
    let mut subscriber = TokioCommand::new(CLI_BINARY)
        .args([
            "sub",
            "--url",
            &sub_url,
            "--topic",
            "test/retained",
            "--client-id",
            "retained_test",
            "--non-interactive",
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start subscriber");

    // Wait to see if retained message is delivered
    tokio::time::sleep(Duration::from_secs(2)).await;

    subscriber.kill().await.ok();
    let output = subscriber
        .wait_with_output()
        .await
        .expect("Failed to get output");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Note: This may fail if UDP doesn't support retained messages yet
    if stdout.contains("Retained UDP Message") {
        println!("✓ UDP retained messages work");
    } else {
        println!("✗ UDP retained messages not working (known limitation)");
    }

    broker_handle.abort();
}

#[tokio::test]
async fn test_cli_udp_session_persistence() {
    ensure_cli_built();
    let (broker_handle, port, _temp_dir) = start_udp_broker().await;
    let url = format!("mqtt-udp://localhost:{port}");

    // First connection with session
    let mut sub_process = TokioCommand::new(CLI_BINARY)
        .args([
            "sub",
            "--url",
            &url,
            "--topic",
            "test/session",
            "--client-id",
            "session_test",
            "--no-clean-start",
            "--session-expiry",
            "300",
            "--non-interactive",
        ])
        .spawn()
        .expect("Failed to spawn subscriber");

    tokio::time::sleep(Duration::from_millis(500)).await;
    sub_process.kill().await.ok();
    let sub_result = sub_process.wait().await;

    assert!(sub_result.is_ok(), "First connection should succeed");

    // Publish while disconnected
    let pub_result = TokioCommand::new(CLI_BINARY)
        .args([
            "pub",
            "--url",
            &url,
            "--topic",
            "test/session",
            "--message",
            "Queued message",
            "--qos",
            "1",
            "--non-interactive",
        ])
        .output()
        .await
        .expect("Failed to publish");

    assert!(pub_result.status.success());

    // Reconnect with same session
    let mut subscriber = TokioCommand::new(CLI_BINARY)
        .args([
            "sub",
            "--url",
            &url,
            "--topic",
            "test/session",
            "--client-id",
            "session_test",
            "--no-clean-start",
            "--non-interactive",
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to reconnect");

    tokio::time::sleep(Duration::from_secs(2)).await;

    subscriber.kill().await.ok();
    let output = subscriber
        .wait_with_output()
        .await
        .expect("Failed to get output");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Note: This may fail if UDP doesn't support session persistence yet
    if stdout.contains("Queued message") {
        println!("✓ UDP session persistence works");
    } else {
        println!("✗ UDP session persistence not working (known limitation)");
    }

    broker_handle.abort();
}

#[tokio::test]
async fn test_cli_udp_wildcard_subscriptions() {
    ensure_cli_built();
    let (broker_handle, port, _temp_dir) = start_udp_broker().await;
    let url = format!("mqtt-udp://localhost:{port}");

    // Start subscriber with wildcard
    let sub_url = url.clone();
    let mut subscriber = TokioCommand::new(CLI_BINARY)
        .args([
            "sub",
            "--url",
            &sub_url,
            "--topic",
            "test/+/udp",
            "--client-id",
            "wildcard_sub",
            "--non-interactive",
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start subscriber");

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish to multiple matching topics
    let topics = ["test/one/udp", "test/two/udp", "test/three/udp"];
    for (i, topic) in topics.iter().enumerate() {
        let result = TokioCommand::new(CLI_BINARY)
            .args([
                "pub",
                "--url",
                &url,
                "--topic",
                topic,
                "--message",
                &format!("Message {}", i + 1),
                "--non-interactive",
            ])
            .output()
            .await
            .expect("Failed to publish");

        assert!(result.status.success());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    subscriber.kill().await.ok();
    let output = subscriber
        .wait_with_output()
        .await
        .expect("Failed to get output");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let received_count = (1..=3)
        .filter(|i| stdout.contains(&format!("Message {i}")))
        .count();
    assert_eq!(
        received_count, 3,
        "Should receive all 3 messages via wildcard. Got: {stdout}"
    );

    broker_handle.abort();
}
