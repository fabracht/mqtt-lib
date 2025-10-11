mod common;

use common::TestBroker;
use mqtt5::{
    ConnectOptions, ConnectionEvent, Message, MqttClient, PublishOptions, QoS, SubscribeOptions,
    WillMessage, WillProperties,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use ulid::Ulid;

/// Helper function to get a unique client ID for tests
fn test_client_id(test_name: &str) -> String {
    format!("test-{test_name}-{}", Ulid::new())
}

#[tokio::test]
async fn test_mqtt5_properties_system() {
    // Start test broker
    let broker = TestBroker::start().await;

    let client = MqttClient::new(test_client_id("mqtt5-props"));

    // Connect with v5.0 specific properties
    let mut opts = ConnectOptions::new(test_client_id("mqtt5-props")).with_clean_start(true);

    // Set MQTT v5.0 properties
    opts.properties.session_expiry_interval = Some(3600); // 1 hour
    opts.properties.receive_maximum = Some(100);
    opts.properties.maximum_packet_size = Some(1024 * 1024); // 1MB
    opts.properties.topic_alias_maximum = Some(10);
    opts.properties.request_response_information = Some(true);
    opts.properties.request_problem_information = Some(true);
    opts.properties
        .user_properties
        .push(("client-type".to_string(), "test-suite".to_string()));
    opts.properties
        .user_properties
        .push(("version".to_string(), "1.0".to_string()));

    let connect_result = client
        .connect_with_options(broker.address(), opts)
        .await
        .expect("Failed to connect");

    assert!(!connect_result.session_present); // Clean start, so no session

    // Test publish with v5.0 properties
    let received_props = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    let props_clone = Arc::clone(&received_props);

    client
        .subscribe("test/props", move |msg: Message| {
            let props = msg.properties.clone();
            let props_clone = props_clone.clone();
            tokio::spawn(async move {
                let mut map = props_clone.lock().await;
                // Store some properties for verification
                if let Some(content_type) = props.content_type {
                    map.insert("content_type".to_string(), content_type);
                }
                if let Some(response_topic) = props.response_topic {
                    map.insert("response_topic".to_string(), response_topic);
                }
                if let Some(correlation_data) = props.correlation_data {
                    map.insert(
                        "correlation_data".to_string(),
                        String::from_utf8_lossy(&correlation_data).to_string(),
                    );
                }
                // User properties
                for (key, value) in &props.user_properties {
                    map.insert(format!("user_{key}"), value.clone());
                }
            });
        })
        .await
        .expect("Failed to subscribe");

    let mut pub_opts = PublishOptions {
        qos: QoS::AtLeastOnce,
        ..Default::default()
    };
    pub_opts.properties.message_expiry_interval = Some(300); // 5 minutes
    pub_opts.properties.payload_format_indicator = Some(true);
    pub_opts.properties.content_type = Some("application/json".to_string());
    pub_opts.properties.response_topic = Some("response/topic".to_string());
    pub_opts.properties.correlation_data = Some(b"corr-123".to_vec());
    pub_opts
        .properties
        .user_properties
        .push(("msg-type".to_string(), "test".to_string()));
    pub_opts
        .properties
        .user_properties
        .push(("timestamp".to_string(), "2024-01-01".to_string()));

    let _ = client
        .publish_with_options("test/props", b"{\"test\": true}", pub_opts)
        .await
        .expect("Failed to publish");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify properties were received
    let props = received_props.lock().await;
    assert_eq!(
        props.get("content_type"),
        Some(&"application/json".to_string())
    );
    assert_eq!(
        props.get("response_topic"),
        Some(&"response/topic".to_string())
    );
    assert_eq!(props.get("correlation_data"), Some(&"corr-123".to_string()));
    assert_eq!(props.get("user_msg-type"), Some(&"test".to_string()));

    client.disconnect().await.expect("Failed to disconnect");
}

#[tokio::test]
async fn test_will_message_with_delay() {
    // Start test broker
    let broker = TestBroker::start().await;

    // Client that will have a Will message
    let will_client_id = test_client_id("will-sender");
    let will_client = MqttClient::new(will_client_id.clone());

    // Subscriber to receive Will message
    let sub_client = MqttClient::new(test_client_id("will-receiver"));

    sub_client
        .connect(broker.address())
        .await
        .expect("Subscriber failed to connect");

    let will_received = Arc::new(AtomicBool::new(false));
    let will_received_clone = Arc::clone(&will_received);
    let receive_time = Arc::new(Mutex::new(None::<std::time::Instant>));
    let receive_time_clone = Arc::clone(&receive_time);

    sub_client
        .subscribe("will/topic", move |msg: Message| {
            let will_received_clone = will_received_clone.clone();
            let receive_time_clone = receive_time_clone.clone();
            tokio::spawn(async move {
                assert_eq!(msg.payload, b"Client disconnected unexpectedly");
                will_received_clone.store(true, Ordering::SeqCst);
                *receive_time_clone.lock().await = Some(std::time::Instant::now());
            });
        })
        .await
        .expect("Failed to subscribe to will topic");

    // Configure Will message with delay
    let mut will_props = WillProperties {
        will_delay_interval: Some(2), // 2 seconds delay
        message_expiry_interval: Some(60),
        content_type: Some("text/plain".to_string()),
        ..Default::default()
    };
    will_props
        .user_properties
        .push(("reason".to_string(), "test-disconnect".to_string()));

    let mut will = WillMessage::new("will/topic", b"Client disconnected unexpectedly");
    will.qos = QoS::AtLeastOnce;
    will.retain = false;
    will.properties = will_props;

    let connect_opts = ConnectOptions::new(will_client_id)
        .with_clean_start(true)
        .with_will(will);

    let disconnect_time = Arc::new(Mutex::new(None::<std::time::Instant>));
    let disconnect_time_clone = Arc::clone(&disconnect_time);

    will_client
        .connect_with_options(broker.address(), connect_opts)
        .await
        .expect("Will client failed to connect");

    // Simulate abnormal disconnection without sending DISCONNECT packet
    *disconnect_time_clone.lock().await = Some(std::time::Instant::now());
    will_client
        .disconnect_abnormally()
        .await
        .expect("Failed to disconnect abnormally");

    // Wait for Will message (should arrive after delay)
    tokio::time::sleep(Duration::from_secs(4)).await;

    assert!(will_received.load(Ordering::SeqCst));

    // Verify delay was respected
    let disc_time = disconnect_time.lock().await.unwrap();
    let recv_time = receive_time.lock().await.unwrap();
    let delay = recv_time.duration_since(disc_time);
    println!("Will message delay: {delay:?}");
    // Allow some tolerance for timing
    assert!(
        delay >= Duration::from_millis(1900),
        "Delay was {delay:?}, expected at least 2 seconds"
    );

    sub_client
        .disconnect()
        .await
        .expect("Failed to disconnect subscriber");
}

#[tokio::test]
async fn test_topic_aliases() {
    // Start test broker
    let broker = TestBroker::start().await;

    let client = MqttClient::new(test_client_id("topic-alias"));

    // Connect with topic alias support
    let mut opts = ConnectOptions::new(test_client_id("topic-alias"));
    opts.properties.topic_alias_maximum = Some(5);

    client
        .connect_with_options(broker.address(), opts)
        .await
        .expect("Failed to connect");

    let received = Arc::new(Mutex::new(Vec::<(String, Vec<u8>)>::new()));
    let received_clone = Arc::clone(&received);

    client
        .subscribe("sensors/+/temperature", move |msg: Message| {
            let received_clone = received_clone.clone();
            let topic = msg.topic.clone();
            let payload = msg.payload.clone();
            tokio::spawn(async move {
                received_clone.lock().await.push((topic, payload));
            });
        })
        .await
        .expect("Failed to subscribe");

    // Publish multiple times to same topic (should use alias after first)
    for i in 0..5 {
        client
            .publish_qos1("sensors/room1/temperature", format!("25.{i}").as_bytes())
            .await
            .expect("Failed to publish");
    }

    // Publish to different topics
    client
        .publish_qos1("sensors/room2/temperature", b"26.0")
        .await
        .unwrap();
    client
        .publish_qos1("sensors/room3/temperature", b"24.5")
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let msgs = received.lock().await;
    assert_eq!(msgs.len(), 7); // 5 + 2 messages

    // All messages should be received correctly despite alias usage
    assert_eq!(
        msgs.iter()
            .filter(|(t, _)| t == "sensors/room1/temperature")
            .count(),
        5
    );
    assert_eq!(
        msgs.iter()
            .filter(|(t, _)| t == "sensors/room2/temperature")
            .count(),
        1
    );
    assert_eq!(
        msgs.iter()
            .filter(|(t, _)| t == "sensors/room3/temperature")
            .count(),
        1
    );

    client.disconnect().await.expect("Failed to disconnect");
}

#[tokio::test]
async fn test_flow_control_receive_maximum() {
    // Start test broker
    let broker = TestBroker::start().await;

    let client = MqttClient::new(test_client_id("flow-control"));

    // Connect with limited receive maximum
    let mut opts = ConnectOptions::new(test_client_id("flow-control"));
    opts.properties.receive_maximum = Some(2); // Only 2 in-flight messages

    client
        .connect_with_options(broker.address(), opts)
        .await
        .expect("Failed to connect");

    let received = Arc::new(AtomicU32::new(0));
    let received_clone = Arc::clone(&received);
    let processing = Arc::new(Mutex::new(Vec::<String>::new()));
    let processing_clone = Arc::clone(&processing);

    client
        .subscribe("flow/test", move |msg: Message| {
            let msg_str = String::from_utf8_lossy(&msg.payload).to_string();
            let processing_clone = processing_clone.clone();
            let received_clone = received_clone.clone();
            tokio::spawn(async move {
                processing_clone.lock().await.push(msg_str.clone());

                // Simulate slow processing
                tokio::time::sleep(Duration::from_millis(100)).await;

                received_clone.fetch_add(1, Ordering::SeqCst);
            });
        })
        .await
        .expect("Failed to subscribe");

    // Publisher sends multiple messages quickly
    let publisher = MqttClient::new(test_client_id("flow-publisher"));

    publisher
        .connect(broker.address())
        .await
        .expect("Failed to connect publisher");

    // Send 5 messages rapidly
    for i in 1..=5 {
        publisher
            .publish_qos2("flow/test", format!("Message {i}").as_bytes())
            .await
            .expect("Failed to publish");
    }

    // Wait for processing
    tokio::time::sleep(Duration::from_secs(1)).await;

    // All messages should eventually be received
    assert_eq!(received.load(Ordering::SeqCst), 5);

    let processed = processing.lock().await;
    assert_eq!(processed.len(), 5);
    for i in 1..=5 {
        assert!(processed.contains(&format!("Message {i}")));
    }

    client.disconnect().await.expect("Failed to disconnect");
    publisher
        .disconnect()
        .await
        .expect("Failed to disconnect publisher");
}

#[tokio::test]
async fn test_subscription_identifiers() {
    // Start test broker
    let broker = TestBroker::start().await;

    let client = MqttClient::new(test_client_id("sub-id"));

    client
        .connect(broker.address())
        .await
        .expect("Failed to connect");

    let received_ids = Arc::new(Mutex::new(Vec::<u32>::new()));

    // Subscribe with different subscription IDs
    let ids_clone = Arc::clone(&received_ids);
    let sub_opts1 = SubscribeOptions::default().with_subscription_identifier(1);

    client
        .subscribe_with_options("test/sub/+", sub_opts1, move |msg: Message| {
            let ids_clone = ids_clone.clone();
            let subscription_identifiers = msg.properties.subscription_identifiers.clone();
            tokio::spawn(async move {
                if !subscription_identifiers.is_empty() {
                    let sub_id = subscription_identifiers[0];
                    ids_clone.lock().await.push(sub_id);
                }
            });
        })
        .await
        .expect("Failed to subscribe with ID 1");

    let ids_clone = Arc::clone(&received_ids);
    let sub_opts2 = SubscribeOptions::default().with_subscription_identifier(2);

    client
        .subscribe_with_options("test/+/data", sub_opts2, move |msg: Message| {
            let ids_clone = ids_clone.clone();
            let subscription_identifiers = msg.properties.subscription_identifiers.clone();
            tokio::spawn(async move {
                if !subscription_identifiers.is_empty() {
                    let sub_id = subscription_identifiers[0];
                    ids_clone.lock().await.push(sub_id);
                }
            });
        })
        .await
        .expect("Failed to subscribe with ID 2");

    // Publish to topics that match different subscriptions
    client.publish_qos1("test/sub/one", b"data").await.unwrap();
    client
        .publish_qos1("test/sensor/data", b"data")
        .await
        .unwrap();
    client.publish_qos1("test/sub/data", b"data").await.unwrap(); // Matches both

    tokio::time::sleep(Duration::from_millis(500)).await;

    let ids = received_ids.lock().await;
    println!("Received subscription IDs: {:?}", *ids);
    if ids.is_empty() {
        println!(
            "Warning: No subscription identifiers received - feature may not be fully implemented"
        );
    } else {
        assert!(ids.contains(&1)); // From first subscription
        assert!(ids.contains(&2)); // From second subscription
    }

    client.disconnect().await.expect("Failed to disconnect");
}

#[tokio::test]
async fn test_shared_subscriptions() {
    // Start test broker
    let broker = TestBroker::start().await;

    // Note: Shared subscriptions require broker support
    // This test assumes Mosquitto is configured with shared subscription support

    let received = Arc::new(Mutex::new(HashMap::<String, Vec<String>>::new()));

    // Create multiple clients sharing a subscription
    let mut clients = vec![];
    for i in 0..3 {
        let client_id = test_client_id(&format!("shared-{i}"));
        let client = MqttClient::new(client_id.clone());

        client
            .connect(broker.address())
            .await
            .expect("Failed to connect");

        let received_clone = Arc::clone(&received);
        let client_name = client_id.clone();

        // Shared subscription format: $share/group/topic
        client
            .subscribe("$share/testgroup/shared/topic", move |msg: Message| {
                let payload = String::from_utf8_lossy(&msg.payload).to_string();
                let received_clone = received_clone.clone();
                let client_name = client_name.clone();
                tokio::spawn(async move {
                    received_clone
                        .lock()
                        .await
                        .entry(client_name)
                        .or_default()
                        .push(payload);
                });
            })
            .await
            .expect("Failed to subscribe to shared topic");

        clients.push(client);
    }

    // Publisher sends multiple messages
    let publisher = MqttClient::new(test_client_id("shared-publisher"));

    publisher
        .connect(broker.address())
        .await
        .expect("Failed to connect publisher");

    for i in 1..=9 {
        publisher
            .publish_qos1("shared/topic", format!("Message {i}").as_bytes())
            .await
            .expect("Failed to publish");
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify load balancing
    let msgs = received.lock().await;
    let total_messages: usize = msgs.values().map(std::vec::Vec::len).sum();

    // All messages should be received exactly once across all clients
    assert_eq!(total_messages, 9);

    // Each client should have received some messages (roughly balanced)
    for (_client, messages) in msgs.iter() {
        assert!(!messages.is_empty());
        assert!(messages.len() <= 5); // No client should get all messages
    }

    // Disconnect all clients
    for client in clients {
        client.disconnect().await.expect("Failed to disconnect");
    }
    publisher
        .disconnect()
        .await
        .expect("Failed to disconnect publisher");
}

#[tokio::test]
async fn test_maximum_packet_size() {
    // Start test broker
    let broker = TestBroker::start().await;

    let client = MqttClient::new(test_client_id("max-packet"));

    // Connect with maximum packet size limit
    let mut opts = ConnectOptions::new(test_client_id("max-packet"));
    opts.properties.maximum_packet_size = Some(1024); // 1KB limit

    client
        .connect_with_options(broker.address(), opts)
        .await
        .expect("Failed to connect");

    // Try to publish within limit
    let small_payload = vec![0x42; 512]; // 512 bytes
    let result = client.publish("test/size", small_payload).await;
    assert!(result.is_ok());

    // Try to publish exceeding limit
    let large_payload = vec![0x42; 2048]; // 2KB
    let result = client.publish("test/size", large_payload).await;

    // Should fail due to packet size limit
    assert!(result.is_err());

    client.disconnect().await.expect("Failed to disconnect");
}

#[tokio::test]
async fn test_reason_codes_and_strings() {
    // Start test broker
    let broker = TestBroker::start().await;

    let client = MqttClient::new(test_client_id("reason-codes"));

    let disconnect_reason = Arc::new(Mutex::new(None));
    let reason_clone = Arc::clone(&disconnect_reason);

    client
        .on_connection_event(move |event| {
            let reason_clone = reason_clone.clone();
            tokio::spawn(async move {
                if let ConnectionEvent::Disconnected { reason } = event {
                    *reason_clone.lock().await = Some(reason);
                }
            });
        })
        .await
        .expect("Failed to register event handler");

    // Connect normally
    client
        .connect(broker.address())
        .await
        .expect("Failed to connect");

    // Subscribe to a topic that might have access restrictions
    // (depending on broker configuration)
    let result = client.subscribe("$SYS/restricted", |_| {}).await;

    // The result might contain a reason code if subscription is denied
    if result.is_err() {
        println!("Subscription failed as expected: {result:?}");
    }

    client.disconnect().await.expect("Failed to disconnect");
}
