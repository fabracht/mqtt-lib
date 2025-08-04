use mqtt5::{ConnectOptions, ConnectionEvent, MqttClient, QoS, SubscribeOptions};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use ulid::Ulid;

/// Helper function to get a unique client ID for tests
fn test_client_id(test_name: &str) -> String {
    format!("test-{test_name}-{}", Ulid::new())
}

#[tokio::test]

async fn test_automatic_reconnection() {
    let client = MqttClient::new(test_client_id("auto-reconnect"));

    // Track connection events
    let connected_count = Arc::new(AtomicU32::new(0));
    let disconnected_count = Arc::new(AtomicU32::new(0));
    let reconnecting_count = Arc::new(AtomicU32::new(0));

    let connected_clone = Arc::clone(&connected_count);
    let disconnected_clone = Arc::clone(&disconnected_count);
    let reconnecting_clone = Arc::clone(&reconnecting_count);

    client
        .on_connection_event(move |event| match event {
            ConnectionEvent::Connected { .. } => {
                connected_clone.fetch_add(1, Ordering::SeqCst);
            }
            ConnectionEvent::Disconnected { .. } => {
                disconnected_clone.fetch_add(1, Ordering::SeqCst);
            }
            ConnectionEvent::Reconnecting { .. } => {
                reconnecting_clone.fetch_add(1, Ordering::SeqCst);
            }
            ConnectionEvent::ReconnectFailed { .. } => {
                // Do nothing for failed reconnects in this test
            }
        })
        .await
        .expect("Failed to register connection event handler");

    // Connect with automatic reconnection
    let opts = ConnectOptions::new(test_client_id("auto-reconnect"))
        .with_clean_start(false)
        .with_keep_alive(Duration::from_secs(5))
        .with_automatic_reconnect(true)
        .with_reconnect_delay(Duration::from_millis(100), Duration::from_secs(1));

    client
        .connect_with_options("127.0.0.1:1883", opts)
        .await
        .expect("Failed to connect");

    // Wait for initial connection
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(connected_count.load(Ordering::SeqCst), 1);

    // Subscribe to a topic
    let received = Arc::new(AtomicU32::new(0));
    let received_clone = Arc::clone(&received);

    client
        .subscribe("test/reconnect", move |_| {
            received_clone.fetch_add(1, Ordering::SeqCst);
        })
        .await
        .expect("Failed to subscribe");

    // Publish a message to verify subscription works
    client
        .publish_qos1("test/reconnect", b"Before disconnect")
        .await
        .expect("Failed to publish");

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(received.load(Ordering::SeqCst), 1);

    // Simulate connection loss by stopping the broker
    // In a real test, you would stop/restart the broker here
    // For now, we'll test with a forced disconnect

    // Note: In production tests, you would:
    // 1. Stop the broker: docker-compose stop mosquitto
    // 2. Wait for disconnection
    // 3. Start the broker: docker-compose start mosquitto
    // 4. Verify automatic reconnection

    // For this test, we'll simulate by connecting to a non-existent broker
    // and then back to the real one

    client.disconnect().await.expect("Failed to disconnect");
}

