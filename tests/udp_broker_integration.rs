#![cfg(feature = "udp")]

use mqtt5::broker::{BrokerConfig, MqttBroker};
use mqtt5::client::MqttClient;
use mqtt5::QoS;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::mpsc;

async fn start_udp_broker() -> (tokio::task::JoinHandle<()>, SocketAddr, tempfile::TempDir) {
    let mut config = BrokerConfig::default();
    // Use unique temp directory for each test to avoid conflicts
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    config.storage_config.base_dir = temp_dir.path().to_path_buf();
    config.storage_config.enable_persistence = true; // Keep storage enabled for proper testing
                                                     // Disable TCP to avoid port conflicts in parallel tests
    config.bind_addresses = vec!["127.0.0.1:0".parse().unwrap()];
    config.udp_config = Some(mqtt5::broker::config::UdpConfig {
        bind_addresses: vec!["127.0.0.1:0".parse().unwrap()],
        mtu: 1500,
        fragment_timeout: Duration::from_secs(30),
    });

    let mut broker = MqttBroker::with_config(config)
        .await
        .expect("Failed to create broker");

    let addr = broker.udp_address().expect("UDP not enabled");

    // Start broker in background
    let handle = tokio::spawn(async move {
        let _ = broker.run().await;
    });

    // Give broker time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    (handle, addr, temp_dir)
}

#[tokio::test]
async fn test_udp_connect_disconnect() {
    let _ = tracing_subscriber::fmt::try_init();

    let (broker_handle, addr, _temp_dir) = start_udp_broker().await;

    let client = MqttClient::new("udp_test_client");

    let udp_url = format!("mqtt-udp://{addr}");
    eprintln!("TEST: Calling client.connect()");
    client.connect(&udp_url).await.expect("Failed to connect");
    eprintln!("TEST: client.connect() returned successfully");

    eprintln!("TEST: Checking if connected");
    assert!(client.is_connected().await);
    eprintln!("TEST: Client is connected");

    client.disconnect().await.expect("Failed to disconnect");
    assert!(!client.is_connected().await);

    // Cleanup
    broker_handle.abort();
}

#[tokio::test]
async fn test_udp_publish_qos0() {
    let _ = tracing_subscriber::fmt::try_init();

    let (broker_handle, addr, _temp_dir) = start_udp_broker().await;

    let publisher = MqttClient::new("udp_publisher");

    let udp_url = format!("mqtt-udp://{addr}");
    publisher
        .connect(&udp_url)
        .await
        .expect("Failed to connect");

    let subscriber = MqttClient::new("udp_subscriber");

    subscriber
        .connect(&udp_url)
        .await
        .expect("Failed to connect");

    let (tx, mut rx) = mpsc::channel::<mqtt5::types::Message>(10);

    subscriber
        .subscribe("test/udp", move |msg| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(msg).await;
            });
        })
        .await
        .expect("Failed to subscribe");

    publisher
        .publish("test/udp", b"Hello UDP")
        .await
        .expect("Failed to publish");

    let received = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("Timeout waiting for message")
        .expect("Channel closed");

    assert_eq!(received.topic, "test/udp");
    assert_eq!(received.payload, b"Hello UDP");

    // Cleanup
    broker_handle.abort();
}

#[tokio::test]
async fn test_udp_publish_qos1() {
    let _ = tracing_subscriber::fmt::try_init();

    let (broker_handle, addr, _temp_dir) = start_udp_broker().await;

    let client = MqttClient::new("udp_qos1_client");

    let udp_url = format!("mqtt-udp://{addr}");
    client.connect(&udp_url).await.expect("Failed to connect");

    let result = client
        .publish_qos("test/qos1", b"QoS 1 message", QoS::AtLeastOnce)
        .await
        .expect("Failed to publish");

    assert!(
        result.packet_id().unwrap_or(0) > 0,
        "QoS 1 publish should return packet ID"
    );

    // Cleanup
    broker_handle.abort();
}

