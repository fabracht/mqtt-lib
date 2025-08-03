//! Broker performance tests
//!  
//! These are not traditional benchmarks but performance tests that can measure
//! broker performance characteristics and identify bottlenecks.

use mqtt_v5::broker::{BrokerConfig, MqttBroker};
use mqtt_v5::client::MqttClient;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[tokio::test]
async fn test_broker_startup_performance() {
    let start = Instant::now();

    let config = BrokerConfig::default()
        .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
        .with_max_clients(1000);

    let _broker = MqttBroker::with_config(config).await.unwrap();
    let startup_time = start.elapsed();

    println!("Broker startup time: {:?}", startup_time);

    // Startup should be under 100ms for reasonable performance
    assert!(
        startup_time < Duration::from_millis(100),
        "Broker startup took {:?}, expected < 100ms",
        startup_time
    );
}

#[tokio::test]
async fn test_connection_establishment_performance() {
    let config = BrokerConfig::default()
        .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
        .with_max_clients(100);

    let mut broker = MqttBroker::with_config(config).await.unwrap();
    let broker_addr = broker.local_addr().unwrap();

    let broker_handle = tokio::spawn(async move {
        let _ = broker.run().await;
    });

    sleep(Duration::from_millis(100)).await;

    // Test sequential connections
    let start = Instant::now();
    let mut clients = Vec::new();

    for i in 0..10 {
        let client = MqttClient::new(format!("perf-client-{}", i));
        client.connect(&broker_addr.to_string()).await.unwrap();
        clients.push(client);
    }

    let connection_time = start.elapsed();
    let avg_connection_time = connection_time / 10;

    println!("10 sequential connections took: {:?}", connection_time);
    println!("Average connection time: {:?}", avg_connection_time);

    // Average connection should be under 50ms
    assert!(
        avg_connection_time < Duration::from_millis(50),
        "Average connection time {:?} > 50ms",
        avg_connection_time
    );

    // Test concurrent connections
    let start = Instant::now();
    let concurrent_tasks: Vec<_> = (0..10)
        .map(|i| {
            let addr = broker_addr.to_string();
            tokio::spawn(async move {
                let client = MqttClient::new(format!("concurrent-client-{}", i));
                client.connect(&addr).await
            })
        })
        .collect();

    let results: Vec<_> = futures::future::join_all(concurrent_tasks)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    let concurrent_time = start.elapsed();

    println!("10 concurrent connections took: {:?}", concurrent_time);

    let success_count = results.iter().filter(|r| r.is_ok()).count();
    println!("Successful concurrent connections: {}/10", success_count);

    // Concurrent connections should be faster than sequential
    assert!(
        concurrent_time < connection_time,
        "Concurrent connections ({:?}) should be faster than sequential ({:?})",
        concurrent_time,
        connection_time
    );

    // Clean up
    for client in clients {
        let _ = client.disconnect().await;
    }

    broker_handle.abort();
}

#[tokio::test]
async fn test_message_throughput_performance() {
    let config = BrokerConfig::default()
        .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
        .with_max_clients(50);

    let mut broker = MqttBroker::with_config(config).await.unwrap();
    let resource_monitor = broker.resource_monitor();
    let broker_addr = broker.local_addr().unwrap();

    let broker_handle = tokio::spawn(async move {
        let _ = broker.run().await;
    });

    sleep(Duration::from_millis(100)).await;

    // Set up publisher and subscriber
    let publisher = MqttClient::new("throughput-publisher");
    let subscriber = MqttClient::new("throughput-subscriber");

    let addr_str = broker_addr.to_string();
    publisher.connect(&addr_str).await.unwrap();
    subscriber.connect(&addr_str).await.unwrap();

    subscriber.subscribe("perf/test", |_msg| {}).await.unwrap();
    sleep(Duration::from_millis(50)).await; // Let subscription settle

    // Test message publishing throughput
    let message_count = 100;
    let payload = "x".repeat(1000); // 1KB payload

    let start = Instant::now();

    for i in 0..message_count {
        publisher
            .publish(&format!("perf/test/{}", i), payload.as_bytes())
            .await
            .unwrap();
    }

    let publish_time = start.elapsed();
    let messages_per_second = (message_count as f64) / publish_time.as_secs_f64();
    let throughput_mbps =
        (message_count * payload.len()) as f64 / publish_time.as_secs_f64() / 1_000_000.0;

    println!("Published {} messages in {:?}", message_count, publish_time);
    println!("Throughput: {:.2} messages/second", messages_per_second);
    println!("Throughput: {:.2} MB/s", throughput_mbps);

    // Check resource monitor stats
    let stats = resource_monitor.get_stats().await;
    println!(
        "Resource stats: {} messages, {} bytes total",
        stats.total_messages, stats.total_bytes
    );

    // Should handle at least 500 messages/second
    assert!(
        messages_per_second > 500.0,
        "Throughput too low: {:.2} msg/s < 500 msg/s",
        messages_per_second
    );

    // Resource monitor should track most messages (allow for some timing variance)
    assert!(
        stats.total_messages >= (message_count * 7 / 10) as u64,
        "Resource monitor should track at least 70% of {} messages, got {}",
        message_count,
        stats.total_messages
    );

    println!(
        "Resource monitor tracked {}/{} messages ({:.1}%)",
        stats.total_messages,
        message_count,
        (stats.total_messages as f64 / message_count as f64) * 100.0
    );

    broker_handle.abort();
}