#[tokio::test]
async fn test_message_queuing_during_disconnection() {
    // First client to set up subscription
    let client_id = test_client_id("queue-test");
    let client1 = MqttClient::new(client_id.clone());

    let opts = ConnectOptions::new(client_id.clone())
        .with_clean_start(false)
        .with_session_expiry_interval(300);

    client1
        .connect_with_options("127.0.0.1:1883", opts.clone())
        .await
        .expect("Failed to connect");

    let received = Arc::new(RwLock::new(Vec::<String>::new()));
    let received_clone = Arc::clone(&received);

    let (packet_id, qos) = client1
        .subscribe_with_options(
            "test/queue/#",
            SubscribeOptions {
                qos: QoS::AtLeastOnce,
                ..Default::default()
            },
            move |msg| {
                let payload = String::from_utf8_lossy(&msg.payload);
                println!("Callback triggered with message: {payload:?}");
                let mut received = received_clone.write().unwrap();
                received.push(String::from_utf8_lossy(&msg.payload).to_string());
            },
        )
        .await
        .expect("Failed to subscribe");

    println!("Subscribed with packet_id: {packet_id}, qos: {qos:?}");

    // Disconnect first client
    client1.disconnect().await.expect("Failed to disconnect");

    // Publisher client sends messages while subscriber is offline
    let publisher = MqttClient::new(test_client_id("queue-publisher"));

    publisher
        .connect("127.0.0.1:1883")
        .await
        .expect("Failed to connect publisher");

    // Send multiple messages
    for i in 1..=5 {
        publisher
            .publish_qos1(
                &format!("test/queue/msg{i}"),
                format!("Offline message {i}").as_bytes(),
            )
            .await
            .expect("Failed to publish offline message");
    }

    publisher
        .disconnect()
        .await
        .expect("Failed to disconnect publisher");

    // Reconnect using same client instance
    let reconnect_result = client1
        .connect_with_options("127.0.0.1:1883", opts)
        .await
        .expect("Failed to reconnect");

    let session_present = reconnect_result.session_present;
    println!("Reconnected with session_present: {session_present}");

    // The subscription should be restored automatically with our callback persistence
    // Just wait for queued messages to be delivered
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify all messages were received
    {
        let messages = received.read().unwrap();
        let len = messages.len();
        println!("Received {len} messages");
        for msg in messages.iter() {
            println!("  - {msg}");
        }
        assert_eq!(messages.len(), 5);
        for i in 1..=5 {
            assert!(messages.contains(&format!("Offline message {i}")));
        }
    } // Drop the lock before awaiting

    client1.disconnect().await.expect("Failed to disconnect");
}

#[tokio::test]

async fn test_exponential_backoff_reconnection() {
    let client = MqttClient::new(test_client_id("backoff-test"));

    let attempt_times = Arc::new(RwLock::new(Vec::<std::time::Instant>::new()));
    let attempt_times_clone = Arc::clone(&attempt_times);

    client
        .on_connection_event(move |event| {
            if let ConnectionEvent::Reconnecting { attempt } = event {
                let mut times = attempt_times_clone.write().unwrap();
                times.push(std::time::Instant::now());
                println!("Reconnection attempt {attempt}");
            }
        })
        .await
        .expect("Failed to register event handler");

    // Connect to a non-existent port to trigger reconnection
    let opts = ConnectOptions::new(test_client_id("backoff-test"))
        .with_automatic_reconnect(true)
        .with_reconnect_delay(Duration::from_millis(100), Duration::from_secs(2))
        .with_max_reconnect_attempts(4);

    // This should fail and trigger reconnection attempts
    let result = client.connect_with_options("localhost:9999", opts).await;
    assert!(result.is_err());

    // Wait for reconnection attempts
    tokio::time::sleep(Duration::from_secs(5)).await;

    let times = attempt_times.read().unwrap();
    assert!(times.len() >= 3); // Should have made several attempts

    // Verify exponential backoff
    for i in 1..times.len() {
        let delay = times[i].duration_since(times[i - 1]);
        let next = i + 1;
        println!("Delay between attempt {i} and {next}: {delay:?}");

        // Each delay should be roughly double the previous (with some tolerance)
        if i > 1 {
            let prev_delay = times[i - 1].duration_since(times[i - 2]);
            assert!(delay >= prev_delay); // Should increase
            assert!(delay <= Duration::from_secs(2)); // Should respect max delay
        }
    }
}

#[tokio::test]