#[tokio::test]
async fn test_udp_publish_qos2() {
    let _ = tracing_subscriber::fmt::try_init();

    let (broker_handle, addr, _temp_dir) = start_udp_broker().await;

    let client = MqttClient::new("udp_qos2_client");

    let udp_url = format!("mqtt-udp://{addr}");
    client.connect(&udp_url).await.expect("Failed to connect");

    let result = client
        .publish_qos("test/qos2", b"QoS 2 message", QoS::ExactlyOnce)
        .await
        .expect("Failed to publish QoS 2");

    assert!(
        result.packet_id().unwrap_or(0) > 0,
        "QoS 2 publish should return packet ID"
    );

    // Cleanup
    broker_handle.abort();
}

#[tokio::test]
async fn test_udp_fragmentation() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut config = BrokerConfig::default();
    // Use unique temp directory for storage
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    config.storage_config.base_dir = temp_dir.path().to_path_buf();
    // Disable TCP to avoid port conflicts
    config.bind_addresses = vec!["127.0.0.1:0".parse().unwrap()];
    config.udp_config = Some(mqtt5::broker::config::UdpConfig {
        bind_addresses: vec!["127.0.0.1:0".parse().unwrap()],
        mtu: 256, // Small MTU to force fragmentation
        fragment_timeout: Duration::from_secs(30),
    });

    let mut broker = MqttBroker::with_config(config)
        .await
        .expect("Failed to create broker");

    let addr = broker.udp_address().expect("UDP not enabled");

    // Start broker in background
    let broker_handle = tokio::spawn(async move {
        let _ = broker.run().await;
    });

    // Give broker time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = MqttClient::new("udp_fragment_client");

    let udp_url = format!("mqtt-udp://{addr}");
    client.connect(&udp_url).await.expect("Failed to connect");

    // Large payload to force fragmentation
    let large_payload = vec![b'x'; 1024];

    client
        .publish("test/fragment", large_payload.clone())
        .await
        .expect("Failed to publish large message");

    client.disconnect().await.expect("Failed to disconnect");

    // Cleanup
    broker_handle.abort();
}

#[tokio::test]
async fn test_udp_multiple_clients() {
    let _ = tracing_subscriber::fmt::try_init();

    let (broker_handle, addr, _temp_dir) = start_udp_broker().await;

    let mut clients = Vec::new();

    for i in 0..5 {
        let client = MqttClient::new(format!("udp_client_{i}"));

        let udp_url = format!("mqtt-udp://{addr}");
        client.connect(&udp_url).await.expect("Failed to connect");

        clients.push(client);
    }

    // All clients should be connected
    for client in &clients {
        assert!(client.is_connected().await);
    }

    // Disconnect all
    for client in clients {
        client.disconnect().await.expect("Failed to disconnect");
    }

    // Cleanup
    broker_handle.abort();
}

#[tokio::test]
async fn test_udp_ping_keepalive() {
    let _ = tracing_subscriber::fmt::try_init();

    let (broker_handle, addr, _temp_dir) = start_udp_broker().await;

    let client = MqttClient::new("udp_ping_client");

    let udp_url = format!("mqtt-udp://{addr}");
    client.connect(&udp_url).await.expect("Failed to connect");

    // Wait for multiple keepalive intervals
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Client should still be connected
    assert!(client.is_connected().await);

    client.disconnect().await.expect("Failed to disconnect");

    // Cleanup
    broker_handle.abort();
}

#[tokio::test]
async fn test_udp_subscribe_unsubscribe() {
    let _ = tracing_subscriber::fmt::try_init();

    let (broker_handle, addr, _temp_dir) = start_udp_broker().await;

    let client = MqttClient::new("udp_sub_unsub_client");

    let udp_url = format!("mqtt-udp://{addr}");
    client.connect(&udp_url).await.expect("Failed to connect");

    // Subscribe to multiple topics
    let topics = ["topic/1", "topic/2", "topic/3"];

    for topic in &topics {
        client
            .subscribe(*topic, |_msg| {})
            .await
            .expect("Failed to subscribe");
    }

    // Unsubscribe from all
    for topic in &topics {
        client
            .unsubscribe(*topic)
            .await
            .expect("Failed to unsubscribe");
    }

    client.disconnect().await.expect("Failed to disconnect");

    // Cleanup
    broker_handle.abort();
}

