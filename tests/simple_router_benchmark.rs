//! Simple router benchmark to demonstrate optimization improvements

use mqtt_v5::broker::optimized_router::OptimizedMessageRouter;
use mqtt_v5::broker::router::MessageRouter;
use mqtt_v5::packet::publish::PublishPacket;
use mqtt_v5::QoS;
use std::time::Instant;
use tokio::sync::mpsc;

#[tokio::test]
async fn benchmark_router_performance() {
    println!("=== Router Performance Benchmark ===");

    const NUM_CLIENTS: usize = 50;
    const NUM_SUBSCRIPTIONS: usize = 10;
    const NUM_MESSAGES: usize = 1000;

    println!("Configuration:");
    println!("  Clients: {}", NUM_CLIENTS);
    println!("  Subscriptions per client: {}", NUM_SUBSCRIPTIONS);
    println!("  Messages to route: {}", NUM_MESSAGES);

    // Test original router
    println!("\n--- Original Router ---");
    let original_router = MessageRouter::new();

    // Setup clients and subscriptions
    let setup_start = Instant::now();
    for client_id in 0..NUM_CLIENTS {
        let (sender, _) = mpsc::channel(100);
        original_router
            .register_client(format!("client_{}", client_id), sender)
            .await;

        for sub_id in 0..NUM_SUBSCRIPTIONS {
            let topic = format!("sensors/room_{}/data/{}", client_id, sub_id);
            original_router
                .subscribe(
                    format!("client_{}", client_id),
                    topic,
                    QoS::AtMostOnce,
                    None,
                )
                .await;
        }
    }
    let original_setup_time = setup_start.elapsed();

    // Generate messages
    let mut messages = Vec::new();
    for i in 0..NUM_MESSAGES {
        let topic = format!(
            "sensors/room_{}/data/{}",
            i % NUM_CLIENTS,
            i % NUM_SUBSCRIPTIONS
        );
        messages.push(PublishPacket::new(topic, b"test payload", QoS::AtMostOnce));
    }

    // Route messages
    let routing_start = Instant::now();
    for message in &messages {
        original_router.route_message(message).await;
    }
    let original_routing_time = routing_start.elapsed();

    println!("Setup time: {:?}", original_setup_time);
    println!("Routing time: {:?}", original_routing_time);
    println!(
        "Messages/sec: {:.2}",
        NUM_MESSAGES as f64 / original_routing_time.as_secs_f64()
    );

    // Test optimized router
    println!("\n--- Optimized Router ---");
    let optimized_router = OptimizedMessageRouter::new();

    // Setup clients and subscriptions
    let setup_start = Instant::now();
    for client_id in 0..NUM_CLIENTS {
        let (sender, _) = mpsc::channel(100);
        optimized_router
            .register_client(format!("client_{}", client_id), sender)
            .await;

        for sub_id in 0..NUM_SUBSCRIPTIONS {
            let topic = format!("sensors/room_{}/data/{}", client_id, sub_id);
            optimized_router
                .subscribe(
                    format!("client_{}", client_id),
                    topic,
                    QoS::AtMostOnce,
                    None,
                )
                .await;
        }
    }
    let optimized_setup_time = setup_start.elapsed();

    // Route messages
    let routing_start = Instant::now();
    for message in &messages {
        optimized_router.route_message_optimized(message).await;
    }
    let optimized_routing_time = routing_start.elapsed();

    let (total_messages, subscriptions_matched, avg_time, lookups) = optimized_router.get_metrics();

    println!("Setup time: {:?}", optimized_setup_time);
    println!("Routing time: {:?}", optimized_routing_time);
    println!(
        "Messages/sec: {:.2}",
        NUM_MESSAGES as f64 / optimized_routing_time.as_secs_f64()
    );
    println!("Total messages processed: {}", total_messages);
    println!("Subscriptions matched: {}", subscriptions_matched);
    println!("Average delivery time: {:.2} μs", avg_time);
    println!("Subscription lookups: {}", lookups);

    // Performance comparison
    println!("\n--- Performance Improvement ---");
    let speed_improvement = (NUM_MESSAGES as f64 / optimized_routing_time.as_secs_f64())
        / (NUM_MESSAGES as f64 / original_routing_time.as_secs_f64());
    let setup_improvement = original_setup_time.as_secs_f64() / optimized_setup_time.as_secs_f64();

    println!("Routing speed improvement: {:.2}x", speed_improvement);
    println!("Setup speed improvement: {:.2}x", setup_improvement);

    // Verify performance gains
    assert!(speed_improvement > 1.0, "Optimized router should be faster");
    println!("\n✅ Performance optimization successful!");
}
