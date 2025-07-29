mod common;

use common::{
    cleanup_topic, create_test_client, test_basic_pubsub, test_qos_flow, test_retained_messages,
};
use mqtt_v5::QoS;

#[tokio::test]
async fn test_using_common_scenarios() {
    // Create a test client
    let client = create_test_client("scenario-demo").await;

    // Clean up any existing retained messages
    cleanup_topic(&client, "demo/retained").await.unwrap();

    // Test basic pub/sub scenario
    test_basic_pubsub(&client, "demo/basic")
        .await
        .expect("Basic pubsub test failed");

    // Test QoS flows
    test_qos_flow(&client, "demo/qos0", QoS::AtMostOnce)
        .await
        .expect("QoS 0 test failed");

    test_qos_flow(&client, "demo/qos1", QoS::AtLeastOnce)
        .await
        .expect("QoS 1 test failed");

    // Test retained messages
    test_retained_messages(&client, "demo/retained")
        .await
        .expect("Retained message test failed");

    // Cleanup
    client.disconnect().await.expect("Failed to disconnect");
}

#[tokio::test]
async fn test_message_collector_advanced() {
    use common::{wait_with_message, MessageCollector};
    use std::time::Duration;

    let client = create_test_client("collector-demo").await;
    let collector = MessageCollector::new();

    // Subscribe with collector
    client
        .subscribe("test/collector/#", collector.callback())
        .await
        .expect("Failed to subscribe");

    // Publish multiple messages
    for i in 0..5 {
        client
            .publish(
                &format!("test/collector/{i}"),
                format!("Message {i}").as_bytes(),
            )
            .await
            .expect("Failed to publish");
    }

    // Wait for messages
    wait_with_message(Duration::from_millis(200), "Waiting for messages...").await;

    // Verify
    let messages = collector.get_messages().await;
    assert_eq!(messages.len(), 5);

    // Check message contents
    for (i, msg) in messages.iter().enumerate() {
        assert_eq!(msg.topic, format!("test/collector/{i}"));
        assert_eq!(
            String::from_utf8_lossy(&msg.payload),
            format!("Message {i}")
        );
    }

    client.disconnect().await.unwrap();
}

#[tokio::test]
async fn test_event_counter_with_wildcards() {
    use common::EventCounter;
    use std::time::Duration;

    let client = create_test_client("counter-demo").await;
    let counter = EventCounter::new();

    // Subscribe to wildcard
    client
        .subscribe("events/+/data", counter.callback())
        .await
        .expect("Failed to subscribe");

    // Publish to different topics
    client
        .publish("events/sensor1/data", b"data1")
        .await
        .unwrap();
    client
        .publish("events/sensor2/data", b"data2")
        .await
        .unwrap();
    client
        .publish("events/sensor3/data", b"data3")
        .await
        .unwrap();
    client
        .publish("events/other/info", b"should not count")
        .await
        .unwrap(); // Won't match

    // Wait for messages
    assert!(
        counter.wait_for(3, Duration::from_secs(1)).await,
        "Failed to receive expected messages"
    );
    assert_eq!(counter.get(), 3);

    client.disconnect().await.unwrap();
}
