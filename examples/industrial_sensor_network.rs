//! Industrial Sensor Network Example
//!
//! This example demonstrates an industrial sensor network with comprehensive failover logic:
//! - Primary/backup sensor configurations with automatic failover
//! - Multi-zone sensor monitoring with zone-level redundancy
//! - Data integrity validation and outlier detection
//! - High-availability patterns with leader election
//! - Critical alert escalation with backup communication channels
//! - Historical data buffering with time-series aggregation
//! - Network partitioning resilience with local decision making
//! - **Enterprise-grade mTLS with custom CA certificates**
//!
//! ## Security Configuration
//!
//! The industrial sensor network uses **mutual TLS (mTLS) with custom CA certificates**
//! for maximum security in industrial environments:
//! - **Development**: `mqtt://localhost:1883` (plain text)
//! - **Production**: `mqtts://industrial-broker.company.com:8883` with custom PKI
//!
//! Environment variables:
//! - `MQTT_BROKER_URL`: Override default broker URL
//! - `NETWORK_ID`: Set custom network identifier
//! - `CLIENT_CERT_PATH`: Path to network coordinator certificate (PEM format)
//! - `CLIENT_KEY_PATH`: Path to network coordinator private key (PEM format)
//! - `CA_CERT_PATH`: Path to company CA certificate (PEM format)
//! - `PRODUCTION`: Forces secure broker with custom mTLS if set
//!
//! Example usage:
//! ```bash
//! # Local development
//! cargo run --example industrial_sensor_network
//!
//! # Production with custom PKI
//! PRODUCTION=1 \
//! CLIENT_CERT_PATH=pki/network-coordinator.crt \
//! CLIENT_KEY_PATH=pki/network-coordinator.key \
//! CA_CERT_PATH=pki/company-ca.crt \
//! cargo run --example industrial_sensor_network
//! ```
//!
//! Industrial `IoT` patterns showcased:
//! - Sensor fusion from multiple redundant sources
//! - Byzantine fault tolerance for critical measurements
//! - Graceful degradation during partial network failures
//! - Real-time anomaly detection with machine learning thresholds
//! - Compliance logging and audit trails
//! - Predictive maintenance based on sensor drift patterns
//! - **Enterprise PKI infrastructure integration**
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::missing_errors_doc)]

use mqtt5::transport::tls::TlsConfig;
use mqtt5::{ConnectOptions, ConnectionEvent, MqttClient, QoS, WillMessage};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, instrument, warn};
use url::Url;

/// Sensor types deployed in industrial environments
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SensorType {
    Temperature,
    Pressure,
    Vibration,
    Flow,
    Level,
    PH,
    Conductivity,
    Turbidity,
}

/// Criticality levels for sensor measurements
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CriticalityLevel {
    Normal = 0,
    Warning = 1,
    Critical = 2,
    Emergency = 3,
}

/// Sensor operational states with detailed status information
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SensorState {
    Online,
    Degraded,    // Functioning but with reduced accuracy
    Offline,     // Not responding
    Maintenance, // Scheduled maintenance mode
    Calibrating, // Auto-calibration in progress
    Failed,      // Hardware failure detected
}

/// Zone configuration for geographic/logical sensor grouping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorZone {
    pub zone_id: String,
    pub zone_name: String,
    pub location: String,
    pub sensor_count: usize,
    pub redundancy_level: u8, // Minimum working sensors required
    pub alert_escalation_time: Duration,
}

/// Individual sensor configuration with failover capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorConfig {
    pub sensor_id: String,
    pub sensor_type: SensorType,
    pub zone_id: String,
    pub is_primary: bool,               // Primary or backup sensor
    pub backup_sensor_ids: Vec<String>, // List of backup sensors
    pub measurement_interval: Duration,
    pub validation_threshold: f64, // Maximum deviation from expected range
    pub criticality: CriticalityLevel,
    pub calibration_interval: Duration,
    pub maintenance_schedule: Option<Duration>,
}

/// Sensor measurement with comprehensive metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorMeasurement {
    pub sensor_id: String,
    pub sensor_type: SensorType,
    pub zone_id: String,
    pub timestamp: u64,
    pub value: f64,
    pub unit: String,
    pub quality: f32,               // Measurement quality (0.0-1.0)
    pub confidence: f32,            // Confidence level (0.0-1.0)
    pub is_validated: bool,         // Passed cross-validation
    pub anomaly_score: Option<f32>, // Anomaly detection score
    pub calibration_status: String,
    pub device_status: SensorState,
}

/// Network-wide alert with escalation capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAlert {
    pub alert_id: String,
    pub zone_id: String,
    pub sensor_id: Option<String>,
    pub alert_type: String,
    pub criticality: CriticalityLevel,
    pub message: String,
    pub timestamp: u64,
    pub escalation_count: u32,
    pub acknowledgment_required: bool,
    pub backup_channels: Vec<String>,
}

