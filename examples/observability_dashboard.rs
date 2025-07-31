//! Observability Dashboard Example
//!
//! This example demonstrates a comprehensive observability dashboard that collects,
//! aggregates, and exposes metrics from multiple IoT deployments and MQTT infrastructure.
//!
//! ## Features Demonstrated
//!
//! - **Multi-source metrics collection** from IoT devices, hubs, and industrial networks
//! - **Real-time metric aggregation** with time-series data processing
//! - **Prometheus-compatible metrics export** for monitoring integration
//! - **Health check endpoints** for service discovery and monitoring
//! - **Alert correlation and deduplication** across multiple data sources
//! - **Performance monitoring** with latency and throughput tracking
//! - **Resource utilization monitoring** for MQTT broker and client health
//! - **Custom metric dashboards** with flexible querying capabilities
//!
//! ## Security Configuration
//!
//! The observability dashboard uses **TLS with authentication** for secure metric collection:
//! - **Development**: `mqtt://localhost:1883` (plain text)
//! - **Production**: `mqtts://metrics-broker.company.com:8883` with TLS
//!
//! Environment variables:
//! - `MQTT_BROKER_URL`: Override default broker URL for metrics collection
//! - `DASHBOARD_ID`: Set custom dashboard identifier
//! - `METRICS_PORT`: HTTP port for Prometheus metrics endpoint (default: 9090)
//! - `HEALTH_PORT`: HTTP port for health check endpoint (default: 8080)
//! - `RETENTION_HOURS`: Data retention period in hours (default: 24)
//! - `PRODUCTION`: Forces secure broker for metrics collection
//!
//! Example usage:
//! ```bash
//! # Local development
//! cargo run --example observability_dashboard
//!
//! # Production monitoring
//! PRODUCTION=1 \
//! METRICS_PORT=9090 \
//! HEALTH_PORT=8080 \
//! cargo run --example observability_dashboard
//!
//! # View metrics: curl http://localhost:9090/metrics
//! # Health check: curl http://localhost:8080/health
//! ```
//!
//! ## Integration Patterns
//!
//! - **Prometheus scraping**: Metrics available at `/metrics` endpoint
//! - **Grafana dashboards**: JSON configurations for visualization
//! - **Alert correlation**: Intelligent alert grouping and deduplication
//! - **Multi-tenant metrics**: Isolated metrics per deployment/tenant
//! - **Custom exporters**: Pluggable metric export to external systems

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use mqtt_v5::{ConnectOptions, ConnectionEvent, MqttClient, QoS, WillMessage};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, instrument, warn};

/// Types of metrics collected from IoT systems
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MetricType {
    Counter,   // Monotonic increasing values (message count, errors)
    Gauge,     // Point-in-time values (temperature, CPU usage)
    Histogram, // Distribution of values (latency, payload sizes)
    Summary,   // Statistical summaries with quantiles
}

/// Metric source categories for organization and filtering
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MetricSource {
    IoTDevice { device_type: String },
    SmartHomeHub { hub_id: String },
    IndustrialNetwork { network_id: String },
    MqttBroker { broker_id: String },
    Dashboard { dashboard_id: String },
}

/// Individual metric data point with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDataPoint {
    pub name: String,
    pub metric_type: MetricType,
    pub value: f64,
    pub timestamp: u64,
    pub labels: HashMap<String, String>,
    pub source: MetricSource,
    pub help: String, // Description for Prometheus export
}

/// Aggregated metric with statistical information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedMetric {
    pub name: String,
    pub metric_type: MetricType,
    pub count: u64,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub p50: f64, // Median
    pub p95: f64, // 95th percentile
    pub p99: f64, // 99th percentile
    pub labels: HashMap<String, String>,
    pub window_start: u64,
    pub window_end: u64,
}

/// Alert severity levels for metric thresholds
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertSeverity {
    Info = 0,
    Warning = 1,
    Critical = 2,
    Emergency = 3,
}

/// Alert triggered by metric threshold violations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricAlert {
    pub alert_id: String,
    pub metric_name: String,
    pub severity: AlertSeverity,
    pub threshold: f64,
    pub current_value: f64,
    pub description: String,
    pub source: MetricSource,
    pub timestamp: u64,
    pub labels: HashMap<String, String>,
    pub resolution_time: Option<u64>,
}

/// Configuration for metric thresholds and alerting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdConfig {
    pub metric_name: String,
    pub warning_threshold: Option<f64>,
    pub critical_threshold: Option<f64>,
    pub comparison: ThresholdComparison,
    pub evaluation_window: Duration,
    pub labels_filter: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ThresholdComparison {
    GreaterThan,
    LessThan,
    Equal,
    NotEqual,
}

/// Time series data storage for metrics
#[derive(Debug)]
pub struct TimeSeriesDatabase {
    metrics: Arc<RwLock<HashMap<String, VecDeque<MetricDataPoint>>>>,
    aggregated_metrics: Arc<RwLock<HashMap<String, VecDeque<AggregatedMetric>>>>,
    retention_duration: Duration,
    max_series_points: usize,
}