#[tokio::test]
async fn test_udp_qos2_delivery() {
    use mqtt5::{RetainHandling, SubscribeOptions};

    let _ = tracing_subscriber::fmt::try_init();

    let (broker_handle, addr, _temp_dir) = start_udp_broker().await;

    // Create publisher and subscriber
    let publisher = MqttClient::new("udp_qos2_pub");
    let subscriber = MqttClient::new("udp_qos2_sub");

    let udp_url = format!("mqtt-udp://{addr}");

    // Connect both clients
    publisher
        .connect(&udp_url)
        .await
        .expect("Publisher failed to connect");
    subscriber
        .connect(&udp_url)
        .await
        .expect("Subscriber failed to connect");

    // Set up message channel
    let (tx, mut rx) = mpsc::channel::<mqtt5::types::Message>(10);

    // Subscribe with QoS 2
    let options = SubscribeOptions {
        qos: QoS::ExactlyOnce,
        no_local: false,
        retain_as_published: false,
        retain_handling: RetainHandling::SendAtSubscribe,
        subscription_identifier: None,
    };

    subscriber
        .subscribe_with_options("test/qos2/delivery", options, move |msg| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(msg).await;
            });
        })
        .await
        .expect("Failed to subscribe");

    // Wait for subscription to be established
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish with QoS 2
    let test_payload = b"QoS 2 Exactly Once Delivery Test";
    let result = publisher
        .publish_qos("test/qos2/delivery", test_payload, QoS::ExactlyOnce)
        .await
        .expect("Failed to publish QoS 2");

    assert!(
        result.packet_id().unwrap_or(0) > 0,
        "QoS 2 should return packet ID"
    );

    // Verify message received exactly once
    let received = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("Timeout waiting for QoS 2 message")
        .expect("Channel closed");

    assert_eq!(received.topic, "test/qos2/delivery");
    assert_eq!(received.payload, test_payload);
    assert_eq!(received.qos, QoS::ExactlyOnce);

    // Ensure no duplicate messages
    let duplicate = tokio::time::timeout(Duration::from_millis(500), rx.recv()).await;
    assert!(
        duplicate.is_err(),
        "Should not receive duplicate messages with QoS 2"
    );

    // Cleanup
    broker_handle.abort();
}

#[tokio::test]
async fn test_udp_qos_levels_mixed() {
    let _ = tracing_subscriber::fmt::try_init();

    let (broker_handle, addr, _temp_dir) = start_udp_broker().await;

    let client = MqttClient::new("udp_qos_mixed");
    let udp_url = format!("mqtt-udp://{addr}");
    client.connect(&udp_url).await.expect("Failed to connect");

    // Test all QoS levels in sequence
    let qos0_result = client
        .publish_qos("test/qos/0", b"QoS 0", QoS::AtMostOnce)
        .await;
    assert!(qos0_result.is_ok(), "QoS 0 publish should succeed");

    let qos1_result = client
        .publish_qos("test/qos/1", b"QoS 1", QoS::AtLeastOnce)
        .await;
    assert!(qos1_result.is_ok(), "QoS 1 publish should succeed");
    assert!(
        qos1_result.unwrap().packet_id().is_some(),
        "QoS 1 should have packet ID"
    );

    let qos2_result = client
        .publish_qos("test/qos/2", b"QoS 2", QoS::ExactlyOnce)
        .await;
    assert!(qos2_result.is_ok(), "QoS 2 publish should succeed");
    assert!(
        qos2_result.unwrap().packet_id().is_some(),
        "QoS 2 should have packet ID"
    );

    // Cleanup
    broker_handle.abort();
}
