//! Tests for CLI with UDP reliability layer

use mqtt5::client::MqttClient;
use mqtt5::ConnectOptions;
use std::process::Command;
use std::time::Duration;
use tokio::time::timeout;

const CLI_BINARY: &str = "target/release/mqttv5";

fn ensure_cli_built() {
    if !std::path::Path::new(CLI_BINARY).exists() {
        let output = Command::new("cargo")
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

async fn start_udp_broker() -> (tokio::task::JoinHandle<()>, u16) {
    use mqtt5::broker::config::UdpConfig;
    use mqtt5::broker::{BrokerConfig, MqttBroker};

    let config = BrokerConfig {
        bind_address: "127.0.0.1:0".parse().unwrap(),
        udp_config: Some(UdpConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(),
            mtu: 1500,
            fragment_timeout: Duration::from_secs(30),
        }),
        ..Default::default()
    };

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

    (handle, port)
}

#[tokio::test]
async fn test_udp_client_with_reliability() {
    use mqtt5::transport::udp::{UdpConfig, UdpTransport};
    use mqtt5::transport::Transport;

    let (broker_handle, port) = start_udp_broker().await;
    let addr = format!("127.0.0.1:{port}").parse().unwrap();

    // Create client with UDP transport with reliability enabled (default)
    let config = UdpConfig::new(addr);
    let mut transport = UdpTransport::new(config);

    // Connect should work with reliability layer
    transport.connect().await.expect("Should connect");

    // Test write and read
    let test_data = b"Hello UDP with reliability";
    transport.write(test_data).await.expect("Should write");

    // Clean up
    broker_handle.abort();
}

#[tokio::test]
async fn test_mqtt_client_over_udp_with_reliability() {
    let (broker_handle, port) = start_udp_broker().await;

    // Test using the MQTT client with UDP
    let client = MqttClient::new("test-udp-client");
    let options = ConnectOptions::new("test-udp-client");

    let url = format!("mqtt-udp://127.0.0.1:{port}");

    // This should use the reliability layer automatically
    let connect_result = timeout(
        Duration::from_secs(5),
        client.connect_with_options(&url, options),
    )
    .await;

    assert!(connect_result.is_ok(), "Should connect within timeout");
    assert!(connect_result.unwrap().is_ok(), "Connection should succeed");

    // Test publish
    let publish_result = client.publish("test/udp", b"Test message".to_vec()).await;

    assert!(publish_result.is_ok(), "Publish should succeed");

    client.disconnect().await.ok();
    broker_handle.abort();
}

#[tokio::test]
async fn test_cli_udp_basic_with_reliability() {
    ensure_cli_built();
    let (broker_handle, port) = start_udp_broker().await;
    let url = format!("mqtt-udp://127.0.0.1:{port}");

    // Simple publish test using tokio::process::Command
    let result = timeout(
        Duration::from_secs(5),
        tokio::process::Command::new(CLI_BINARY)
            .args([
                "pub",
                "--url",
                &url,
                "--topic",
                "test/cli",
                "--message",
                "Test with reliability",
                "--non-interactive",
            ])
            .output(),
    )
    .await;

    assert!(result.is_ok(), "Command should complete within timeout");
    let output = result.unwrap().expect("Failed to run command");
    assert!(output.status.success(), "CLI publish should succeed");

    broker_handle.abort();
}
