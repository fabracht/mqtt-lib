use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use mqtt_v5::callback::CallbackManager;
use mqtt_v5::packet::publish::PublishPacket;
use mqtt_v5::protocol::v5::properties::Properties;
use mqtt_v5::session::{SessionConfig, SessionState};
use mqtt_v5::*;
use std::sync::Arc;

// Benchmark concurrent callback dispatching
fn benchmark_callback_dispatch(c: &mut Criterion) {
    let mut group = c.benchmark_group("callback_dispatch");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Test with different numbers of callbacks
    for num_callbacks in [1, 10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_callbacks", num_callbacks),
            num_callbacks,
            |b, &num_callbacks| {
                let callback_manager = Arc::new(CallbackManager::new());

                // Register callbacks
                runtime.block_on(async {
                    for i in 0..num_callbacks {
                        let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
                        let counter_clone = counter.clone();
                        let callback: Arc<dyn Fn(PublishPacket) + Send + Sync> =
                            Arc::new(move |_msg| {
                                counter_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            });
                        callback_manager
                            .register(format!("test/topic/{}", i), callback)
                            .await
                            .unwrap();
                    }
                });

                // Create test message
                let message = PublishPacket {
                    topic_name: "test/topic/50".to_string(),
                    payload: vec![0u8; 128],
                    qos: QoS::AtMostOnce,
                    retain: false,
                    dup: false,
                    packet_id: None,
                    properties: Properties::new(),
                };

                b.iter(|| {
                    runtime.block_on(async {
                        callback_manager.dispatch(&message).await.unwrap();
                    });
                });
            },
        );
    }

    // Benchmark wildcard callback matching
    group.bench_function("wildcard_matching", |b| {
        let callback_manager = Arc::new(CallbackManager::new());

        runtime.block_on(async {
            // Register various wildcard patterns
            for pattern in [
                "test/+/data",
                "test/#",
                "+/topic/+",
                "sensors/+/temperature",
            ]
            .iter()
            {
                let callback: Arc<dyn Fn(PublishPacket) + Send + Sync> = Arc::new(|_msg| {});
                callback_manager
                    .register(pattern.to_string(), callback)
                    .await
                    .unwrap();
            }
        });

        let message = PublishPacket {
            topic_name: "test/device1/data".to_string(),
            payload: vec![0u8; 64],
            qos: QoS::AtMostOnce,
            retain: false,
            dup: false,
            packet_id: None,
            properties: Properties::new(),
        };

        b.iter(|| {
            runtime.block_on(async {
                callback_manager.dispatch(&message).await.unwrap();
                black_box(());
            });
        });
    });

    group.finish();
}

// Benchmark concurrent session access
fn benchmark_concurrent_session_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_session");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("read_write_contention", |b| {
        let session = Arc::new(SessionState::new(
            "bench-client".to_string(),
            SessionConfig::default(),
            true,
        ));

        b.iter(|| {
            runtime.block_on(async {
                let session_clone = session.clone();

                // Spawn multiple concurrent tasks
                let tasks: Vec<_> = (0..4)
                    .map(|i| {
                        let session = session_clone.clone();
                        tokio::spawn(async move {
                            if i % 2 == 0 {
                                // Read operation
                                black_box(session.matching_subscriptions("test/topic").await);
                            } else {
                                // Write operation
                                session.touch().await;
                            }
                        })
                    })
                    .collect();

                // Wait for all tasks
                for task in tasks {
                    task.await.unwrap();
                }
            });
        });
    });

    group.finish();
}

// Benchmark flow control operations
fn benchmark_flow_control(c: &mut Criterion) {
    let mut group = c.benchmark_group("flow_control");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("register_acknowledge_cycle", |b| {
        let session = SessionState::new("bench-client".to_string(), SessionConfig::default(), true);

        runtime.block_on(async {
            session.set_receive_maximum(100).await;
        });

        b.iter(|| {
            runtime.block_on(async {
                // Register and acknowledge messages
                for packet_id in 1..=50 {
                    session.register_in_flight(packet_id).await.unwrap();
                }
                for packet_id in 1..=50 {
                    session.acknowledge_in_flight(packet_id).await.unwrap();
                }
            });
        });
    });

    group.finish();
}