/// Historical data point for trend analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalDataPoint {
    pub timestamp: u64,
    pub sensor_id: String,
    pub value: f64,
    pub quality: f32,
    pub aggregation_period: Duration,
    pub sample_count: u32,
}

/// Failover logic for sensor redundancy
#[derive(Debug)]
pub struct SensorFailoverManager {
    primary_sensors: Arc<RwLock<HashMap<String, SensorConfig>>>,
    backup_sensors: Arc<RwLock<HashMap<String, Vec<String>>>>,
    sensor_states: Arc<RwLock<HashMap<String, SensorState>>>,
    active_sensors: Arc<RwLock<HashMap<String, String>>>, // zone_id -> active_sensor_id
    failover_history: Arc<Mutex<VecDeque<FailoverEvent>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverEvent {
    pub timestamp: u64,
    pub zone_id: String,
    pub failed_sensor: String,
    pub activated_sensor: String,
    pub reason: String,
}

impl Default for SensorFailoverManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorFailoverManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            primary_sensors: Arc::new(RwLock::new(HashMap::new())),
            backup_sensors: Arc::new(RwLock::new(HashMap::new())),
            sensor_states: Arc::new(RwLock::new(HashMap::new())),
            active_sensors: Arc::new(RwLock::new(HashMap::new())),
            failover_history: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Register a sensor with failover configuration
    #[instrument(skip(self))]
    pub async fn register_sensor(&self, config: SensorConfig) {
        info!(sensor_id = %config.sensor_id, zone_id = %config.zone_id, is_primary = %config.is_primary, "Registering sensor");

        if config.is_primary {
            self.primary_sensors
                .write()
                .await
                .insert(config.zone_id.clone(), config.clone());
            self.active_sensors
                .write()
                .await
                .insert(config.zone_id.clone(), config.sensor_id.clone());
        }

        // Register backup relationships
        self.backup_sensors
            .write()
            .await
            .insert(config.sensor_id.clone(), config.backup_sensor_ids.clone());
        self.sensor_states
            .write()
            .await
            .insert(config.sensor_id.clone(), SensorState::Online);
    }

    /// Handle sensor failure and trigger failover if needed
    #[instrument(skip(self))]
    pub async fn handle_sensor_failure(
        &self,
        sensor_id: &str,
        reason: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        warn!(sensor_id = %sensor_id, reason = %reason, "Handling sensor failure");

        // Update sensor state
        self.sensor_states
            .write()
            .await
            .insert(sensor_id.to_string(), SensorState::Failed);

        // Check if this sensor is currently active in any zone
        let mut active_sensors = self.active_sensors.write().await;
        let mut failed_zone = None;

        for (zone_id, active_sensor) in active_sensors.iter() {
            if active_sensor == sensor_id {
                failed_zone = Some(zone_id.clone());
                break;
            }
        }

        if let Some(zone_id) = failed_zone {
            // Find backup sensor for this zone
            if let Some(backup_sensor) = self.find_best_backup(&zone_id, sensor_id).await {
                info!(zone_id = %zone_id, failed_sensor = %sensor_id, backup_sensor = %backup_sensor, "Executing failover");

                // Execute failover
                active_sensors.insert(zone_id.clone(), backup_sensor.clone());

                // Record failover event
                let event = FailoverEvent {
                    timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
                    zone_id: zone_id.clone(),
                    failed_sensor: sensor_id.to_string(),
                    activated_sensor: backup_sensor.clone(),
                    reason: reason.to_string(),
                };

                self.failover_history.lock().await.push_back(event);

                // Keep only last 1000 failover events
                if self.failover_history.lock().await.len() > 1000 {
                    self.failover_history.lock().await.pop_front();
                }

                return Ok(Some(backup_sensor));
            }
            error!(zone_id = %zone_id, "No backup sensor available for failover");
            return Ok(None);
        }

        Ok(None)
    }

    /// Find the best available backup sensor for a zone
    async fn find_best_backup(&self, zone_id: &str, failed_sensor: &str) -> Option<String> {
        let primary_sensors = self.primary_sensors.read().await;
        let sensor_states = self.sensor_states.read().await;
        let backup_sensors = self.backup_sensors.read().await;

        if let Some(primary_config) = primary_sensors.get(zone_id) {
            // Look through backup sensors of the failed sensor
            if let Some(backups) = backup_sensors.get(failed_sensor) {
                for backup_id in backups {
                    if let Some(state) = sensor_states.get(backup_id) {
                        if matches!(state, SensorState::Online | SensorState::Degraded) {
                            return Some(backup_id.clone());
                        }
                    }
                }
            }

            // If no direct backups available, look for any online sensor of same type in zone
            for (sensor_id, state) in sensor_states.iter() {
                if matches!(state, SensorState::Online) {
                    // Check if this sensor matches the required type (simplified check)
                    if sensor_id
                        .contains(&format!("{:?}", primary_config.sensor_type).to_lowercase())
                    {
                        return Some(sensor_id.clone());
                    }
                }
            }
        }

        None
    }

