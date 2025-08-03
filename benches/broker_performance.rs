//! Working broker performance benchmarks
//!
//! This suite benchmarks the actual MQTT broker performance using proper APIs.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mqtt_v5::broker::{BrokerConfig, MqttBroker};
use mqtt_v5::client::MqttClient;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::sleep;

/// Benchmark broker startup time
fn benchmark_broker_startup(c: &mut Criterion) {
    let mut group = c.benchmark_group("broker_startup");

    group.bench_function("broker_creation", |b| {
        let rt = Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let config = BrokerConfig::default()
                    .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap());
                let _broker = MqttBroker::with_config(config).await.unwrap();
            });
        });
    });

    group.finish();
}

/// Benchmark connection establishment
fn benchmark_connection_establishment(c: &mut Criterion) {
    let mut group = c.benchmark_group("connection_establishment");
    let rt = Runtime::new().unwrap();

    // Start a broker for testing
    let (broker_addr, _broker_handle) = rt.block_on(async {
        let config = BrokerConfig::default()
            .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
            .with_max_clients(1000);
        let mut broker = MqttBroker::with_config(config).await.unwrap();
        let addr = broker.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            let _ = broker.run().await;
        });

        sleep(Duration::from_millis(100)).await;
        (addr, handle)
    });

    for num_connections in &[1, 5, 10, 20] {
        group.throughput(Throughput::Elements(*num_connections as u64));

        group.bench_with_input(
            BenchmarkId::new("sequential_connections", num_connections),
            num_connections,
            |b, &num_connections| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut clients = Vec::new();

                        let start = std::time::Instant::now();

                        for i in 0..num_connections {
                            let client = MqttClient::new(format!("bench-client-{}", i));
                            match client.connect(&broker_addr.to_string()).await {
                                Ok(_) => clients.push(client),
                                Err(_) => break, // Stop if we hit connection limits
                            }
                        }

                        let connect_time = start.elapsed();

                        // Clean disconnect
                        for client in clients {
                            let _ = client.disconnect().await;
                        }

                        connect_time
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark concurrent connections
fn benchmark_concurrent_connections(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_connections");
    let rt = Runtime::new().unwrap();

    let (broker_addr, _broker_handle) = rt.block_on(async {
        let config = BrokerConfig::default()
            .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
            .with_max_clients(1000);
        let mut broker = MqttBroker::with_config(config).await.unwrap();
        let addr = broker.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            let _ = broker.run().await;
        });

        sleep(Duration::from_millis(100)).await;
        (addr, handle)
    });

    for num_connections in &[5, 10, 20] {
        group.throughput(Throughput::Elements(*num_connections as u64));

        group.bench_with_input(
            BenchmarkId::new("concurrent_connections", num_connections),
            num_connections,
            |b, &num_connections| {
                b.iter(|| {
                    rt.block_on(async {
                        let start = std::time::Instant::now();

                        let tasks: Vec<_> = (0..num_connections)
                            .map(|i| {
                                let addr = broker_addr.to_string();
                                tokio::spawn(async move {
                                    let client =
                                        MqttClient::new(format!("concurrent-client-{}", i));
                                    let result = client.connect(&addr).await;
                                    if result.is_ok() {
                                        let _ = client.disconnect().await;
                                    }
                                    result.is_ok()
                                })
                            })
                            .collect();

                        let results: Vec<_> = futures::future::join_all(tasks)
                            .await
                            .into_iter()
                            .map(|r| r.unwrap_or(false))
                            .collect();

                        let connect_time = start.elapsed();
                        let success_count = results.iter().filter(|&&x| x).count();

                        (connect_time, success_count)
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark message publishing throughput
fn benchmark_message_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_throughput");
    let rt = Runtime::new().unwrap();

    let (broker_addr, _broker_handle) = rt.block_on(async {
        let config = BrokerConfig::default()
            .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
            .with_max_clients(100);
        let mut broker = MqttBroker::with_config(config).await.unwrap();
        let addr = broker.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            let _ = broker.run().await;
        });

        sleep(Duration::from_millis(100)).await;
        (addr, handle)
    });

    // Set up a publisher and subscriber
    let (publisher, _subscriber) = rt.block_on(async {
        let publisher = MqttClient::new("throughput-publisher");
        let subscriber = MqttClient::new("throughput-subscriber");

        let addr = broker_addr.to_string();
        publisher.connect(&addr).await.unwrap();
        subscriber.connect(&addr).await.unwrap();

        // Subscribe to test topic
        subscriber
            .subscribe("throughput/test", |_msg| {})
            .await
            .unwrap();

        sleep(Duration::from_millis(50)).await; // Let subscription settle

        (publisher, subscriber)
    });

    for payload_size in &[100, 1000, 10000] {
        group.throughput(Throughput::Bytes(*payload_size as u64));

        group.bench_with_input(
            BenchmarkId::new("publish_messages", payload_size),
            payload_size,
            |b, &payload_size| {
                let payload = "x".repeat(payload_size);

                b.iter(|| {
                    rt.block_on(async {
                        let start = std::time::Instant::now();

                        // Publish messages
                        for i in 0..10 {
                            let topic = format!("throughput/test/{}", i);
                            match publisher.publish(&topic, payload.as_bytes()).await {
                                Ok(_) => {}
                                Err(_) => break, // Stop on first error
                            }
                        }

                        start.elapsed()
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark resource monitoring overhead
fn benchmark_resource_monitoring(c: &mut Criterion) {
    let mut group = c.benchmark_group("resource_monitoring");
    let rt = Runtime::new().unwrap();

    // Test with resource monitoring enabled (default)
    let (broker_addr_monitored, _handle1) = rt.block_on(async {
        let config = BrokerConfig::default()
            .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
            .with_max_clients(100);
        let mut broker = MqttBroker::with_config(config).await.unwrap();
        let addr = broker.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            let _ = broker.run().await;
        });

        sleep(Duration::from_millis(100)).await;
        (addr, handle)
    });

    group.bench_function("with_resource_monitoring", |b| {
        let client = rt.block_on(async {
            let client = MqttClient::new("resource-monitor-test");
            client
                .connect(&broker_addr_monitored.to_string())
                .await
                .unwrap();
            client
        });

        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();

                // Publish several messages to test monitoring overhead
                for i in 0..5 {
                    let _ = client.publish(&format!("test/{}", i), "benchmark").await;
                }

                start.elapsed()
            })
        });
    });

    group.finish();
}

/// Benchmark broker statistics collection
fn benchmark_broker_stats(c: &mut Criterion) {
    let mut group = c.benchmark_group("broker_stats");
    let rt = Runtime::new().unwrap();

    let (broker, resource_monitor) = rt.block_on(async {
        let config = BrokerConfig::default()
            .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap());
        let broker = MqttBroker::with_config(config).await.unwrap();
        let resource_monitor = broker.resource_monitor();
        let _ = broker.local_addr().unwrap();

        // Don't start the broker, just use it for stats
        (broker, resource_monitor)
    });

    group.bench_function("get_broker_stats", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _stats = broker.stats();
            })
        });
    });

    group.bench_function("get_resource_stats", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _stats = resource_monitor.get_stats().await;
            })
        });
    });

    group.finish();
}

criterion_group!(
    broker_performance_benches,
    benchmark_broker_startup,
    benchmark_connection_establishment,
    benchmark_concurrent_connections,
    benchmark_message_throughput,
    benchmark_resource_monitoring,
    benchmark_broker_stats
);

criterion_main!(broker_performance_benches);