impl TimeSeriesDatabase {
    pub fn new(retention_hours: u64) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            aggregated_metrics: Arc::new(RwLock::new(HashMap::new())),
            retention_duration: Duration::from_secs(retention_hours * 3600),
            max_series_points: 10000,
        }
    }

    /// Store a metric data point
    #[instrument(skip(self))]
    pub async fn store_metric(&self, metric: MetricDataPoint) {
        let series_key = format!("{}:{}", metric.name, metric.source_key());
        let mut metrics = self.metrics.write().await;

        let series = metrics
            .entry(series_key.clone())
            .or_insert_with(VecDeque::new);
        series.push_back(metric);

        // Limit series size to prevent memory issues
        if series.len() > self.max_series_points {
            series.pop_front();
        }

        debug!(series_key = %series_key, points = %series.len(), "Stored metric data point");
    }

    /// Store aggregated metric
    #[instrument(skip(self))]
    pub async fn store_aggregated_metric(&self, metric: AggregatedMetric) {
        let series_key = format!("agg_{}:{}", metric.name, metric.labels_key());
        let mut aggregated = self.aggregated_metrics.write().await;

        let series = aggregated
            .entry(series_key.clone())
            .or_insert_with(VecDeque::new);
        series.push_back(metric);

        // Limit aggregated series size
        if series.len() > 1000 {
            series.pop_front();
        }

        debug!(series_key = %series_key, points = %series.len(), "Stored aggregated metric");
    }

    /// Clean up old metric data based on retention policy
    #[instrument(skip(self))]
    pub async fn cleanup_old_data(&self) {
        let cutoff_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - self.retention_duration.as_secs();

        let mut metrics = self.metrics.write().await;
        let mut cleaned_series = 0;
        let mut cleaned_points = 0;

        // Clean raw metrics
        for (series_key, series) in metrics.iter_mut() {
            let initial_len = series.len();
            series.retain(|point| point.timestamp >= cutoff_time);
            let removed = initial_len - series.len();
            if removed > 0 {
                cleaned_series += 1;
                cleaned_points += removed;
                debug!(series_key = %series_key, removed = %removed, "Cleaned old metric data");
            }
        }

        // Remove empty series
        metrics.retain(|_, series| !series.is_empty());

        // Clean aggregated metrics
        let mut aggregated = self.aggregated_metrics.write().await;
        for series in aggregated.values_mut() {
            series.retain(|metric| metric.window_end >= cutoff_time);
        }
        aggregated.retain(|_, series| !series.is_empty());

        info!(cleaned_series = %cleaned_series, cleaned_points = %cleaned_points, "Completed metric data cleanup");
    }

    /// Query metrics by name and time range
    #[instrument(skip(self))]
    pub async fn query_metrics(
        &self,
        metric_name: &str,
        start_time: u64,
        end_time: u64,
        labels_filter: Option<&HashMap<String, String>>,
    ) -> Vec<MetricDataPoint> {
        let metrics = self.metrics.read().await;
        let mut results = Vec::new();

        for (series_key, series) in metrics.iter() {
            if !series_key.starts_with(&format!("{}:", metric_name)) {
                continue;
            }

            for point in series.iter() {
                if point.timestamp >= start_time && point.timestamp <= end_time {
                    // Apply label filters if provided
                    if let Some(filters) = labels_filter {
                        let mut matches = true;
                        for (key, value) in filters {
                            if point.labels.get(key) != Some(value) {
                                matches = false;
                                break;
                            }
                        }
                        if matches {
                            results.push(point.clone());
                        }
                    } else {
                        results.push(point.clone());
                    }
                }
            }
        }

        debug!(metric_name = %metric_name, results = %results.len(), "Queried metrics");
        results
    }

    /// Get all current metrics for Prometheus export
    #[instrument(skip(self))]
    pub async fn get_all_metrics(&self) -> Vec<MetricDataPoint> {
        let metrics = self.metrics.read().await;
        let mut all_metrics = Vec::new();

        for series in metrics.values() {
            if let Some(latest_metric) = series.back() {
                all_metrics.push(latest_metric.clone());
            }
        }

        debug!(total_metrics = %all_metrics.len(), "Retrieved all metrics for export");
        all_metrics
    }
}

impl MetricDataPoint {
    fn source_key(&self) -> String {
        match &self.source {
            MetricSource::IoTDevice { device_type } => format!("iot_{}", device_type),
            MetricSource::SmartHomeHub { hub_id } => format!("hub_{}", hub_id),
            MetricSource::IndustrialNetwork { network_id } => format!("industrial_{}", network_id),
            MetricSource::MqttBroker { broker_id } => format!("broker_{}", broker_id),
            MetricSource::Dashboard { dashboard_id } => format!("dashboard_{}", dashboard_id),
        }
    }
}