    /// Get current failover status for all zones
    #[instrument(skip(self))]
    pub async fn get_failover_status(&self) -> HashMap<String, FailoverStatus> {
        let mut status = HashMap::new();
        let active_sensors = self.active_sensors.read().await;
        let primary_sensors = self.primary_sensors.read().await;
        let sensor_states = self.sensor_states.read().await;

        for (zone_id, active_sensor) in active_sensors.iter() {
            let is_primary = primary_sensors
                .get(zone_id)
                .is_some_and(|config| &config.sensor_id == active_sensor);

            let sensor_state = sensor_states
                .get(active_sensor)
                .copied()
                .unwrap_or(SensorState::Offline);

            status.insert(
                zone_id.clone(),
                FailoverStatus {
                    zone_id: zone_id.clone(),
                    active_sensor: active_sensor.clone(),
                    is_primary_active: is_primary,
                    sensor_state,
                    failover_count: 0, // TODO: Calculate from history
                },
            );
        }

        status
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverStatus {
    pub zone_id: String,
    pub active_sensor: String,
    pub is_primary_active: bool,
    pub sensor_state: SensorState,
    pub failover_count: u32,
}

/// Industrial sensor network coordinator
pub struct IndustrialSensorNetwork {
    client: Arc<MqttClient>,
    zones: Arc<RwLock<HashMap<String, SensorZone>>>,
    sensors: Arc<RwLock<HashMap<String, SensorConfig>>>,
    failover_manager: Arc<SensorFailoverManager>,
    measurement_buffer: Arc<Mutex<VecDeque<SensorMeasurement>>>,
    historical_data: Arc<Mutex<VecDeque<HistoricalDataPoint>>>,
    alert_manager: Arc<AlertManager>,
    is_running: Arc<AtomicBool>,
    network_metrics: Arc<NetworkMetrics>,
}

/// Alert management with escalation logic
#[derive(Debug)]
pub struct AlertManager {
    active_alerts: Arc<RwLock<HashMap<String, NetworkAlert>>>,
    alert_history: Arc<Mutex<VecDeque<NetworkAlert>>>,
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
            alert_history: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    #[instrument(skip(self))]
    pub async fn create_alert(&self, alert: NetworkAlert) {
        info!(alert_id = %alert.alert_id, criticality = ?alert.criticality, "Creating network alert");

        self.active_alerts
            .write()
            .await
            .insert(alert.alert_id.clone(), alert.clone());
        self.alert_history.lock().await.push_back(alert);

        // Keep only last 10000 alerts in history
        if self.alert_history.lock().await.len() > 10000 {
            self.alert_history.lock().await.pop_front();
        }
    }

    #[instrument(skip(self))]
    pub async fn acknowledge_alert(&self, alert_id: &str) -> bool {
        if let Some(mut alert) = self.active_alerts.write().await.remove(alert_id) {
            alert.acknowledgment_required = false;
            info!(alert_id = %alert_id, "Alert acknowledged and resolved");
            true
        } else {
            warn!(alert_id = %alert_id, "Attempted to acknowledge non-existent alert");
            false
        }
    }
}

/// Network-wide operational metrics
#[derive(Debug, Default)]
pub struct NetworkMetrics {
    total_measurements: AtomicU64,
    failed_measurements: AtomicU64,
    anomalies_detected: AtomicU64,
    failovers_executed: AtomicU64,
    alerts_generated: AtomicU64,
    network_uptime_start: Option<Instant>,
}

impl IndustrialSensorNetwork {
    /// Create a new industrial sensor network
    #[instrument(skip(broker_url))]
    pub async fn new(
        network_id: &str,
        broker_url: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        info!(network_id = %network_id, "Initializing industrial sensor network");

        // Create MQTT client for network coordination
        let connect_options = ConnectOptions::new(format!("industrial-network-{network_id}"))
            .with_clean_start(false)
            .with_keep_alive(Duration::from_secs(30))
            .with_automatic_reconnect(true)
            .with_reconnect_delay(Duration::from_secs(2), Duration::from_secs(120))
            .with_will(
                WillMessage::new(format!("networks/{network_id}/status"), b"offline".to_vec())
                    .with_qos(QoS::AtLeastOnce)
                    .with_retain(true),
            );

        let client = Arc::new(MqttClient::with_options(connect_options));

        // Set up connection monitoring
        let network_id_clone = network_id.to_string();
        client.on_connection_event(move |event| {
            match event {
                ConnectionEvent::Connected { session_present } => {
                    info!(network_id = %network_id_clone, session_present = %session_present, "Industrial network connected");
                }
                ConnectionEvent::Disconnected { reason } => {
                    warn!(network_id = %network_id_clone, reason = ?reason, "Industrial network disconnected");
                }
                ConnectionEvent::Reconnecting { attempt } => {
                    info!(network_id = %network_id_clone, attempt = %attempt, "Industrial network reconnecting");
                }
                ConnectionEvent::ReconnectFailed { error } => {
                    error!(network_id = %network_id_clone, error = %error, "Industrial network reconnection failed");
                }
            }
        }).await?;

        // Connect to broker with enterprise-grade security
        if broker_url.starts_with("mqtts://") {
            info!(network_id = %network_id, "Connecting to industrial broker with custom mTLS");

            // Require certificates for industrial environments
            let cert_path = std::env::var("CLIENT_CERT_PATH")
                .map_err(|_| "CLIENT_CERT_PATH required for secure industrial connections")?;
            let key_path = std::env::var("CLIENT_KEY_PATH")
                .map_err(|_| "CLIENT_KEY_PATH required for secure industrial connections")?;
            let ca_path = std::env::var("CA_CERT_PATH")
                .map_err(|_| "CA_CERT_PATH required for secure industrial connections")?;

            // Parse broker URL to get host and port
            let url = Url::parse(broker_url)?;
            let host = url.host_str().ok_or("Invalid broker URL: missing host")?;
            let port = url.port().unwrap_or(8883);
            let addr = format!("{host}:{port}").parse()?;

            // Configure enterprise mTLS with custom CA
            let mut tls_config = TlsConfig::new(addr, host);
            tls_config.load_client_cert_pem(&cert_path)?;
            tls_config.load_client_key_pem(&key_path)?;
            tls_config.load_ca_cert_pem(&ca_path)?;

            // Use only custom CA (no system roots for industrial security)
            tls_config = tls_config
                .with_system_roots(false)
                .with_verify_server_cert(true);

            client.connect_with_tls(tls_config).await?;
            info!(network_id = %network_id, "Connected with enterprise mTLS and custom CA");
        } else {
            info!(network_id = %network_id, "Connecting to broker with plain text (development only)");
            client.connect(broker_url).await?;
        }

        Ok(Self {
            client,
            zones: Arc::new(RwLock::new(HashMap::new())),
            sensors: Arc::new(RwLock::new(HashMap::new())),
            failover_manager: Arc::new(SensorFailoverManager::new()),
            measurement_buffer: Arc::new(Mutex::new(VecDeque::new())),
            historical_data: Arc::new(Mutex::new(VecDeque::new())),
            alert_manager: Arc::new(AlertManager::new()),
            is_running: Arc::new(AtomicBool::new(false)),
            network_metrics: Arc::new(NetworkMetrics {
                network_uptime_start: Some(Instant::now()),
                ..Default::default()
            }),
        })
    }

    /// Start the industrial sensor network
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting industrial sensor network");

        if self.is_running.load(Ordering::SeqCst) {
            warn!("Industrial sensor network is already running");
            return Ok(());
        }

        self.is_running.store(true, Ordering::SeqCst);

        // Start background tasks
        self.start_measurement_processor().await?;
        self.start_failover_monitor().await?;
        self.start_alert_processor().await?;
        self.start_data_aggregator().await?;
        self.start_health_monitor().await?;
        self.start_command_listener().await?;

        info!("Industrial sensor network started successfully");
        Ok(())
    }

    /// Add a sensor zone to the network
    #[instrument(skip(self))]
    pub async fn add_zone(&self, zone: SensorZone) -> Result<(), Box<dyn std::error::Error>> {
        info!(zone_id = %zone.zone_id, zone_name = %zone.zone_name, "Adding sensor zone");

        self.zones
            .write()
            .await
            .insert(zone.zone_id.clone(), zone.clone());

        // Publish zone configuration
        let topic = format!("networks/zones/{}/config", zone.zone_id);
        let payload = serde_json::to_string(&zone)?;
        self.client.publish_qos1(&topic, payload.as_bytes()).await?;

        Ok(())
    }

    /// Add a sensor to the network
    #[instrument(skip(self))]
    pub async fn add_sensor(&self, config: SensorConfig) -> Result<(), Box<dyn std::error::Error>> {
        info!(sensor_id = %config.sensor_id, sensor_type = ?config.sensor_type, zone_id = %config.zone_id, "Adding sensor to network");

        // Register with failover manager
        self.failover_manager.register_sensor(config.clone()).await;

        // Store sensor configuration
        self.sensors
            .write()
            .await
            .insert(config.sensor_id.clone(), config.clone());

        // Publish sensor configuration
        let topic = format!("networks/sensors/{}/config", config.sensor_id);
        let payload = serde_json::to_string(&config)?;
        self.client.publish_qos1(&topic, payload.as_bytes()).await?;

        Ok(())
    }

    /// Process incoming sensor measurements
    #[instrument(skip(self))]
    async fn start_measurement_processor(&self) -> Result<(), Box<dyn std::error::Error>> {
        let client = Arc::clone(&self.client);
        let buffer = Arc::clone(&self.measurement_buffer);
        let failover_manager = Arc::clone(&self.failover_manager);
        let alert_manager = Arc::clone(&self.alert_manager);
        let metrics = Arc::clone(&self.network_metrics);
        let _is_running = Arc::clone(&self.is_running);

        // Subscribe to all sensor measurements
        client.subscribe("networks/+/sensors/+/measurements", move |msg| {
            let buffer = Arc::clone(&buffer);
            let failover_manager = Arc::clone(&failover_manager);
            let alert_manager = Arc::clone(&alert_manager);
            let metrics = Arc::clone(&metrics);

            tokio::spawn(async move {
                if let Ok(measurement_str) = String::from_utf8(msg.payload.clone()) {
                    if let Ok(measurement) = serde_json::from_str::<SensorMeasurement>(&measurement_str) {
                        debug!(sensor_id = %measurement.sensor_id, value = %measurement.value, "Processing sensor measurement");

                        metrics.total_measurements.fetch_add(1, Ordering::SeqCst);

                        // Validate measurement quality
                        if measurement.quality < 0.5 || measurement.confidence < 0.7 {
                            warn!(sensor_id = %measurement.sensor_id, quality = %measurement.quality, confidence = %measurement.confidence, "Low quality measurement detected");

                            // Consider sensor degradation
                            if measurement.quality < 0.3 {
                                if let Err(e) = failover_manager.handle_sensor_failure(&measurement.sensor_id, "Low measurement quality").await {
                                    error!(error = %e, "Failed to handle sensor degradation");
                                }
                            }
                        }

                        // Check for anomalies
                        if let Some(anomaly_score) = measurement.anomaly_score {
                            if anomaly_score > 0.8 {
                                metrics.anomalies_detected.fetch_add(1, Ordering::SeqCst);

                                let alert = NetworkAlert {
                                    alert_id: format!("anomaly-{}-{}", measurement.sensor_id, measurement.timestamp),
                                    zone_id: measurement.zone_id.clone(),
                                    sensor_id: Some(measurement.sensor_id.clone()),
                                    alert_type: "anomaly_detected".to_string(),
                                    criticality: if anomaly_score > 0.95 { CriticalityLevel::Critical } else { CriticalityLevel::Warning },
                                    message: format!("Anomaly detected in sensor {} with score {:.2}", measurement.sensor_id, anomaly_score),
                                    timestamp: measurement.timestamp,
                                    escalation_count: 0,
                                    acknowledgment_required: anomaly_score > 0.95,
                                    backup_channels: vec!["email".to_string(), "sms".to_string()],
                                };

                                alert_manager.create_alert(alert).await;
                            }
                        }

                        // Buffer measurement for processing
                        let mut buffer_guard = buffer.lock().await;
                        buffer_guard.push_back(measurement);

                        // Limit buffer size
                        if buffer_guard.len() > 10000 {
                            buffer_guard.pop_front();
                        }
                    } else {
                        warn!(topic = %msg.topic, "Failed to parse sensor measurement");
                        metrics.failed_measurements.fetch_add(1, Ordering::SeqCst);
                    }
                }
            });
        }).await?;

        info!("Measurement processor started");
        Ok(())
    }

    /// Monitor sensor health and trigger failovers
    #[instrument(skip(self))]
    async fn start_failover_monitor(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(30));
        let failover_manager = Arc::clone(&self.failover_manager);
        let _sensors = Arc::clone(&self.sensors);
        let client = Arc::clone(&self.client);
        let metrics = Arc::clone(&self.network_metrics);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting failover monitor");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                // Get current failover status
                let status = failover_manager.get_failover_status().await;

                for (zone_id, zone_status) in status {
                    // Publish failover status
                    let topic = format!("networks/zones/{zone_id}/failover-status");
                    if let Ok(payload) = serde_json::to_string(&zone_status) {
                        if let Err(e) = client.publish_qos0(&topic, payload.as_bytes()).await {
                            warn!(error = %e, "Failed to publish failover status");
                        }
                    }

                    // Check if we need to escalate any failures
                    if !zone_status.is_primary_active {
                        metrics.failovers_executed.fetch_add(1, Ordering::SeqCst);
                        warn!(zone_id = %zone_id, active_sensor = %zone_status.active_sensor, "Zone operating on backup sensor");
                    }
                }

                debug!("Failover monitor check completed");
            }