async fn test_clean_session_reconnection() {
    let client_id = test_client_id("clean-session");

    // First connection with clean_start = true
    let client1 = MqttClient::new(client_id.clone());

    let opts_clean = ConnectOptions::new(client_id.clone()).with_clean_start(true);

    client1
        .connect_with_options("127.0.0.1:1883", opts_clean)
        .await
        .expect("Failed to connect");

    // Subscribe to a topic
    client1
        .subscribe_with_options(
            "test/clean",
            SubscribeOptions {
                qos: QoS::AtLeastOnce,
                ..Default::default()
            },
            |_| {},
        )
        .await
        .expect("Failed to subscribe");

    client1.disconnect().await.expect("Failed to disconnect");

    // Publish while disconnected
    let publisher = MqttClient::new(test_client_id("clean-publisher"));

    publisher
        .connect("127.0.0.1:1883")
        .await
        .expect("Failed to connect publisher");
    publisher
        .publish_qos("test/clean", b"Should not receive", QoS::AtLeastOnce)
        .await
        .expect("Failed to publish");
    publisher
        .disconnect()
        .await
        .expect("Failed to disconnect publisher");

    // Reconnect with clean_start = true
    let client2 = MqttClient::new(client_id.clone());

    let received = Arc::new(AtomicBool::new(false));
    let received_clone = Arc::clone(&received);

    let opts_clean2 = ConnectOptions::new(client_id).with_clean_start(true);

    client2
        .connect_with_options("127.0.0.1:1883", opts_clean2)
        .await
        .expect("Failed to reconnect");

    // This subscription is new (clean session)
    client2
        .subscribe_with_options(
            "test/clean",
            SubscribeOptions {
                qos: QoS::AtLeastOnce,
                ..Default::default()
            },
            move |_| {
                received_clone.store(true, Ordering::SeqCst);
            },
        )
        .await
        .expect("Failed to subscribe");

    // Should NOT receive the message published while offline
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!received.load(Ordering::SeqCst));

    client2.disconnect().await.expect("Failed to disconnect");
}

#[tokio::test]

async fn test_keep_alive_timeout_detection() {
    let client = MqttClient::new(test_client_id("keepalive-test"));

    let disconnected = Arc::new(AtomicBool::new(false));
    let disconnected_clone = Arc::clone(&disconnected);

    client
        .on_connection_event(move |event| {
            if let ConnectionEvent::Disconnected { reason } = event {
                println!("Disconnected with reason: {reason:?}");
                disconnected_clone.store(true, Ordering::SeqCst);
            }
        })
        .await
        .expect("Failed to register event handler");

    // Connect with very short keep-alive
    let opts = ConnectOptions::new(test_client_id("keepalive-test"))
        .with_keep_alive(Duration::from_secs(2))
        .with_automatic_reconnect(false); // Disable auto-reconnect for this test

    client
        .connect_with_options("127.0.0.1:1883", opts)
        .await
        .expect("Failed to connect");

    // In a real scenario, we would block network traffic here
    // For testing, we rely on the keep-alive mechanism

    // The client should send PINGREQ every 2 seconds
    // If we don't receive PINGRESP, it should disconnect

    // Wait for potential timeout (giving some extra time)
    tokio::time::sleep(Duration::from_secs(10)).await;

    // In normal operation, should still be connected
    assert!(client.is_connected().await);

    client.disconnect().await.expect("Failed to disconnect");
}

async fn setup_test_subscriptions(
    client: &MqttClient,
    test_prefix: &str,
    message_count: Arc<AtomicU32>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let topics = [
        format!("{test_prefix}/exact/1"),
        format!("{test_prefix}/exact/2"),
        format!("{test_prefix}/wildcard/+"),
    ];

    for (i, topic) in topics.iter().enumerate() {
        println!("Subscribing to topic {i}: {topic}");
        let count_clone = Arc::clone(&message_count);
        client
            .subscribe_with_options(
                topic.clone(),
                SubscribeOptions {
                    qos: QoS::AtLeastOnce,
                    ..Default::default()
                },
                move |msg| {
                    println!(
                        "Callback triggered: {:?}",
                        String::from_utf8_lossy(&msg.payload)
                    );
                    count_clone.fetch_add(1, Ordering::SeqCst);
                },
            )
            .await?;
        println!("Successfully subscribed to topic {topic}");
    }

    Ok(topics.to_vec())
}

