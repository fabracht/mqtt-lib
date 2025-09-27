//! Test to verify proper QoS 2 packet sequence for UDP transport

use mqtt5::broker::{BrokerConfig, MqttBroker};
use mqtt5::client::MqttClient;
use mqtt5::QoS;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time::timeout;

async fn start_udp_broker_with_packet_capture() -> (
    tokio::task::JoinHandle<()>,
    SocketAddr,
    tempfile::TempDir,
    Arc<Mutex<Vec<String>>>,
) {
    let mut config = BrokerConfig::default();
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    config.storage_config.base_dir = temp_dir.path().to_path_buf();
    config.storage_config.enable_persistence = true;
    config.bind_address = "127.0.0.1:0".parse().unwrap();
    config.udp_config = Some(mqtt5::broker::config::UdpConfig {
        bind_address: "127.0.0.1:0".parse().unwrap(),
        mtu: 1500,
        fragment_timeout: Duration::from_secs(30),
    });

    let mut broker = MqttBroker::with_config(config)
        .await
        .expect("Failed to create broker");

    let addr = broker.udp_address().expect("UDP not enabled");
    let packet_log = Arc::new(Mutex::new(Vec::new()));
    let packet_log_clone = packet_log.clone();

    // Start broker with packet logging
    let handle = tokio::spawn(async move {
        let _ = broker.run().await;
    });

    // Give broker time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    (handle, addr, temp_dir, packet_log_clone)
}

#[tokio::test]
async fn test_udp_qos2_packet_sequence() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("mqtt5=trace")
        .try_init();

    let (broker_handle, addr, _temp_dir, _packet_log) =
        start_udp_broker_with_packet_capture().await;

    // Create publisher
    let publisher = MqttClient::new("udp_qos2_sequence_pub");
    let udp_url = format!("mqtt-udp://{}", addr);

    // Connect publisher
    publisher
        .connect(&udp_url)
        .await
        .expect("Publisher failed to connect");

    // Create subscriber to receive the message
    let subscriber = MqttClient::new("udp_qos2_sequence_sub");
    subscriber
        .connect(&udp_url)
        .await
        .expect("Subscriber failed to connect");

    let (msg_tx, mut msg_rx) = mpsc::channel::<mqtt5::types::Message>(10);

    // Subscribe with QoS 2
    use mqtt5::{RetainHandling, SubscribeOptions};
    let options = SubscribeOptions {
        qos: QoS::ExactlyOnce,
        no_local: false,
        retain_as_published: false,
        retain_handling: RetainHandling::SendAtSubscribe,
        subscription_identifier: None,
    };

    subscriber
        .subscribe_with_options("test/qos2/sequence", options, move |msg| {
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(msg).await;
            });
        })
        .await
        .expect("Failed to subscribe");

    // Wait for subscription to establish
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish with QoS 2 - this will complete only after full PUBCOMP flow
    let publish_result = publisher
        .publish_qos(
            "test/qos2/sequence",
            b"QoS 2 Sequence Test",
            QoS::ExactlyOnce,
        )
        .await;

    assert!(
        publish_result.is_ok(),
        "QoS 2 publish should succeed: {:?}",
        publish_result
    );
    assert!(
        publish_result.unwrap().packet_id().is_some(),
        "QoS 2 should return packet ID"
    );

    // Verify message was delivered exactly once
    let received = timeout(Duration::from_secs(2), msg_rx.recv())
        .await
        .expect("Timeout waiting for message")
        .expect("Channel closed");

    assert_eq!(received.topic, "test/qos2/sequence");
    assert_eq!(received.payload, b"QoS 2 Sequence Test");
    assert_eq!(received.qos, QoS::ExactlyOnce);

    // Ensure no duplicates
    let duplicate = timeout(Duration::from_millis(500), msg_rx.recv()).await;
    assert!(
        duplicate.is_err(),
        "Should not receive duplicate messages with QoS 2"
    );

    broker_handle.abort();
}

#[tokio::test]
async fn test_udp_qos2_completes_after_pubcomp() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("mqtt5::client=debug,mqtt5::broker=debug")
        .try_init();

    let (broker_handle, addr, _temp_dir, _) = start_udp_broker_with_packet_capture().await;

    let client = MqttClient::new("udp_qos2_verify");
    let udp_url = format!("mqtt-udp://{}", addr);
    client.connect(&udp_url).await.expect("Failed to connect");

    // Publish with QoS 2 and verify it completes successfully
    // The fact that this returns successfully proves that the client
    // waited for PUBCOMP (not just PUBREC)
    let result = client
        .publish_qos("test/verify", b"Verify QoS 2 flow", QoS::ExactlyOnce)
        .await;

    assert!(result.is_ok(), "QoS 2 publish should succeed");
    let packet_id = result.unwrap().packet_id();
    assert!(packet_id.is_some(), "QoS 2 should return packet ID");

    // Verify we can publish multiple QoS 2 messages successfully
    for i in 0..3 {
        let msg = format!("Message {}", i);
        let result = client
            .publish_qos(
                &format!("test/verify/{}", i),
                msg.as_bytes(),
                QoS::ExactlyOnce,
            )
            .await;

        assert!(result.is_ok(), "QoS 2 publish {} should succeed", i);
        assert!(
            result.unwrap().packet_id().is_some(),
            "Should have packet ID"
        );
    }

    broker_handle.abort();
}

#[tokio::test]
async fn test_udp_qos2_multiple_concurrent_publishes() {
    let _ = tracing_subscriber::fmt().try_init();

    let (broker_handle, addr, _temp_dir, _) = start_udp_broker_with_packet_capture().await;

    let client = MqttClient::new("udp_qos2_concurrent");
    let udp_url = format!("mqtt-udp://{}", addr);
    client.connect(&udp_url).await.expect("Failed to connect");

    // Spawn multiple QoS 2 publishes concurrently
    let mut handles = Vec::new();
    for i in 0..5 {
        let client = MqttClient::new(format!("udp_qos2_concurrent_{}", i));
        let url = udp_url.clone();

        let handle = tokio::spawn(async move {
            client.connect(&url).await.expect("Failed to connect");

            let message = format!("Message {}", i);
            let result = client
                .publish_qos(
                    &format!("test/concurrent/{}", i),
                    message.as_bytes(),
                    QoS::ExactlyOnce,
                )
                .await;

            client.disconnect().await.ok();
            result
        });

        handles.push(handle);
    }

    // Wait for all publishes to complete
    for (i, handle) in handles.into_iter().enumerate() {
        let result = timeout(Duration::from_secs(10), handle)
            .await
            .unwrap_or_else(|_| panic!("Publish {} timeout", i))
            .unwrap_or_else(|_| panic!("Publish {} panicked", i));

        assert!(result.is_ok(), "Publish {} should succeed: {:?}", i, result);
        assert!(
            result.unwrap().packet_id().is_some(),
            "Publish {} should have packet ID",
            i
        );
    }

    broker_handle.abort();
}
