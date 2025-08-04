//! Basic test for shared subscriptions

use mqtt5::broker::router::MessageRouter;
use mqtt5::packet::publish::PublishPacket;
use mqtt5::QoS;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_shared_subscription_distribution() {
    // Create router
    let router = Arc::new(MessageRouter::new());

    // Create channels for three workers
    let (tx1, mut rx1) = mpsc::channel(100);
    let (tx2, mut rx2) = mpsc::channel(100);
    let (tx3, mut rx3) = mpsc::channel(100);

    // Register workers
    router.register_client("worker1".to_string(), tx1).await;
    router.register_client("worker2".to_string(), tx2).await;
    router.register_client("worker3".to_string(), tx3).await;

    // All workers subscribe to same shared subscription
    router
        .subscribe(
            "worker1".to_string(),
            "$share/workers/tasks/+".to_string(),
            QoS::AtMostOnce,
            None,
        )
        .await;

    router
        .subscribe(
            "worker2".to_string(),
            "$share/workers/tasks/+".to_string(),
            QoS::AtMostOnce,
            None,
        )
        .await;

    router
        .subscribe(
            "worker3".to_string(),
            "$share/workers/tasks/+".to_string(),
            QoS::AtMostOnce,
            None,
        )
        .await;

    // Publish 9 messages
    for i in 0..9 {
        let publish = PublishPacket::new(
            format!("tasks/job{}", i % 3),
            format!("Task {}", i).as_bytes(),
            QoS::AtMostOnce,
        );
        router.route_message(&publish).await;
    }

    // Count messages received by each worker
    let mut count1 = 0;
    let mut count2 = 0;
    let mut count3 = 0;

    // Drain all channels
    while rx1.try_recv().is_ok() {
        count1 += 1;
    }
    while rx2.try_recv().is_ok() {
        count2 += 1;
    }
    while rx3.try_recv().is_ok() {
        count3 += 1;
    }

    println!("Worker 1 received: {} messages", count1);
    println!("Worker 2 received: {} messages", count2);
    println!("Worker 3 received: {} messages", count3);
    println!("Total: {} messages", count1 + count2 + count3);

    // Each worker should get exactly 3 messages (round-robin)
    assert_eq!(count1, 3);
    assert_eq!(count2, 3);
    assert_eq!(count3, 3);
    assert_eq!(count1 + count2 + count3, 9);
}

#[tokio::test]
async fn test_mixed_shared_and_regular_subscriptions() {
    let router = Arc::new(MessageRouter::new());

    // Create channels
    let (tx_shared1, mut rx_shared1) = mpsc::channel(100);
    let (tx_shared2, mut rx_shared2) = mpsc::channel(100);
    let (tx_regular, mut rx_regular) = mpsc::channel(100);

    // Register clients
    router
        .register_client("shared1".to_string(), tx_shared1)
        .await;
    router
        .register_client("shared2".to_string(), tx_shared2)
        .await;
    router
        .register_client("regular".to_string(), tx_regular)
        .await;

    // Shared subscriptions
    router
        .subscribe(
            "shared1".to_string(),
            "$share/team/alerts/+".to_string(),
            QoS::AtMostOnce,
            None,
        )
        .await;

    router
        .subscribe(
            "shared2".to_string(),
            "$share/team/alerts/+".to_string(),
            QoS::AtMostOnce,
            None,
        )
        .await;

    // Regular subscription
    router
        .subscribe(
            "regular".to_string(),
            "alerts/+".to_string(),
            QoS::AtMostOnce,
            None,
        )
        .await;

    // Publish 4 messages
    for i in 0..4 {
        let publish = PublishPacket::new(
            format!("alerts/critical{}", i),
            format!("Alert {}", i).as_bytes(),
            QoS::AtMostOnce,
        );
        router.route_message(&publish).await;
    }

    // Count messages
    let mut shared1_count = 0;
    let mut shared2_count = 0;
    let mut regular_count = 0;

    while rx_shared1.try_recv().is_ok() {
        shared1_count += 1;
    }
    while rx_shared2.try_recv().is_ok() {
        shared2_count += 1;
    }
    while rx_regular.try_recv().is_ok() {
        regular_count += 1;
    }

    println!("\nMixed subscription test:");
    println!("Shared1 received: {} messages", shared1_count);
    println!("Shared2 received: {} messages", shared2_count);
    println!("Regular received: {} messages", regular_count);

    // Regular subscriber should get all 4 messages
    assert_eq!(regular_count, 4);

    // Shared subscribers should split the messages
    assert_eq!(shared1_count + shared2_count, 4);
    assert_eq!(shared1_count, 2);
    assert_eq!(shared2_count, 2);
}
