use bytes::BytesMut;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use mqtt_v5::encoding::{encode_string, encode_variable_int};
use mqtt_v5::packet::{
    connect::ConnectPacket, publish::PublishPacket, subscribe::*, FixedHeader, MqttPacket,
};
use mqtt_v5::protocol::v5::properties::{Properties, PropertyId, PropertyValue};
use mqtt_v5::topic_matching::matches;
use mqtt_v5::types::ConnectOptions;
use mqtt_v5::validation::validate_topic_name;
use mqtt_v5::*;
use std::time::Duration;

// Packet encoding/decoding benchmarks
fn benchmark_packet_encoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("packet_encoding");

    // CONNECT packet encoding
    group.bench_function("connect", |b| {
        let options = ConnectOptions::new("test-client-12345");
        let packet = ConnectPacket::new(options);
        b.iter(|| {
            let mut buf = BytesMut::with_capacity(1024);
            packet.encode(&mut buf).unwrap();
            black_box(&buf);
        });
    });

    // PUBLISH packet encoding with different payload sizes
    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::new("publish", size), size, |b, &size| {
            let packet = PublishPacket {
                topic_name: "test/topic/benchmark".to_string(),
                payload: vec![0u8; size],
                qos: QoS::AtLeastOnce,
                retain: false,
                dup: false,
                packet_id: Some(12345),
                properties: Properties::new(),
            };
            b.iter(|| {
                let mut buf = BytesMut::with_capacity(size + 128);
                packet.encode(&mut buf).unwrap();
                black_box(&buf);
            });
        });
    }

    // SUBSCRIBE packet encoding
    group.bench_function("subscribe", |b| {
        let packet = SubscribePacket {
            packet_id: 42,
            properties: Properties::new(),
            filters: vec![
                TopicFilter {
                    filter: "test/+/topic".to_string(),
                    options: SubscriptionOptions::default(),
                },
                TopicFilter {
                    filter: "test/#".to_string(),
                    options: SubscriptionOptions::default(),
                },
            ],
        };
        b.iter(|| {
            let mut buf = BytesMut::with_capacity(256);
            packet.encode(&mut buf).unwrap();
            black_box(&buf);
        });
    });

    group.finish();
}

fn benchmark_packet_decoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("packet_decoding");

    // Pre-encode packets for decoding benchmarks
    let connect_bytes = {
        let options = ConnectOptions::new("test-client-12345");
        let packet = ConnectPacket::new(options);
        let mut buf = BytesMut::new();
        packet.encode(&mut buf).unwrap();
        buf.freeze()
    };

    let publish_bytes = {
        let packet = PublishPacket {
            topic_name: "test/topic/benchmark".to_string(),
            payload: vec![0u8; 1024],
            qos: QoS::AtLeastOnce,
            retain: false,
            dup: false,
            packet_id: Some(12345),
            properties: Properties::new(),
        };
        let mut buf = BytesMut::new();
        packet.encode(&mut buf).unwrap();
        buf.freeze()
    };

    // CONNECT packet decoding
    group.bench_function("connect", |b| {
        b.iter(|| {
            let mut buf = BytesMut::from(connect_bytes.as_ref());
            let header = FixedHeader::decode(&mut buf).unwrap();
            let packet = ConnectPacket::decode_body(&mut buf, &header).unwrap();
            black_box(packet);
        });
    });

    // PUBLISH packet decoding
    group.bench_function("publish_1k", |b| {
        b.iter(|| {
            let mut buf = BytesMut::from(publish_bytes.as_ref());
            let header = FixedHeader::decode(&mut buf).unwrap();
            let packet = PublishPacket::decode_body(&mut buf, &header).unwrap();
            black_box(packet);
        });
    });

    group.finish();
}