// Benchmark topic alias operations
fn benchmark_topic_alias(c: &mut Criterion) {
    let mut group = c.benchmark_group("topic_alias");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("create_and_lookup", |b| {
        let session = SessionState::new("bench-client".to_string(), SessionConfig::default(), true);

        runtime.block_on(async {
            session.set_topic_alias_maximum_out(100).await;
        });

        b.iter(|| {
            runtime.block_on(async {
                // Create aliases for new topics
                for i in 0..20 {
                    let topic = format!("test/topic/{}", i);
                    black_box(session.get_or_create_topic_alias(&topic).await);
                }

                // Lookup existing aliases
                for i in 0..20 {
                    let topic = format!("test/topic/{}", i);
                    black_box(session.get_or_create_topic_alias(&topic).await);
                }
            });
        });
    });

    group.finish();
}

// Benchmark message queueing under load
fn benchmark_message_queue_stress(c: &mut Criterion) {
    use mqtt_v5::session::queue::QueuedMessage;

    let mut group = c.benchmark_group("message_queue_stress");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("queue_dequeue_cycle", |b| {
        let config = SessionConfig {
            max_queued_messages: 1000,
            max_queued_size: 1_048_576, // 1MB
            ..SessionConfig::default()
        };

        let session = SessionState::new("bench-client".to_string(), config, true);

        b.iter(|| {
            runtime.block_on(async {
                // Queue messages
                for i in 0..100 {
                    let msg = QueuedMessage {
                        topic: format!("test/topic/{}", i),
                        payload: vec![0u8; 256],
                        qos: QoS::AtLeastOnce,
                        retain: false,
                        packet_id: Some(i as u16),
                    };
                    session.queue_message(msg).await.unwrap();
                }

                // Dequeue all messages
                black_box(session.dequeue_messages(100).await);
            });
        });
    });

    group.finish();
}

// Real-world scenario benchmarks
fn benchmark_real_world_scenarios(c: &mut Criterion) {
    let mut group = c.benchmark_group("real_world");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Simulate IoT sensor publishing
    group.bench_function("iot_sensor_publish", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let _client = MqttClient::new("bench-client");

                // Simulate 10 sensors publishing data
                for sensor_id in 0..10 {
                    let topic = format!("sensors/{}/temperature", sensor_id);
                    let payload = r#"{"temp": 23.5, "humidity": 45, "timestamp": 1234567890}"#;

                    let mut props = Properties::new();
                    props
                        .add(
                            mqtt_v5::protocol::v5::properties::PropertyId::ContentType,
                            mqtt_v5::protocol::v5::properties::PropertyValue::Utf8String(
                                "application/json".to_string(),
                            ),
                        )
                        .unwrap();

                    let packet = PublishPacket {
                        topic_name: topic,
                        payload: payload.as_bytes().to_vec(),
                        qos: QoS::AtLeastOnce,
                        retain: true,
                        dup: false,
                        packet_id: Some(sensor_id as u16),
                        properties: props,
                    };

                    black_box(packet);
                }
            });
        });
    });

    // Simulate high-frequency trading data
    group.bench_function("high_frequency_publish", |b| {
        let packets: Vec<PublishPacket> = (0..100)
            .map(|i| PublishPacket {
                topic_name: "market/trades/BTCUSD".to_string(),
                payload: format!(r#"{{"price": 50000.{}, "volume": 0.1, "side": "buy"}}"#, i)
                    .into_bytes(),
                qos: QoS::AtMostOnce,
                retain: false,
                dup: false,
                packet_id: None,
                properties: Properties::new(),
            })
            .collect();

        b.iter(|| {
            for packet in &packets {
                black_box(packet.clone());
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_callback_dispatch,
    benchmark_concurrent_session_access,
    benchmark_flow_control,
    benchmark_topic_alias,
    benchmark_message_queue_stress,
    benchmark_real_world_scenarios
);
criterion_main!(benches);