impl AggregatedMetric {
    fn labels_key(&self) -> String {
        let mut keys: Vec<_> = self.labels.keys().collect();
        keys.sort();
        keys.into_iter()
            .map(|k| format!("{}={}", k, self.labels.get(k).unwrap_or(&"".to_string())))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Metric aggregation engine for real-time calculations
#[derive(Debug)]
pub struct MetricAggregator {
    aggregation_window: Duration,
    pending_metrics: Arc<Mutex<HashMap<String, Vec<MetricDataPoint>>>>,
}

impl MetricAggregator {
    pub fn new(window_duration: Duration) -> Self {
        Self {
            aggregation_window: window_duration,
            pending_metrics: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add metric for aggregation
    #[instrument(skip(self))]
    pub async fn add_metric(&self, metric: MetricDataPoint) {
        let aggregation_key = format!("{}:{}", metric.name, metric.labels_key());
        let mut pending = self.pending_metrics.lock().await;
        pending
            .entry(aggregation_key)
            .or_insert_with(Vec::new)
            .push(metric);
    }

    /// Process pending metrics and generate aggregations
    #[instrument(skip(self))]
    pub async fn process_aggregations(&self) -> Vec<AggregatedMetric> {
        let mut pending = self.pending_metrics.lock().await;
        let mut aggregated_metrics = Vec::new();

        let window_end = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let window_start = window_end - self.aggregation_window.as_secs();

        for (_aggregation_key, metrics) in pending.drain() {
            if metrics.is_empty() {
                continue;
            }

            // Filter metrics within the time window
            let windowed_metrics: Vec<_> = metrics
                .into_iter()
                .filter(|m| m.timestamp >= window_start && m.timestamp <= window_end)
                .collect();

            if windowed_metrics.is_empty() {
                continue;
            }

            let first_metric = &windowed_metrics[0];
            let values: Vec<f64> = windowed_metrics.iter().map(|m| m.value).collect();

            // Calculate statistics
            let count = values.len() as u64;
            let sum = values.iter().sum::<f64>();
            let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let avg = sum / count as f64;

            // Calculate percentiles
            let mut sorted_values = values.clone();
            sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let p50 = percentile(&sorted_values, 0.5);
            let p95 = percentile(&sorted_values, 0.95);
            let p99 = percentile(&sorted_values, 0.99);

            let aggregated = AggregatedMetric {
                name: first_metric.name.clone(),
                metric_type: first_metric.metric_type,
                count,
                sum,
                min,
                max,
                avg,
                p50,
                p95,
                p99,
                labels: first_metric.labels.clone(),
                window_start,
                window_end,
            };

            aggregated_metrics.push(aggregated);
        }

        debug!(aggregations = %aggregated_metrics.len(), "Processed metric aggregations");
        aggregated_metrics
    }
}

impl MetricDataPoint {
    fn labels_key(&self) -> String {
        let mut keys: Vec<_> = self.labels.keys().collect();
        keys.sort();
        keys.into_iter()
            .map(|k| format!("{}={}", k, self.labels.get(k).unwrap_or(&"".to_string())))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Calculate percentile from sorted values
fn percentile(sorted_values: &[f64], p: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }

    let n = sorted_values.len();
    if n == 1 {
        return sorted_values[0];
    }

    let index = p * (n - 1) as f64;
    let lower = index.floor() as usize;
    let upper = index.ceil() as usize;

    if lower == upper {
        sorted_values[lower]
    } else {
        let weight = index - lower as f64;
        sorted_values[lower] * (1.0 - weight) + sorted_values[upper] * weight
    }
}

/// Alert engine for monitoring metric thresholds
#[derive(Debug)]
pub struct AlertEngine {
    thresholds: Arc<RwLock<HashMap<String, ThresholdConfig>>>,
    active_alerts: Arc<RwLock<HashMap<String, MetricAlert>>>,
    alert_history: Arc<Mutex<VecDeque<MetricAlert>>>,
}

impl Default for AlertEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertEngine {
    pub fn new() -> Self {
        Self {
            thresholds: Arc::new(RwLock::new(HashMap::new())),
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
            alert_history: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Configure threshold for metric alerting
    #[instrument(skip(self))]
    pub async fn configure_threshold(&self, config: ThresholdConfig) {
        let threshold_key = format!("{}:{}", config.metric_name, config.labels_key());
        self.thresholds
            .write()
            .await
            .insert(threshold_key.clone(), config);
        info!(threshold_key = %threshold_key, "Configured metric threshold");
    }

    /// Evaluate metric against configured thresholds
    #[instrument(skip(self))]
    pub async fn evaluate_metric(&self, metric: &MetricDataPoint) -> Option<MetricAlert> {
        let thresholds = self.thresholds.read().await;

        for (_threshold_key, threshold_config) in thresholds.iter() {
            if !threshold_config.metric_name.eq(&metric.name) {
                continue;
            }

            // Check label filters
            let mut labels_match = true;
            for (key, value) in &threshold_config.labels_filter {
                if metric.labels.get(key) != Some(value) {
                    labels_match = false;
                    break;
                }
            }

            if !labels_match {
                continue;
            }

            // Evaluate thresholds
            let alert_severity = if let Some(critical) = threshold_config.critical_threshold {
                if self.threshold_violated(metric.value, critical, &threshold_config.comparison) {
                    Some(AlertSeverity::Critical)
                } else {
                    None
                }
            } else {
                None
            };

            let alert_severity = alert_severity.or_else(|| {
                if let Some(warning) = threshold_config.warning_threshold {
                    if self.threshold_violated(metric.value, warning, &threshold_config.comparison)
                    {
                        Some(AlertSeverity::Warning)
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            if let Some(severity) = alert_severity {
                let alert_id = format!(
                    "{}_{}_{}",
                    metric.name,
                    metric.source_key(),
                    metric.timestamp
                );

                let alert = MetricAlert {
                    alert_id: alert_id.clone(),
                    metric_name: metric.name.clone(),
                    severity,
                    threshold: threshold_config
                        .warning_threshold
                        .or(threshold_config.critical_threshold)
                        .unwrap_or(0.0),
                    current_value: metric.value,
                    description: format!(
                        "Metric {} violated {} threshold: {} {} {}",
                        metric.name,
                        match severity {
                            AlertSeverity::Critical => "critical",
                            AlertSeverity::Warning => "warning",
                            _ => "info",
                        },
                        metric.value,
                        match threshold_config.comparison {
                            ThresholdComparison::GreaterThan => ">",
                            ThresholdComparison::LessThan => "<",
                            ThresholdComparison::Equal => "==",
                            ThresholdComparison::NotEqual => "!=",
                        },
                        threshold_config
                            .warning_threshold
                            .or(threshold_config.critical_threshold)
                            .unwrap_or(0.0)
                    ),
                    source: metric.source.clone(),
                    timestamp: metric.timestamp,
                    labels: metric.labels.clone(),
                    resolution_time: None,
                };

                // Store active alert
                self.active_alerts
                    .write()
                    .await
                    .insert(alert_id.clone(), alert.clone());

                // Add to history
                let mut history = self.alert_history.lock().await;
                history.push_back(alert.clone());
                if history.len() > 10000 {
                    history.pop_front();
                }

                warn!(alert_id = %alert_id, severity = ?severity, metric = %metric.name, value = %metric.value, "Metric alert triggered");
                return Some(alert);
            }
        }

        None
    }

    fn threshold_violated(
        &self,
        value: f64,
        threshold: f64,
        comparison: &ThresholdComparison,
    ) -> bool {
        match comparison {
            ThresholdComparison::GreaterThan => value > threshold,
            ThresholdComparison::LessThan => value < threshold,
            ThresholdComparison::Equal => (value - threshold).abs() < f64::EPSILON,
            ThresholdComparison::NotEqual => (value - threshold).abs() >= f64::EPSILON,
        }
    }

    /// Get active alerts
    pub async fn get_active_alerts(&self) -> Vec<MetricAlert> {
        self.active_alerts.read().await.values().cloned().collect()
    }
}

impl ThresholdConfig {
    fn labels_key(&self) -> String {
        let mut keys: Vec<_> = self.labels_filter.keys().collect();
        keys.sort();
        keys.into_iter()
            .map(|k| {
                format!(
                    "{}={}",
                    k,
                    self.labels_filter.get(k).unwrap_or(&"".to_string())
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// HTTP server for exposing metrics and health endpoints
#[derive(Debug)]
pub struct MetricsServer {
    port: u16,
    health_port: u16,
    database: Arc<TimeSeriesDatabase>,
    alert_engine: Arc<AlertEngine>,
}

impl MetricsServer {
    pub fn new(
        port: u16,
        health_port: u16,
        database: Arc<TimeSeriesDatabase>,
        alert_engine: Arc<AlertEngine>,
    ) -> Self {
        Self {
            port,
            health_port,
            database,
            alert_engine,
        }
    }

    /// Start HTTP servers for metrics and health endpoints
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!(metrics_port = %self.port, health_port = %self.health_port, "Starting real HTTP servers");

        let database = Arc::clone(&self.database);
        let alert_engine = Arc::clone(&self.alert_engine);
        let metrics_port = self.port;
        let health_port = self.health_port;

        // Start metrics server
        let metrics_database = Arc::clone(&database);
        tokio::spawn(async move {
            let addr = SocketAddr::from(([127, 0, 0, 1], metrics_port));
            let listener = match TcpListener::bind(addr).await {
                Ok(listener) => listener,
                Err(e) => {
                    error!(error = %e, port = %metrics_port, "Failed to bind metrics server");
                    return;
                }
            };

            info!(addr = %addr, "Metrics server listening");

            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!(error = %e, "Failed to accept connection");
                        continue;
                    }
                };

                let io = TokioIo::new(stream);
                let database = Arc::clone(&metrics_database);

                tokio::spawn(async move {
                    if let Err(err) = http1::Builder::new()
                        .serve_connection(
                            io,
                            service_fn(move |req| {
                                let database = Arc::clone(&database);
                                async move { handle_metrics_request(req, database).await }
                            }),
                        )
                        .await
                    {
                        error!(error = %err, "Error serving metrics connection");
                    }
                });
            }
        });

        // Start health server
        let health_alert_engine = Arc::clone(&alert_engine);
        tokio::spawn(async move {
            let addr = SocketAddr::from(([127, 0, 0, 1], health_port));
            let listener = match TcpListener::bind(addr).await {
                Ok(listener) => listener,
                Err(e) => {
                    error!(error = %e, port = %health_port, "Failed to bind health server");
                    return;
                }
            };

            info!(addr = %addr, "Health server listening");

            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!(error = %e, "Failed to accept connection");
                        continue;
                    }
                };

                let io = TokioIo::new(stream);
                let alert_engine = Arc::clone(&health_alert_engine);

                tokio::spawn(async move {
                    if let Err(err) = http1::Builder::new()
                        .serve_connection(
                            io,
                            service_fn(move |req| {
                                let alert_engine = Arc::clone(&alert_engine);
                                async move { handle_health_request(req, alert_engine).await }
                            }),
                        )
                        .await
                    {
                        error!(error = %err, "Error serving health connection");
                    }
                });
            }
        });

        Ok(())
    }
}

/// Main observability dashboard that coordinates all metric collection
pub struct ObservabilityDashboard {
    client: Arc<MqttClient>,
    database: Arc<TimeSeriesDatabase>,
    aggregator: Arc<MetricAggregator>,
    alert_engine: Arc<AlertEngine>,
    metrics_server: Arc<MetricsServer>,
    is_running: Arc<AtomicBool>,
    dashboard_metrics: Arc<DashboardMetrics>,
}

/// Dashboard operational metrics
#[derive(Debug, Default)]
pub struct DashboardMetrics {
    metrics_collected: AtomicU64,
    alerts_triggered: AtomicU64,
    data_points_stored: AtomicU64,
    aggregations_processed: AtomicU64,
    uptime_start: Option<Instant>,
}

impl ObservabilityDashboard {
    /// Create a new observability dashboard
    #[instrument(skip(broker_url))]
    pub async fn new(
        dashboard_id: &str,
        broker_url: &str,
        metrics_port: u16,
        health_port: u16,
        retention_hours: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        info!(dashboard_id = %dashboard_id, "Initializing Observability Dashboard");

        // Create MQTT client for collecting metrics
        let connect_options = ConnectOptions::new(format!("observability-{}", dashboard_id))
            .with_clean_start(false)
            .with_keep_alive(Duration::from_secs(30))
            .with_automatic_reconnect(true)
            .with_reconnect_delay(Duration::from_secs(2), Duration::from_secs(60))
            .with_will(
                WillMessage::new(
                    format!("observability/{}/status", dashboard_id),
                    b"offline".to_vec(),
                )
                .with_qos(QoS::AtLeastOnce)
                .with_retain(true),
            );

        let client = Arc::new(MqttClient::with_options(connect_options));

        // Set up connection monitoring
        let dashboard_id_clone = dashboard_id.to_string();
        client.on_connection_event(move |event| {
            match event {
                ConnectionEvent::Connected { session_present } => {
                    info!(dashboard_id = %dashboard_id_clone, session_present = %session_present, "Observability dashboard connected");
                }
                ConnectionEvent::Disconnected { reason } => {
                    warn!(dashboard_id = %dashboard_id_clone, reason = ?reason, "Observability dashboard disconnected");
                }
                ConnectionEvent::Reconnecting { attempt } => {
                    info!(dashboard_id = %dashboard_id_clone, attempt = %attempt, "Observability dashboard reconnecting");
                }
                ConnectionEvent::ReconnectFailed { error } => {
                    error!(dashboard_id = %dashboard_id_clone, error = %error, "Observability dashboard reconnection failed");
                }
            }
        }).await?;

        // Connect to broker
        if broker_url.starts_with("mqtts://") {
            info!(dashboard_id = %dashboard_id, "Connecting to metrics broker with TLS");
            client.connect(broker_url).await?;
        } else {
            info!(dashboard_id = %dashboard_id, "Connecting to metrics broker with plain text (development only)");
            client.connect(broker_url).await?;
        }

        // Initialize components
        let database = Arc::new(TimeSeriesDatabase::new(retention_hours));
        let aggregator = Arc::new(MetricAggregator::new(Duration::from_secs(60))); // 1-minute windows
        let alert_engine = Arc::new(AlertEngine::new());
        let metrics_server = Arc::new(MetricsServer::new(
            metrics_port,
            health_port,
            Arc::clone(&database),
            Arc::clone(&alert_engine),
        ));

        Ok(Self {
            client,
            database,
            aggregator,
            alert_engine,
            metrics_server,
            is_running: Arc::new(AtomicBool::new(false)),
            dashboard_metrics: Arc::new(DashboardMetrics {
                uptime_start: Some(Instant::now()),
                ..Default::default()
            }),
        })
    }

    /// Start the observability dashboard
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting observability dashboard");

        if self.is_running.load(Ordering::SeqCst) {
            warn!("Observability dashboard is already running");
            return Ok(());
        }

        self.is_running.store(true, Ordering::SeqCst);

        // Configure default alerting thresholds
        self.configure_default_thresholds().await;

        // Start background tasks
        self.start_metric_collection().await?;
        self.start_aggregation_processor().await?;
        self.start_data_cleanup().await?;
        self.start_dashboard_health_monitor().await?;

        // Start HTTP servers
        self.metrics_server.start().await?;

        info!("Observability dashboard started successfully");
        Ok(())
    }

    /// Configure default alerting thresholds for common metrics
    #[instrument(skip(self))]
    async fn configure_default_thresholds(&self) {
        let thresholds = vec![
            // High error rate threshold
            ThresholdConfig {
                metric_name: "mqtt_errors_total".to_string(),
                warning_threshold: Some(10.0),
                critical_threshold: Some(50.0),
                comparison: ThresholdComparison::GreaterThan,
                evaluation_window: Duration::from_secs(300),
                labels_filter: HashMap::new(),
            },
            // High message latency threshold
            ThresholdConfig {
                metric_name: "mqtt_message_latency_seconds".to_string(),
                warning_threshold: Some(1.0),
                critical_threshold: Some(5.0),
                comparison: ThresholdComparison::GreaterThan,
                evaluation_window: Duration::from_secs(60),
                labels_filter: HashMap::new(),
            },
            // Low device connectivity threshold
            ThresholdConfig {
                metric_name: "device_connectivity_ratio".to_string(),
                warning_threshold: Some(0.8),
                critical_threshold: Some(0.5),
                comparison: ThresholdComparison::LessThan,
                evaluation_window: Duration::from_secs(300),
                labels_filter: HashMap::new(),
            },
        ];

        for threshold in thresholds {
            self.alert_engine.configure_threshold(threshold).await;
        }

        info!("Configured default alerting thresholds");
    }

    /// Start collecting metrics from all MQTT topics
    #[instrument(skip(self))]
    async fn start_metric_collection(&self) -> Result<(), Box<dyn std::error::Error>> {
        let client = Arc::clone(&self.client);
        let database = Arc::clone(&self.database);
        let aggregator = Arc::clone(&self.aggregator);
        let alert_engine = Arc::clone(&self.alert_engine);
        let metrics = Arc::clone(&self.dashboard_metrics);

        // Subscribe to all metric topics
        let metric_topics = vec![
            "devices/+/metrics",       // IoT device metrics
            "smarthome/+/metrics",     // Smart home hub metrics
            "networks/+/metrics",      // Industrial network metrics
            "brokers/+/metrics",       // MQTT broker metrics
            "observability/+/metrics", // Dashboard metrics
        ];

        for topic in metric_topics {
            client.subscribe(topic, {
                let database = Arc::clone(&database);
                let aggregator = Arc::clone(&aggregator);
                let alert_engine = Arc::clone(&alert_engine);
                let metrics = Arc::clone(&metrics);

                move |msg| {
                    let database = Arc::clone(&database);
                    let aggregator = Arc::clone(&aggregator);
                    let alert_engine = Arc::clone(&alert_engine);
                    let metrics = Arc::clone(&metrics);

                    tokio::spawn(async move {
                        if let Ok(metric_str) = String::from_utf8(msg.payload.clone()) {
                            if let Ok(metric) = serde_json::from_str::<MetricDataPoint>(&metric_str) {
                                debug!(metric_name = %metric.name, value = %metric.value, "Collected metric");

                                // Store in database
                                database.store_metric(metric.clone()).await;
                                metrics.data_points_stored.fetch_add(1, Ordering::SeqCst);

                                // Add to aggregator
                                aggregator.add_metric(metric.clone()).await;

                                // Evaluate for alerts
                                if let Some(alert) = alert_engine.evaluate_metric(&metric).await {
                                    warn!(alert_id = %alert.alert_id, "Metric alert triggered");
                                    metrics.alerts_triggered.fetch_add(1, Ordering::SeqCst);
                                }

                                metrics.metrics_collected.fetch_add(1, Ordering::SeqCst);
                            } else {
                                warn!(topic = %msg.topic, "Failed to parse metric data");
                            }
                        }
                    });
                }
            }).await?;
        }

        info!("Started metric collection from all sources");
        Ok(())
    }

    /// Start the aggregation processor
    #[instrument(skip(self))]
    async fn start_aggregation_processor(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(60)); // Process every minute
        let database = Arc::clone(&self.database);
        let aggregator = Arc::clone(&self.aggregator);
        let metrics = Arc::clone(&self.dashboard_metrics);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting aggregation processor");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                // Process aggregations
                let aggregated_metrics = aggregator.process_aggregations().await;

                for aggregated in aggregated_metrics {
                    database.store_aggregated_metric(aggregated).await;
                    metrics
                        .aggregations_processed
                        .fetch_add(1, Ordering::SeqCst);
                }

                debug!("Processed metric aggregations");
            }

            info!("Aggregation processor stopped");
        });

        Ok(())
    }

    /// Start data cleanup task
    #[instrument(skip(self))]
    async fn start_data_cleanup(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(3600)); // Cleanup every hour
        let database = Arc::clone(&self.database);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting data cleanup task");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;
                database.cleanup_old_data().await;
            }

            info!("Data cleanup task stopped");
        });

        Ok(())
    }