#[tokio::test]
async fn test_resource_monitoring_overhead() {
    // Test resource monitor performance by comparing with/without monitoring
    let config = BrokerConfig::default()
        .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
        .with_max_clients(10);

    let mut broker = MqttBroker::with_config(config).await.unwrap();
    let resource_monitor = broker.resource_monitor();
    let broker_addr = broker.local_addr().unwrap();

    let broker_handle = tokio::spawn(async move {
        let _ = broker.run().await;
    });

    sleep(Duration::from_millis(100)).await;

    let client = MqttClient::new("monitor-overhead-test");
    client.connect(&broker_addr.to_string()).await.unwrap();

    // Test resource monitor stats collection speed
    let stats_iterations = 1000;
    let start = Instant::now();

    for _ in 0..stats_iterations {
        let _stats = resource_monitor.get_stats().await;
    }

    let stats_time = start.elapsed();
    let stats_per_second = stats_iterations as f64 / stats_time.as_secs_f64();

    println!(
        "Resource stats collection: {:.2} calls/second",
        stats_per_second
    );

    // Stats collection should be very fast
    assert!(
        stats_per_second > 10_000.0,
        "Stats collection too slow: {:.2} calls/s < 10,000",
        stats_per_second
    );

    // Test publish with monitoring overhead
    let publish_count = 50;
    let start = Instant::now();

    for i in 0..publish_count {
        client
            .publish(&format!("overhead/test/{}", i), "test")
            .await
            .unwrap();
    }

    let publish_time = start.elapsed();
    let publish_rate = publish_count as f64 / publish_time.as_secs_f64();

    println!("Publish rate with monitoring: {:.2} msg/s", publish_rate);

    // Publishing should still be fast even with resource monitoring
    assert!(
        publish_rate > 100.0,
        "Publish rate with monitoring too slow: {:.2} msg/s < 100",
        publish_rate
    );

    broker_handle.abort();
}

#[tokio::test]
async fn test_connection_limit_performance() {
    // Test that connection limiting doesn't significantly impact performance
    let config = BrokerConfig::default()
        .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
        .with_max_clients(5); // Very low limit for testing

    let mut broker = MqttBroker::with_config(config).await.unwrap();
    let resource_monitor = broker.resource_monitor();
    let broker_addr = broker.local_addr().unwrap();

    let broker_handle = tokio::spawn(async move {
        let _ = broker.run().await;
    });

    sleep(Duration::from_millis(100)).await;

    // Connect up to the limit
    let mut clients = Vec::new();
    let start = Instant::now();

    for i in 0..5 {
        let client = MqttClient::new(format!("limit-client-{}", i));
        client.connect(&broker_addr.to_string()).await.unwrap();
        clients.push(client);
    }

    let connection_time = start.elapsed();

    // Verify we're at the limit
    let stats = resource_monitor.get_stats().await;
    assert_eq!(stats.current_connections, 5);
    assert_eq!(stats.connection_utilization(), 100.0);

    println!(
        "Connected {} clients in {:?}",
        clients.len(),
        connection_time
    );

    // Test that the 6th connection is rejected quickly (not blocked)
    let start = Instant::now();
    let client6 = MqttClient::new("rejected-client");
    let result = client6.connect(&broker_addr.to_string()).await;
    let rejection_time = start.elapsed();

    println!("Connection rejection took: {:?}", rejection_time);

    // Should be rejected quickly (under 100ms)
    assert!(
        rejection_time < Duration::from_millis(100),
        "Connection rejection took {:?}, should be < 100ms",
        rejection_time
    );

    // Should fail
    assert!(result.is_err(), "6th connection should be rejected");

    // Connection count should remain at limit
    let final_stats = resource_monitor.get_stats().await;
    assert_eq!(final_stats.current_connections, 5);

    broker_handle.abort();
}