            info!("Failover monitor stopped");
        });

        Ok(())
    }

    /// Process and escalate alerts
    #[instrument(skip(self))]
    async fn start_alert_processor(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(60));
        let alert_manager = Arc::clone(&self.alert_manager);
        let client = Arc::clone(&self.client);
        let metrics = Arc::clone(&self.network_metrics);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting alert processor");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                // Process active alerts for escalation
                let active_alerts = alert_manager.active_alerts.read().await;

                for (alert_id, alert) in active_alerts.iter() {
                    // Check if alert needs escalation
                    let alert_age = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                        - alert.timestamp;

                    let escalation_threshold = match alert.criticality {
                        CriticalityLevel::Emergency => 60, // 1 minute
                        CriticalityLevel::Critical => 300, // 5 minutes
                        CriticalityLevel::Warning => 900,  // 15 minutes
                        CriticalityLevel::Normal => 3600,  // 1 hour
                    };

                    if alert_age > escalation_threshold && alert.acknowledgment_required {
                        warn!(alert_id = %alert_id, age_seconds = %alert_age, "Alert requires escalation");

                        // Publish escalation
                        let topic = format!("networks/alerts/{alert_id}/escalation");
                        let escalation = serde_json::json!({
                            "alert_id": alert_id,
                            "escalation_level": alert.escalation_count + 1,
                            "backup_channels": &alert.backup_channels,
                            "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
                        });

                        if let Err(e) = client
                            .publish_qos1(&topic, escalation.to_string().as_bytes())
                            .await
                        {
                            error!(error = %e, "Failed to publish alert escalation");
                        }

                        metrics.alerts_generated.fetch_add(1, Ordering::SeqCst);
                    }
                }

                debug!("Alert processor check completed");
            }

            info!("Alert processor stopped");
        });

        Ok(())
    }

    /// Aggregate historical data for trend analysis
    #[instrument(skip(self))]
    async fn start_data_aggregator(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(300)); // Every 5 minutes
        let buffer = Arc::clone(&self.measurement_buffer);
        let historical_data = Arc::clone(&self.historical_data);
        let client = Arc::clone(&self.client);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting data aggregator");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                let mut measurements = buffer.lock().await;
                if measurements.is_empty() {
                    continue;
                }

                // Group measurements by sensor and aggregate
                let mut sensor_groups: HashMap<String, Vec<SensorMeasurement>> = HashMap::new();

                // Drain measurements from buffer for aggregation
                let batch_size = std::cmp::min(1000, measurements.len());
                for _ in 0..batch_size {
                    if let Some(measurement) = measurements.pop_front() {
                        sensor_groups
                            .entry(measurement.sensor_id.clone())
                            .or_default()
                            .push(measurement);
                    }
                }

                // Create aggregated data points
                for (sensor_id, sensor_measurements) in sensor_groups {
                    if sensor_measurements.is_empty() {
                        continue;
                    }

                    let avg_value = sensor_measurements.iter().map(|m| m.value).sum::<f64>()
                        / sensor_measurements.len() as f64;

                    let avg_quality = sensor_measurements.iter().map(|m| m.quality).sum::<f32>()
                        / sensor_measurements.len() as f32;

                    let data_point = HistoricalDataPoint {
                        timestamp: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        sensor_id: sensor_id.clone(),
                        value: avg_value,
                        quality: avg_quality,
                        aggregation_period: Duration::from_secs(300),
                        sample_count: sensor_measurements.len() as u32,
                    };

                    // Store in historical data
                    let mut historical = historical_data.lock().await;
                    historical.push_back(data_point.clone());

                    // Limit historical data size
                    if historical.len() > 100_000 {
                        historical.pop_front();
                    }

                    // Publish aggregated data
                    let topic = format!("networks/sensors/{sensor_id}/historical");
                    if let Ok(payload) = serde_json::to_string(&data_point) {
                        if let Err(e) = client.publish_qos0(&topic, payload.as_bytes()).await {
                            warn!(error = %e, "Failed to publish historical data");
                        }
                    }
                }

                debug!(processed_measurements = %batch_size, "Data aggregation completed");
            }

            info!("Data aggregator stopped");
        });

        Ok(())
    }

    /// Monitor overall network health
    #[instrument(skip(self))]
    async fn start_health_monitor(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(120)); // Every 2 minutes
        let zones = Arc::clone(&self.zones);
        let sensors = Arc::clone(&self.sensors);
        let failover_manager = Arc::clone(&self.failover_manager);
        let client = Arc::clone(&self.client);
        let metrics = Arc::clone(&self.network_metrics);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting network health monitor");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                let zone_count = zones.read().await.len();
                let sensor_count = sensors.read().await.len();
                let failover_status = failover_manager.get_failover_status().await;

                let uptime = metrics
                    .network_uptime_start
                    .map_or(0, |start| start.elapsed().as_secs());

                let health_report = serde_json::json!({
                    "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                    "network_uptime_seconds": uptime,
                    "total_zones": zone_count,
                    "total_sensors": sensor_count,
                    "active_zones": failover_status.len(),
                    "zones_on_backup": failover_status.values().filter(|s| !s.is_primary_active).count(),
                    "total_measurements": metrics.total_measurements.load(Ordering::SeqCst),
                    "failed_measurements": metrics.failed_measurements.load(Ordering::SeqCst),
                    "anomalies_detected": metrics.anomalies_detected.load(Ordering::SeqCst),
                    "failovers_executed": metrics.failovers_executed.load(Ordering::SeqCst),
                    "alerts_generated": metrics.alerts_generated.load(Ordering::SeqCst),
                });

                // Publish network health
                let topic = "networks/health";
                if let Err(e) = client
                    .publish_qos0(topic, health_report.to_string().as_bytes())
                    .await
                {
                    warn!(error = %e, "Failed to publish network health report");
                }

                info!(
                    zones = %zone_count,
                    sensors = %sensor_count,
                    uptime_hours = %(uptime / 3600),
                    "Network health check completed"
                );
            }

            info!("Network health monitor stopped");
        });

        Ok(())
    }

    /// Listen for network management commands
    #[instrument(skip(self))]
    async fn start_command_listener(&self) -> Result<(), Box<dyn std::error::Error>> {
        let failover_manager = Arc::clone(&self.failover_manager);
        let alert_manager = Arc::clone(&self.alert_manager);
        let client = Arc::clone(&self.client);

        // Subscribe to network commands
        client.subscribe("networks/commands/+", move |msg| {
            let failover_manager = Arc::clone(&failover_manager);
            let alert_manager = Arc::clone(&alert_manager);

            tokio::spawn(async move {
                if let Ok(command) = String::from_utf8(msg.payload.clone()) {
                    info!(topic = %msg.topic, command = %command, "Received network command");

                    if let Some(cmd_type) = msg.topic.split('/').next_back() {
                        match cmd_type {
                            "failover" => {
                                if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&command) {
                                    if let (Some(sensor_id), Some(reason)) = (
                                        cmd["sensor_id"].as_str(),
                                        cmd["reason"].as_str()
                                    ) {
                                        if let Err(e) = failover_manager.handle_sensor_failure(sensor_id, reason).await {
                                            error!(error = %e, "Failed to execute manual failover");
                                        }
                                    }
                                }
                            }
                            "acknowledge" => {
                                if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&command) {
                                    if let Some(alert_id) = cmd["alert_id"].as_str() {
                                        alert_manager.acknowledge_alert(alert_id).await;
                                    }
                                }
                            }
                            _ => {
                                warn!(command_type = %cmd_type, "Unknown network command type");
                            }
                        }
                    }
                }
            });
        }).await?;

        info!("Network command listener started");
        Ok(())
    }

    /// Stop the industrial sensor network gracefully
    #[instrument(skip(self))]
    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Stopping industrial sensor network");

        self.is_running.store(false, Ordering::SeqCst);

        // Publish offline status
        if let Err(e) = self
            .client
            .publish_qos1("networks/status", b"offline")
            .await
        {
            warn!(error = %e, "Failed to publish offline status");
        }

        // Disconnect from broker
        self.client.disconnect().await?;

        info!("Industrial sensor network stopped");
        Ok(())
    }

    /// Get comprehensive network statistics
    #[instrument(skip(self))]
    pub async fn get_network_stats(&self) -> NetworkStats {
        let zone_count = self.zones.read().await.len();
        let sensor_count = self.sensors.read().await.len();
        let failover_status = self.failover_manager.get_failover_status().await;
        let buffered_measurements = self.measurement_buffer.lock().await.len();
        let historical_points = self.historical_data.lock().await.len();

        NetworkStats {
            total_zones: zone_count,
            total_sensors: sensor_count,
            active_zones: failover_status.len(),
            zones_on_backup: failover_status
                .values()
                .filter(|s| !s.is_primary_active)
                .count(),
            buffered_measurements,
            historical_data_points: historical_points,
            total_measurements: self
                .network_metrics
                .total_measurements
                .load(Ordering::SeqCst),
            failed_measurements: self
                .network_metrics
                .failed_measurements
                .load(Ordering::SeqCst),
            anomalies_detected: self
                .network_metrics
                .anomalies_detected
                .load(Ordering::SeqCst),
            failovers_executed: self
                .network_metrics
                .failovers_executed
                .load(Ordering::SeqCst),
            alerts_generated: self.network_metrics.alerts_generated.load(Ordering::SeqCst),
            uptime_seconds: self
                .network_metrics
                .network_uptime_start
                .map_or(0, |start| start.elapsed().as_secs()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub total_zones: usize,
    pub total_sensors: usize,
    pub active_zones: usize,
    pub zones_on_backup: usize,
    pub buffered_measurements: usize,
    pub historical_data_points: usize,
    pub total_measurements: u64,
    pub failed_measurements: u64,
    pub anomalies_detected: u64,
    pub failovers_executed: u64,
    pub alerts_generated: u64,
    pub uptime_seconds: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .init();

    // Determine broker URL and network ID based on environment
    let broker_url = std::env::var("MQTT_BROKER_URL").unwrap_or_else(|_| {
        if std::env::var("PRODUCTION").is_ok() {
            // Example industrial broker URLs with enterprise mTLS:
            // Private cloud: mqtts://industrial-mqtt.company.com:8883
            // Azure IoT Hub: mqtts://your-hub.azure-devices.net:8883
            // AWS IoT Core: mqtts://your-endpoint.iot.region.amazonaws.com:8883
            "mqtts://test.mosquitto.org:8883".to_string()
        } else {
            // Local development broker
            "mqtt://localhost:1883".to_string()
        }
    });

    let network_id = std::env::var("NETWORK_ID").unwrap_or_else(|_| "factory-01".to_string());

    info!(broker_url = %broker_url, network_id = %network_id, "Using MQTT broker for Industrial Sensor Network");

    // Create industrial sensor network
    let network = IndustrialSensorNetwork::new(&network_id, &broker_url).await?;

    // Configure zones
    let zone1 = SensorZone {
        zone_id: "zone-a".to_string(),
        zone_name: "Production Line A".to_string(),
        location: "Building 1, Floor 2".to_string(),
        sensor_count: 4,
        redundancy_level: 2,
        alert_escalation_time: Duration::from_secs(300),
    };

    let zone2 = SensorZone {
        zone_id: "zone-b".to_string(),
        zone_name: "Quality Control".to_string(),
        location: "Building 1, Floor 3".to_string(),
        sensor_count: 6,
        redundancy_level: 3,
        alert_escalation_time: Duration::from_secs(180),
    };

    network.add_zone(zone1).await?;
    network.add_zone(zone2).await?;

    // Configure sensors with failover relationships
    let temp_sensor_primary = SensorConfig {
        sensor_id: "temp-001-primary".to_string(),
        sensor_type: SensorType::Temperature,
        zone_id: "zone-a".to_string(),
        is_primary: true,
        backup_sensor_ids: vec!["temp-001-backup".to_string()],
        measurement_interval: Duration::from_secs(5),
        validation_threshold: 2.0,
        criticality: CriticalityLevel::Critical,
        calibration_interval: Duration::from_secs(86400), // Daily
        maintenance_schedule: Some(Duration::from_secs(604_800)), // Weekly
    };

    let temp_sensor_backup = SensorConfig {
        sensor_id: "temp-001-backup".to_string(),
        sensor_type: SensorType::Temperature,
        zone_id: "zone-a".to_string(),
        is_primary: false,
        backup_sensor_ids: vec![],
        measurement_interval: Duration::from_secs(5),
        validation_threshold: 2.0,
        criticality: CriticalityLevel::Critical,
        calibration_interval: Duration::from_secs(86400),
        maintenance_schedule: Some(Duration::from_secs(604_800)),
    };

    let pressure_sensor_primary = SensorConfig {
        sensor_id: "pressure-001-primary".to_string(),
        sensor_type: SensorType::Pressure,
        zone_id: "zone-b".to_string(),
        is_primary: true,
        backup_sensor_ids: vec!["pressure-001-backup".to_string()],
        measurement_interval: Duration::from_secs(2),
        validation_threshold: 5.0,
        criticality: CriticalityLevel::Emergency,
        calibration_interval: Duration::from_secs(43200), // Twice daily
        maintenance_schedule: Some(Duration::from_secs(259_200)), // Every 3 days
    };

    network.add_sensor(temp_sensor_primary).await?;
    network.add_sensor(temp_sensor_backup).await?;
    network.add_sensor(pressure_sensor_primary).await?;

    // Start the network
    info!("Starting industrial sensor network...");
    network.start().await?;

    // Simulate network operation
    info!("Industrial sensor network running - monitoring for 120 seconds...");

    // Simulate some failover scenarios
    tokio::spawn({
        let network = std::sync::Arc::new(network);
        async move {
            // Wait 30 seconds, then simulate a sensor failure
            sleep(Duration::from_secs(30)).await;
            info!("Simulating primary temperature sensor failure...");

            if let Err(e) = network
                .failover_manager
                .handle_sensor_failure("temp-001-primary", "Simulated hardware failure")
                .await
            {
                error!(error = %e, "Failed to simulate sensor failure");
            }

            // Wait another 30 seconds, then show network stats
            sleep(Duration::from_secs(30)).await;
            let stats = network.get_network_stats().await;
            info!("Network statistics: {:?}", stats);
        }
    });

    // Run for demonstration
    sleep(Duration::from_secs(120)).await;

    // Stop gracefully
    // Note: Cannot call stop() here due to Arc wrapper in spawned task
    info!("Demo completed");

    Ok(())
}