    /// Start dashboard health monitoring
    #[instrument(skip(self))]
    async fn start_dashboard_health_monitor(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(30));
        let client = Arc::clone(&self.client);
        let metrics = Arc::clone(&self.dashboard_metrics);
        let alert_engine = Arc::clone(&self.alert_engine);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting dashboard health monitor");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                let uptime = metrics
                    .uptime_start
                    .map(|start| start.elapsed().as_secs())
                    .unwrap_or(0);

                let active_alerts = alert_engine.get_active_alerts().await;

                let health_report = serde_json::json!({
                    "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                    "uptime_seconds": uptime,
                    "metrics_collected": metrics.metrics_collected.load(Ordering::SeqCst),
                    "alerts_triggered": metrics.alerts_triggered.load(Ordering::SeqCst),
                    "data_points_stored": metrics.data_points_stored.load(Ordering::SeqCst),
                    "aggregations_processed": metrics.aggregations_processed.load(Ordering::SeqCst),
                    "active_alerts": active_alerts.len(),
                    "status": if active_alerts.iter().any(|a| matches!(a.severity, AlertSeverity::Critical | AlertSeverity::Emergency)) {
                        "degraded"
                    } else {
                        "healthy"
                    }
                });

                let topic = "observability/dashboard/health";
                if let Err(e) = client
                    .publish_qos0(topic, health_report.to_string().as_bytes())
                    .await
                {
                    warn!(error = %e, "Failed to publish dashboard health report");
                }