fn benchmark_properties(c: &mut Criterion) {
    let mut group = c.benchmark_group("properties");

    // Properties encoding
    group.bench_function("encode_multiple", |b| {
        let mut props = Properties::new();
        props
            .add(
                PropertyId::SessionExpiryInterval,
                PropertyValue::FourByteInteger(3600),
            )
            .unwrap();
        props
            .add(
                PropertyId::ReceiveMaximum,
                PropertyValue::TwoByteInteger(100),
            )
            .unwrap();
        props
            .add(
                PropertyId::MaximumPacketSize,
                PropertyValue::FourByteInteger(1_048_576),
            )
            .unwrap();
        props
            .add(
                PropertyId::TopicAliasMaximum,
                PropertyValue::TwoByteInteger(10),
            )
            .unwrap();
        props
            .add(
                PropertyId::UserProperty,
                PropertyValue::Utf8StringPair("key".to_string(), "value".to_string()),
            )
            .unwrap();

        b.iter(|| {
            let mut buf = BytesMut::with_capacity(256);
            props.encode(&mut buf).unwrap();
            black_box(&buf);
        });
    });

    // Properties decoding
    group.bench_function("decode_multiple", |b| {
        let mut props = Properties::new();
        props
            .add(
                PropertyId::SessionExpiryInterval,
                PropertyValue::FourByteInteger(3600),
            )
            .unwrap();
        props
            .add(
                PropertyId::ReceiveMaximum,
                PropertyValue::TwoByteInteger(100),
            )
            .unwrap();
        props
            .add(
                PropertyId::MaximumPacketSize,
                PropertyValue::FourByteInteger(1_048_576),
            )
            .unwrap();
        props
            .add(
                PropertyId::TopicAliasMaximum,
                PropertyValue::TwoByteInteger(10),
            )
            .unwrap();
        props
            .add(
                PropertyId::UserProperty,
                PropertyValue::Utf8StringPair("key".to_string(), "value".to_string()),
            )
            .unwrap();

        let encoded = {
            let mut buf = BytesMut::new();
            props.encode(&mut buf).unwrap();
            buf.freeze()
        };

        b.iter(|| {
            let mut buf = BytesMut::from(encoded.as_ref());
            let props = Properties::decode(&mut buf).unwrap();
            black_box(props);
        });
    });

    group.finish();
}

fn benchmark_encoding_primitives(c: &mut Criterion) {
    let mut group = c.benchmark_group("encoding_primitives");

    // String encoding
    group.bench_function("string_short", |b| {
        let s = "test/topic";
        b.iter(|| {
            let mut buf = BytesMut::with_capacity(32);
            encode_string(&mut buf, s).unwrap();
            black_box(&buf);
        });
    });

    group.bench_function("string_long", |b| {
        let s = "test/very/long/topic/name/that/represents/a/complex/hierarchy/in/mqtt";
        b.iter(|| {
            let mut buf = BytesMut::with_capacity(128);
            encode_string(&mut buf, s).unwrap();
            black_box(&buf);
        });
    });

    // Variable integer encoding
    group.bench_function("variable_int_small", |b| {
        b.iter(|| {
            let mut buf = BytesMut::with_capacity(8);
            encode_variable_int(&mut buf, 127).unwrap();
            black_box(&buf);
        });
    });

    group.bench_function("variable_int_large", |b| {
        b.iter(|| {
            let mut buf = BytesMut::with_capacity(8);
            encode_variable_int(&mut buf, 268_435_455).unwrap(); // Max 4-byte value
            black_box(&buf);
        });
    });

    group.finish();
}

fn benchmark_topic_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("topic_validation");

    group.bench_function("simple", |b| {
        let topic = "test/topic";
        b.iter(|| {
            validate_topic_name(topic).unwrap();
        });
    });

    group.bench_function("complex", |b| {
        let topic = "test/very/long/topic/name/with/many/levels/that/needs/validation";
        b.iter(|| {
            validate_topic_name(topic).unwrap();
        });
    });

    group.bench_function("unicode", |b| {
        let topic = "test/topic/with/émojis/🚀/and/special/characters/测试";
        b.iter(|| {
            validate_topic_name(topic).unwrap();
        });
    });

    group.finish();
}

fn benchmark_topic_matching(c: &mut Criterion) {
    let mut group = c.benchmark_group("topic_matching");

    // Exact match
    group.bench_function("exact", |b| {
        let topic = "test/topic/name";
        let filter = "test/topic/name";
        b.iter(|| {
            black_box(matches(topic, filter));
        });
    });

    // Single-level wildcard
    group.bench_function("single_wildcard", |b| {
        let topic = "test/topic/name";
        let filter = "test/+/name";
        b.iter(|| {
            black_box(matches(topic, filter));
        });
    });

    // Multi-level wildcard
    group.bench_function("multi_wildcard", |b| {
        let topic = "test/topic/name/with/many/levels";
        let filter = "test/#";
        b.iter(|| {
            black_box(matches(topic, filter));
        });
    });

    // Complex pattern
    group.bench_function("complex_pattern", |b| {
        let topic = "sensors/temperature/room1/device42";
        let filter = "sensors/+/+/device42";
        b.iter(|| {
            black_box(matches(topic, filter));
        });
    });

    group.finish();
}

