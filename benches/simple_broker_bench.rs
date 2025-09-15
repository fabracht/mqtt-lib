//! Simple broker benchmark to test basic functionality

use criterion::{criterion_group, criterion_main, Criterion};
use mqtt5::broker::{BrokerConfig, MqttBroker};
use tokio::runtime::Runtime;

fn benchmark_broker_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("broker_creation", |b| {
        b.iter(|| {
            rt.block_on(async {
                let config = BrokerConfig::default()
                    .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap());
                let _broker = MqttBroker::with_config(config).await.unwrap();
            });
        });
    });
}

fn benchmark_resource_stats(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let resource_monitor = rt.block_on(async {
        let config = BrokerConfig::default()
            .with_bind_address("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap());
        let broker = MqttBroker::with_config(config).await.unwrap();
        broker.resource_monitor()
    });

    c.bench_function("get_resource_stats", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _stats = resource_monitor.get_stats().await;
            });
        });
    });
}

criterion_group!(
    simple_benches,
    benchmark_broker_creation,
    benchmark_resource_stats
);
criterion_main!(simple_benches);