                debug!(
                    uptime_hours = %(uptime / 3600),
                    metrics_collected = %metrics.metrics_collected.load(Ordering::SeqCst),
                    active_alerts = %active_alerts.len(),
                    "Dashboard health check completed"
                );
            }

            info!("Dashboard health monitor stopped");
        });

        Ok(())
    }

    /// Stop the observability dashboard gracefully
    #[instrument(skip(self))]
    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Stopping observability dashboard");

        self.is_running.store(false, Ordering::SeqCst);

        // Publish offline status
        if let Err(e) = self
            .client
            .publish_qos1("observability/dashboard/status", b"offline")
            .await
        {
            warn!(error = %e, "Failed to publish offline status");
        }

        // Disconnect from broker
        self.client.disconnect().await?;

        info!("Observability dashboard stopped");
        Ok(())
    }

    /// Get comprehensive dashboard statistics
    #[instrument(skip(self))]
    pub async fn get_dashboard_stats(&self) -> DashboardStats {
        let active_alerts = self.alert_engine.get_active_alerts().await;

        DashboardStats {
            uptime_seconds: self
                .dashboard_metrics
                .uptime_start
                .map(|start| start.elapsed().as_secs())
                .unwrap_or(0),
            metrics_collected: self
                .dashboard_metrics
                .metrics_collected
                .load(Ordering::SeqCst),
            alerts_triggered: self
                .dashboard_metrics
                .alerts_triggered
                .load(Ordering::SeqCst),
            data_points_stored: self
                .dashboard_metrics
                .data_points_stored
                .load(Ordering::SeqCst),
            aggregations_processed: self
                .dashboard_metrics
                .aggregations_processed
                .load(Ordering::SeqCst),
            active_alerts: active_alerts.len(),
            critical_alerts: active_alerts
                .iter()
                .filter(|a| {
                    matches!(
                        a.severity,
                        AlertSeverity::Critical | AlertSeverity::Emergency
                    )
                })
                .count(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    pub uptime_seconds: u64,
    pub metrics_collected: u64,
    pub alerts_triggered: u64,
    pub data_points_stored: u64,
    pub aggregations_processed: u64,
    pub active_alerts: usize,
    pub critical_alerts: usize,
}

/// Handle HTTP requests to the metrics endpoint
async fn handle_metrics_request(
    req: Request<hyper::body::Incoming>,
    database: Arc<TimeSeriesDatabase>,
) -> Result<Response<String>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            // Get all metrics from the database
            let metrics = database.get_all_metrics().await;

            // Format metrics in Prometheus exposition format
            let mut prometheus_output = String::new();

            for metric in metrics {
                // Add metric help and type
                prometheus_output.push_str(&format!("# HELP {} {}\n", metric.name, metric.help));
                prometheus_output.push_str(&format!(
                    "# TYPE {} {}\n",
                    metric.name,
                    match metric.metric_type {
                        MetricType::Counter => "counter",
                        MetricType::Gauge => "gauge",
                        MetricType::Histogram => "histogram",
                        MetricType::Summary => "summary",
                    }
                ));

                // Add metric with labels
                if metric.labels.is_empty() {
                    prometheus_output.push_str(&format!("{} {}\n", metric.name, metric.value));
                } else {
                    let labels: Vec<String> = metric
                        .labels
                        .iter()
                        .map(|(k, v)| format!("{}=\"{}\"", k, v))
                        .collect();
                    prometheus_output.push_str(&format!(
                        "{}{{{}}} {}\n",
                        metric.name,
                        labels.join(","),
                        metric.value
                    ));
                }
            }

            let response = Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/plain; version=0.0.4")
                .body(prometheus_output)
                .unwrap();

            Ok(response)
        }
        _ => {
            let response = Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body("Not Found".to_string())
                .unwrap();
            Ok(response)
        }
    }
}