fn benchmark_session_operations(c: &mut Criterion) {
    use mqtt_v5::session::queue::QueuedMessage;
    use mqtt_v5::session::{SessionConfig, SessionState, Subscription};

    let mut group = c.benchmark_group("session_operations");

    // Session state operations
    group.bench_function("add_subscription", |b| {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            runtime.block_on(async {
                let session =
                    SessionState::new("bench-client".to_string(), SessionConfig::default(), true);
                let sub = Subscription {
                    topic_filter: "test/+/topic".to_string(),
                    options: SubscriptionOptions::default(),
                };
                session
                    .add_subscription("test/+/topic".to_string(), sub)
                    .await
                    .unwrap();
                black_box(());
            });
        });
    });

    group.bench_function("queue_message", |b| {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            runtime.block_on(async {
                let session =
                    SessionState::new("bench-client".to_string(), SessionConfig::default(), true);
                let msg = QueuedMessage {
                    topic: "test/topic".to_string(),
                    payload: vec![0u8; 256],
                    qos: QoS::AtLeastOnce,
                    retain: false,
                    packet_id: Some(1),
                };
                let result = session.queue_message(msg).await.unwrap();
                black_box(result);
            });
        });
    });

    group.bench_function("matching_subscriptions", |b| {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let session = runtime.block_on(async {
            let session =
                SessionState::new("bench-client".to_string(), SessionConfig::default(), true);
            // Add multiple subscriptions
            for i in 0..10 {
                let sub = Subscription {
                    topic_filter: format!("test/{i}/+"),
                    options: SubscriptionOptions::default(),
                };
                session
                    .add_subscription(format!("test/{i}/+"), sub)
                    .await
                    .unwrap();
            }
            session
        });

        b.iter(|| {
            runtime.block_on(async {
                black_box(session.matching_subscriptions("test/5/data").await);
            });
        });
    });

    group.finish();
}

fn benchmark_client_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("client_operations");

    // Client creation
    group.bench_function("create", |b| {
        b.iter(|| {
            black_box(MqttClient::new("bench-client"));
        });
    });

    // Connect options building
    group.bench_function("build_connect_options", |b| {
        b.iter(|| {
            let mut options = ConnectOptions::new("bench-client-12345")
                .with_keep_alive(Duration::from_secs(60))
                .with_clean_start(true);
            // Add properties if needed
            options.properties.session_expiry_interval = Some(3600);
            options.properties.receive_maximum = Some(100);
            options.properties.maximum_packet_size = Some(1_048_576);
            options.properties.topic_alias_maximum = Some(10);
            black_box(options);
        });
    });

    group.finish();
}

// Memory allocation benchmarks
fn benchmark_memory_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocation");

    // Large payload allocation
    group.bench_function("large_payload", |b| {
        b.iter(|| {
            let packet = PublishPacket {
                topic_name: "test/topic".to_string(),
                payload: vec![0u8; 65536], // 64KB
                qos: QoS::AtLeastOnce,
                retain: false,
                dup: false,
                packet_id: Some(1),
                properties: Properties::new(),
            };
            black_box(packet);
        });
    });

    // Many small messages
    group.bench_function("many_small_messages", |b| {
        b.iter(|| {
            let messages: Vec<PublishPacket> = (0..100)
                .map(|i| PublishPacket {
                    topic_name: format!("test/topic/{i}"),
                    payload: vec![0u8; 64],
                    qos: QoS::AtMostOnce,
                    retain: false,
                    dup: false,
                    packet_id: None,
                    properties: Properties::new(),
                })
                .collect();
            black_box(messages);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_packet_encoding,
    benchmark_packet_decoding,
    benchmark_properties,
    benchmark_encoding_primitives,
    benchmark_topic_validation,
    benchmark_topic_matching,
    benchmark_session_operations,
    benchmark_client_operations,
    benchmark_memory_allocation
);
criterion_main!(benches);
