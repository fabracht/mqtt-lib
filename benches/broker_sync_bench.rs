//! Synchronous broker benchmarks using Criterion
//!
//! These benchmarks test non-async broker operations that can be measured with Criterion.

use criterion::{criterion_group, criterion_main, Criterion};
use mqtt_v5::broker::resource_monitor::{ResourceLimits, ResourceMonitor, ResourceStats};
use std::net::IpAddr;
use tokio::runtime::Runtime;

fn benchmark_resource_limits_creation(c: &mut Criterion) {
    c.bench_function("resource_limits_default", |b| {
        b.iter(|| {
            let _limits = ResourceLimits::default();
        });
    });

    c.bench_function("resource_limits_custom", |b| {
        b.iter(|| {
            let _limits = ResourceLimits {
                max_connections: 10000,
                max_connections_per_ip: 100,
                max_memory_bytes: 1024 * 1024 * 1024,
                max_message_rate_per_client: 1000,
                max_bandwidth_per_client: 10 * 1024 * 1024,
                max_connection_rate: 100,
                rate_limit_window: std::time::Duration::from_secs(60),
            };
        });
    });
}

fn benchmark_resource_monitor(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limits = ResourceLimits::default();
    let monitor = ResourceMonitor::new(limits);

    c.bench_function("resource_monitor_creation", |b| {
        b.iter(|| {
            let limits = ResourceLimits::default();
            let _monitor = ResourceMonitor::new(limits);
        });
    });

    c.bench_function("get_memory_usage", |b| {
        b.iter(|| {
            let _usage = monitor.get_memory_usage();
        });
    });

    c.bench_function("is_memory_limit_exceeded", |b| {
        b.iter(|| {
            let _exceeded = monitor.is_memory_limit_exceeded();
        });
    });

    c.bench_function("get_stats", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _stats = monitor.get_stats().await;
            });
        });
    });
}

fn benchmark_resource_stats_operations(c: &mut Criterion) {
    let stats = ResourceStats {
        current_connections: 500,
        max_connections: 1000,
        unique_ips: 200,
        active_clients: 450,
        total_messages: 1_000_000,
        total_bytes: 500_000_000,
    };

    c.bench_function("connection_utilization", |b| {
        b.iter(|| {
            let _utilization = stats.connection_utilization();
        });
    });
}

fn benchmark_ip_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let monitor = ResourceMonitor::new(ResourceLimits::default());
    let ip: IpAddr = "192.168.1.100".parse().unwrap();

    c.bench_function("can_accept_connection", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _can_accept = monitor.can_accept_connection(ip).await;
            });
        });
    });

    c.bench_function("register_connection", |b| {
        let mut counter = 0;
        b.iter(|| {
            rt.block_on(async {
                counter += 1;
                monitor
                    .register_connection(format!("client-{}", counter), ip)
                    .await;
            });
        });
    });
}

criterion_group!(
    broker_sync_benches,
    benchmark_resource_limits_creation,
    benchmark_resource_monitor,
    benchmark_resource_stats_operations,
    benchmark_ip_operations
);

criterion_main!(broker_sync_benches);