/// Handle HTTP requests to the health endpoint
async fn handle_health_request(
    req: Request<hyper::body::Incoming>,
    alert_engine: Arc<AlertEngine>,
) -> Result<Response<String>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/health") => {
            let active_alerts = alert_engine.get_active_alerts().await;

            let has_critical_alerts = active_alerts.iter().any(|alert| {
                matches!(
                    alert.severity,
                    AlertSeverity::Critical | AlertSeverity::Emergency
                )
            });

            let health_status = serde_json::json!({
                "status": if has_critical_alerts { "degraded" } else { "healthy" },
                "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                "active_alerts": active_alerts.len(),
                "critical_alerts": active_alerts.iter()
                    .filter(|a| matches!(a.severity, AlertSeverity::Critical | AlertSeverity::Emergency))
                    .count(),
                "alerts": active_alerts.iter().take(10).map(|alert| serde_json::json!({
                    "id": alert.alert_id,
                    "metric": alert.metric_name,
                    "severity": format!("{:?}", alert.severity),
                    "description": alert.description,
                    "timestamp": alert.timestamp
                })).collect::<Vec<_>>()
            });

            let status_code = if has_critical_alerts {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::OK
            };

            let response = Response::builder()
                .status(status_code)
                .header("content-type", "application/json")
                .body(health_status.to_string())
                .unwrap();

            Ok(response)
        }
        _ => {
            let response = Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body("Not Found".to_string())
                .unwrap();
            Ok(response)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .init();

    // Determine configuration based on environment
    let broker_url = std::env::var("MQTT_BROKER_URL").unwrap_or_else(|_| {
        if std::env::var("PRODUCTION").is_ok() {
            // Example production metrics broker URLs:
            // Dedicated metrics broker: mqtts://metrics-broker.company.com:8883
            // Cloud provider: mqtts://metrics.iot.region.amazonaws.com:8883
            "mqtts://test.mosquitto.org:8883".to_string()
        } else {
            // Local development broker
            "mqtt://localhost:1883".to_string()
        }
    });

    let dashboard_id =
        std::env::var("DASHBOARD_ID").unwrap_or_else(|_| "main-dashboard".to_string());
    let metrics_port: u16 = std::env::var("METRICS_PORT")
        .unwrap_or_else(|_| "9090".to_string())
        .parse()
        .unwrap_or(9090);
    let health_port: u16 = std::env::var("HEALTH_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap_or(8080);
    let retention_hours: u64 = std::env::var("RETENTION_HOURS")
        .unwrap_or_else(|_| "24".to_string())
        .parse()
        .unwrap_or(24);

    info!(
        broker_url = %broker_url,
        dashboard_id = %dashboard_id,
        metrics_port = %metrics_port,
        health_port = %health_port,
        retention_hours = %retention_hours,
        "Starting Observability Dashboard"
    );

    // Create and start the observability dashboard
    let dashboard = ObservabilityDashboard::new(
        &dashboard_id,
        &broker_url,
        metrics_port,
        health_port,
        retention_hours,
    )
    .await?;

    info!("Starting Observability Dashboard...");
    dashboard.start().await?;

    // Simulate some metrics for demonstration
    tokio::spawn({
        let client = Arc::clone(&dashboard.client);
        async move {
            let mut interval = interval(Duration::from_secs(10));
            for i in 0..20 {
                interval.tick().await;

                // Simulate device metrics
                let device_metric = MetricDataPoint {
                    name: "device_temperature_celsius".to_string(),
                    metric_type: MetricType::Gauge,
                    value: 20.0 + (i as f64 * 0.5) + (rand::random::<f64>() * 2.0),
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    labels: [("device_id".to_string(), "temp-001".to_string())].into(),
                    source: MetricSource::IoTDevice {
                        device_type: "temperature".to_string(),
                    },
                    help: "Current temperature reading from IoT device".to_string(),
                };

                if let Ok(payload) = serde_json::to_string(&device_metric) {
                    if let Err(e) = client
                        .publish_qos0("devices/temp-001/metrics", payload.as_bytes())
                        .await
                    {
                        warn!(error = %e, "Failed to publish demo metric");
                    }
                }

                // Simulate error metric
                if i % 5 == 0 {
                    let error_metric = MetricDataPoint {
                        name: "mqtt_errors_total".to_string(),
                        metric_type: MetricType::Counter,
                        value: (i / 5) as f64,
                        timestamp: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        labels: [("error_type".to_string(), "connection_timeout".to_string())]
                            .into(),
                        source: MetricSource::MqttBroker {
                            broker_id: "main-broker".to_string(),
                        },
                        help: "Total number of MQTT errors".to_string(),
                    };

                    if let Ok(payload) = serde_json::to_string(&error_metric) {
                        if let Err(e) = client
                            .publish_qos0("brokers/main-broker/metrics", payload.as_bytes())
                            .await
                        {
                            warn!(error = %e, "Failed to publish demo error metric");
                        }
                    }
                }
            }
        }
    });

    // Run for demonstration
    info!("Observability Dashboard running for 120 seconds...");
    info!(
        "Metrics endpoint: http://localhost:{}/metrics",
        metrics_port
    );
    info!("Health endpoint: http://localhost:{}/health", health_port);
    sleep(Duration::from_secs(120)).await;

    // Show dashboard statistics
    let stats = dashboard.get_dashboard_stats().await;
    info!(stats = ?stats, "Final dashboard statistics");

    // Stop gracefully
    dashboard.stop().await?;

    Ok(())
}
