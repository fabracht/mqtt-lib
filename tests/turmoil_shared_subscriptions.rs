//! Shared subscription tests using Turmoil
//!
//! These tests verify the shared subscription functionality using
//! the existing `MessageRouter` directly, which we know works.

#[cfg(feature = "turmoil-testing")]
use mqtt5::broker::router::MessageRouter;
#[cfg(feature = "turmoil-testing")]
use mqtt5::packet::publish::PublishPacket;
#[cfg(feature = "turmoil-testing")]
use mqtt5::QoS;
#[cfg(feature = "turmoil-testing")]
use std::sync::Arc;
#[cfg(feature = "turmoil-testing")]
use std::time::Duration;
#[cfg(feature = "turmoil-testing")]
use tokio::sync::mpsc;

#[cfg(feature = "turmoil-testing")]
#[test]
fn test_shared_subscriptions_in_turmoil() {
    let mut sim = turmoil::Builder::new()
        .simulation_duration(Duration::from_secs(10))
        .build();

    sim.host("test", || async {
        // This test uses the known-working MessageRouter directly
        let router = Arc::new(MessageRouter::new());

        // Create channels for three workers
        let (tx1, mut rx1) = mpsc::channel(100);
        let (tx2, mut rx2) = mpsc::channel(100);
        let (tx3, mut rx3) = mpsc::channel(100);

        // Register workers
        let (dtx1, _drx1) = tokio::sync::oneshot::channel();
        router
            .register_client("worker1".to_string(), tx1, dtx1)
            .await;
        let (dtx2, _drx2) = tokio::sync::oneshot::channel();
        router
            .register_client("worker2".to_string(), tx2, dtx2)
            .await;
        let (dtx3, _drx3) = tokio::sync::oneshot::channel();
        router
            .register_client("worker3".to_string(), tx3, dtx3)
            .await;

        // All workers subscribe to same shared subscription
        router
            .subscribe(
                "worker1".to_string(),
                "$share/workers/tasks/+".to_string(),
                QoS::AtMostOnce,
                None,
                false,
            )
            .await;

        router
            .subscribe(
                "worker2".to_string(),
                "$share/workers/tasks/+".to_string(),
                QoS::AtMostOnce,
                None,
                false,
            )
            .await;

        router
            .subscribe(
                "worker3".to_string(),
                "$share/workers/tasks/+".to_string(),
                QoS::AtMostOnce,
                None,
                false,
            )
            .await;

        // Publish 9 messages
        for i in 0..9 {
            let publish = PublishPacket::new(
                format!("tasks/job{}", i % 3),
                format!("Task {i}").as_bytes(),
                QoS::AtMostOnce,
            );
            router.route_message(&publish, None).await;
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

        // Verify distribution
        assert_eq!(count1 + count2 + count3, 9);
        assert!((2..=4).contains(&count1));
        assert!((2..=4).contains(&count2));
        assert!((2..=4).contains(&count3));

        Ok::<(), Box<dyn std::error::Error>>(())
    });

    sim.run().unwrap();
}
