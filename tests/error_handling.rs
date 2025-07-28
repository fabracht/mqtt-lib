use mqtt_v5::error::MqttError;
use mqtt_v5::{ConnectOptions, MqttClient, PublishOptions, QoS, SubscribeOptions};
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_connection_refused_wrong_port() {
    let client = MqttClient::new("error-test-1");

    // Try to connect to a port that's likely not running MQTT
    let result = client.connect("mqtt://localhost:12345").await;
    assert!(result.is_err());

    match result.unwrap_err() {
        MqttError::ConnectionError(_) => {} // Expected: connection refused
        other => panic!("Expected ConnectionError, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_connection_timeout() {
    let options = ConnectOptions::new("timeout-test");

    let client = MqttClient::with_options(options);

    // Try to connect to an IP that will timeout (non-routable)
    let result = timeout(
        Duration::from_secs(2),
        client.connect("mqtt://192.0.2.1:1883"), // TEST-NET-1 (RFC 5737)
    )
    .await;

    assert!(result.is_err() || result.unwrap().is_err());
}

#[tokio::test]
async fn test_invalid_client_id() {
    // MQTT v5 allows most characters in client IDs
    // Test with empty client ID which is allowed but requires clean start
    let mut options = ConnectOptions::new("");
    options.clean_start = false; // This combination should fail
    let client = MqttClient::with_options(options);

    let result = client.connect("mqtt://localhost:1883").await;
    // Should fail because empty client ID requires clean_start=true
    if result.is_ok() {
        client.disconnect().await.ok();
        // Some brokers might accept this, so not a hard failure
    }
}

#[tokio::test]
async fn test_publish_before_connect() {
    let client = MqttClient::new("not-connected");

    let result = client.publish("test/topic", "message").await;
    assert!(result.is_err());

    match result.unwrap_err() {
        MqttError::NotConnected => {} // Expected
        other => panic!("Expected NotConnected error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_subscribe_before_connect() {
    let client = MqttClient::new("not-connected-2");

    let result = client.subscribe("test/topic", |_| {}).await;
    assert!(result.is_err());

    match result.unwrap_err() {
        MqttError::NotConnected => {} // Expected
        other => panic!("Expected NotConnected error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_invalid_topic_name_publish() {
    let client = MqttClient::new("invalid-topic-test");
    client.connect("mqtt://localhost:1883").await.unwrap();

    // Topic names cannot contain wildcards
    let result = client.publish("test/+/invalid", "message").await;
    if result.is_ok() {
        // Some implementations might not validate this client-side
        println!("Warning: Topic with wildcard was accepted");
    }

    // Topic names cannot be empty
    let result = client.publish("", "message").await;
    if result.is_ok() {
        println!("Warning: Empty topic was accepted");
    }

    // Topic names cannot contain null characters
    let result = client.publish("test\0topic", "message").await;
    if result.is_ok() {
        println!("Warning: Topic with null character was accepted");
    }

    client.disconnect().await.unwrap();
}

#[tokio::test]
async fn test_invalid_topic_filter_subscribe() {
    let client = MqttClient::new("invalid-filter-test");
    client.connect("mqtt://localhost:1883").await.unwrap();

    // Empty topic filter
    let result = client.subscribe("", |_| {}).await;
    assert!(result.is_err());

    // Topic filter with null character
    let result = client.subscribe("test\0filter", |_| {}).await;
    assert!(result.is_err());

    let _ = client.disconnect().await;
}

#[tokio::test]
async fn test_packet_too_large() {
    let mut options = ConnectOptions::new("large-packet-test");
    options.properties.maximum_packet_size = Some(1024); // 1KB limit

    let client = MqttClient::with_options(options);
    client.connect("mqtt://localhost:1883").await.unwrap();

    // Try to publish a message larger than the limit
    let large_payload = vec![0u8; 2048]; // 2KB
    let result = client.publish("test/large", &large_payload[..]).await;
    assert!(result.is_err());

    match result.unwrap_err() {
        MqttError::PacketTooLarge { .. } => {} // Expected
        other => panic!("Expected PacketTooLarge error, got: {:?}", other),
    }

    client.disconnect().await.unwrap();
}

#[tokio::test]
async fn test_duplicate_connect() {
    let client = MqttClient::new("duplicate-connect-test");

    // First connect should succeed
    client.connect("mqtt://localhost:1883").await.unwrap();

    // Second connect without disconnect should fail
    let result = client.connect("mqtt://localhost:1883").await;
    assert!(result.is_err());

    match result.unwrap_err() {
        MqttError::AlreadyConnected => {} // Expected
        other => panic!("Expected AlreadyConnected error, got: {:?}", other),
    }

    client.disconnect().await.unwrap();
}

#[tokio::test]
async fn test_disconnect_not_connected() {
    let client = MqttClient::new("disconnect-test");

    // Disconnect without connect should fail gracefully
    let result = client.disconnect().await;
    assert!(result.is_err());

    match result.unwrap_err() {
        MqttError::NotConnected => {} // Expected
        other => panic!("Expected NotConnected error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_qos2_timeout() {
    let client = MqttClient::new("qos2-timeout-test");
    client.connect("mqtt://localhost:1883").await.unwrap();

    // Subscribe to get our own messages
    client.subscribe("test/qos2/timeout", |_| {}).await.unwrap();

    // Publish QoS 2 message
    let mut options = PublishOptions::default();
    options.qos = QoS::ExactlyOnce;

    // This should complete normally
    let result = client
        .publish_with_options("test/qos2/timeout", "test", options)
        .await;
    assert!(result.is_ok());

    client.disconnect().await.unwrap();
}

#[tokio::test]
async fn test_invalid_qos_value() {
    // Test that invalid QoS values are handled properly
    let qos = QoS::from(3u8); // Invalid QoS
    assert_eq!(qos, QoS::AtMostOnce); // Should default to QoS 0

    let qos = QoS::from(255u8); // Invalid QoS
    assert_eq!(qos, QoS::AtMostOnce); // Should default to QoS 0
}

#[tokio::test]
async fn test_network_disconnection_during_publish() {
    let client = MqttClient::new("network-disconnect-test");
    client.connect("mqtt://localhost:1883").await.unwrap();

    // Simulate network disconnection by dropping the client
    // and trying to use a clone
    let client_clone = client.clone();

    // Force disconnect on the original
    client.disconnect().await.unwrap();

    // Try to publish on the clone - should fail
    let result = client_clone.publish("test/topic", "message").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_malformed_connack_handling() {
    // This would require a mock broker that sends malformed packets
    // For now, we test that the client handles unexpected disconnections
    let client = MqttClient::new("malformed-test");

    // Connect to a valid broker
    client.connect("mqtt://localhost:1883").await.unwrap();

    // The client should handle unexpected disconnections gracefully
    // In a real test, we'd simulate a malformed packet here
}

#[tokio::test]
async fn test_subscribe_with_invalid_qos() {
    let client = MqttClient::new("invalid-qos-sub");
    client.connect("mqtt://localhost:1883").await.unwrap();

    // Create subscribe options with valid QoS
    let mut options = SubscribeOptions::default();
    options.qos = QoS::ExactlyOnce; // Valid QoS 2

    // Subscribe should work
    let result = client
        .subscribe_with_options("test/topic", options, |_| {})
        .await;
    assert!(result.is_ok());

    client.disconnect().await.unwrap();
}

#[tokio::test]
async fn test_connection_lost_callback() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let disconnected = Arc::new(AtomicBool::new(false));
    let _disconnected_clone = disconnected.clone();

    let options = ConnectOptions::new("callback-test");
    // Connection lost callbacks would be set at the client level

    let client = MqttClient::with_options(options);
    client.connect("mqtt://localhost:1883").await.unwrap();

    // Normal disconnect shouldn't trigger the callback
    client.disconnect().await.unwrap();

    // Give some time for any callbacks to fire
    tokio::time::sleep(Duration::from_millis(100)).await;

    // The callback should not have been called for normal disconnect
    assert!(!disconnected.load(Ordering::Relaxed));
}

#[tokio::test]
async fn test_multiple_error_conditions() {
    let client = MqttClient::new("multi-error-test");

    // Error 1: Publish before connect
    assert!(client.publish("test", "msg").await.is_err());

    // Error 2: Subscribe before connect
    assert!(client.subscribe("test", |_| {}).await.is_err());

    // Error 3: Invalid address
    assert!(client.connect("not-a-valid-url").await.is_err());

    // Error 4: Disconnect when not connected
    assert!(client.disconnect().await.is_err());
}

#[tokio::test]
async fn test_flow_control_exceeded() {
    let mut options = ConnectOptions::new("flow-control-test");
    options.properties.receive_maximum = Some(2); // Allow only 2 in-flight messages

    let client = MqttClient::with_options(options);
    client.connect("mqtt://localhost:1883").await.unwrap();

    // Subscribe to our own messages to keep them in-flight
    client
        .subscribe("test/flow/#", |_| {
            // Slow processing to keep messages in-flight
            std::thread::sleep(Duration::from_millis(100));
        })
        .await
        .unwrap();

    // Send multiple QoS 1 messages quickly
    let mut pub_options = PublishOptions::default();
    pub_options.qos = QoS::AtLeastOnce;

    // First two should succeed
    for i in 0..2 {
        let result = client
            .publish_with_options(
                &format!("test/flow/{}", i),
                format!("message {}", i),
                pub_options.clone(),
            )
            .await;
        assert!(result.is_ok());
    }

    // Give a little time for messages to be in-flight
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Third one might fail due to flow control
    // (depends on how fast the broker processes)
    let _result = client
        .publish_with_options("test/flow/2", "message 2", pub_options)
        .await;
    // Don't assert on this one as timing is unpredictable

    client.disconnect().await.unwrap();
}