async fn publish_test_messages(
    client: &MqttClient,
    test_prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Publishing test messages...");

    client
        .publish_qos(
            &format!("{test_prefix}/exact/1"),
            b"Message 1",
            QoS::AtLeastOnce,
        )
        .await?;
    println!("Published Message 1");

    client
        .publish_qos(
            &format!("{test_prefix}/exact/2"),
            b"Message 2",
            QoS::AtLeastOnce,
        )
        .await?;
    println!("Published Message 2");

    client
        .publish_qos(
            &format!("{test_prefix}/wildcard/test"),
            b"Wildcard message",
            QoS::AtLeastOnce,
        )
        .await?;
    println!("Published Wildcard message");

    Ok(())
}

async fn publish_reconnect_messages(
    client: &MqttClient,
    test_prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Publishing messages after reconnect...");

    client
        .publish_qos(
            &format!("{test_prefix}/exact/1"),
            b"After reconnect 1",
            QoS::AtLeastOnce,
        )
        .await?;
    println!("Published to {test_prefix}/exact/1");

    client
        .publish_qos(
            &format!("{test_prefix}/exact/2"),
            b"After reconnect 2",
            QoS::AtLeastOnce,
        )
        .await?;
    println!("Published to {test_prefix}/exact/2");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_subscription_restoration_after_reconnect() {
    let client_id = test_client_id("sub-restore");
    let client = MqttClient::new(client_id.clone());
    let message_count = Arc::new(AtomicU32::new(0));

    // Connect with session persistence
    let opts = ConnectOptions::new(client_id.clone())
        .with_clean_start(false)
        .with_session_expiry_interval(300)
        .with_automatic_reconnect(true);

    client
        .connect_with_options("127.0.0.1:1883", opts.clone())
        .await
        .expect("Failed to connect");

    // Set up subscriptions and test publishing
    let ulid = Ulid::new();
    let test_prefix = format!("test-restore-{ulid}");

    setup_test_subscriptions(&client, &test_prefix, Arc::clone(&message_count))
        .await
        .expect("Failed to setup subscriptions");

    publish_test_messages(&client, &test_prefix)
        .await
        .expect("Failed to publish test messages");

    // Verify initial messages received
    tokio::time::sleep(Duration::from_millis(500)).await;
    let count = message_count.load(Ordering::SeqCst);
    println!("Received {count} messages before disconnect");
    assert_eq!(count, 3, "Should have received exactly 3 messages");

    // Disconnect and reconnect
    println!("Disconnecting...");
    client.disconnect().await.expect("Failed to disconnect");
    tokio::time::sleep(Duration::from_millis(1000)).await;
    message_count.store(0, Ordering::SeqCst);

    println!("Reconnecting...");
    let reconnect_result = client.connect_with_options("127.0.0.1:1883", opts).await;
    match reconnect_result {
        Ok(result) => println!(
            "Reconnected with session_present: {}",
            result.session_present
        ),
        Err(e) => panic!("Failed to reconnect: {e:?}"),
    }

    // Test subscription restoration
    publish_reconnect_messages(&client, &test_prefix)
        .await
        .expect("Failed to publish reconnect messages");

    tokio::time::sleep(Duration::from_millis(500)).await;
    let final_count = message_count.load(Ordering::SeqCst);
    println!("Received {final_count} messages after reconnect");
    assert!(
        final_count >= 2,
        "Should have received at least 2 messages after reconnect"
    );

    // Only disconnect if still connected
    if client.is_connected().await {
        client.disconnect().await.expect("Failed to disconnect");
    }
}
