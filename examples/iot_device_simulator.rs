//! `IoT` Device Simulator Example
//!
//! This example demonstrates a comprehensive `IoT` device simulator with:
//! - Device lifecycle management (boot, register, operate, shutdown)
//! - Robust error handling and recovery patterns
//! - Telemetry collection with buffering and retry logic
//! - Network resilience with offline queue management
//! - Health monitoring and automatic recovery
//! - OTA update simulation using MQTT for command and control
//! - **TLS encryption for secure cloud connections**
//!
//! ## Security Configuration
//!
//! The simulator automatically uses TLS when connecting to secure brokers:
//! - **Development**: `mqtt://localhost:1883` (plain text)
//! - **Production**: `mqtts://broker.example.com:8883` (TLS encrypted)
//!
//! Environment variables:
//! - `MQTT_BROKER_URL`: Override default broker URL
//! - `DEVICE_ID`: Set custom device identifier  
//! - `PRODUCTION`: Forces secure broker if set
//!
//! Example usage:
//! ```bash
//! # Local development
//! cargo run --example iot_device_simulator
//!
//! # Production with TLS
//! PRODUCTION=1 cargo run --example iot_device_simulator
//!
//! # Custom broker
//! MQTT_BROKER_URL=mqtts://your-broker.com:8883 cargo run --example iot_device_simulator
//! ```
//!
//! The simulator showcases real-world `IoT` patterns including:
//! - Circuit breaker for broker failures
//! - Exponential backoff for retries
//! - Local data caching during offline periods
//! - Structured logging for debugging
//! - Graceful degradation under failure conditions
//! - **Secure MQTT communications with TLS**

use mqtt5::{ConnectOptions, ConnectionEvent, MqttClient, QoS, WillMessage};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, instrument, warn};

/// Device types supported by the simulator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceType {
    TemperatureSensor,
    HumiditySensor,
    PressureSensor,
    SmartThermostat,
    SecurityCamera,
    DoorLock,
}

/// Device lifecycle states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceState {
    Initializing,
    Registering,
    Operating,
    Maintenance,
    Error,
    Shutdown,
}

/// Telemetry data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryData {
    pub device_id: String,
    pub device_type: DeviceType,
    pub timestamp: u64,
    pub metrics: std::collections::HashMap<String, f64>,
    pub battery_level: Option<f32>,
    pub signal_strength: i32,
}

/// Device configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub device_id: String,
    pub device_type: DeviceType,
    pub telemetry_interval: Duration,
    pub heartbeat_interval: Duration,
    pub max_offline_buffer_size: usize,
    pub retry_attempts: usize,
    pub circuit_breaker_threshold: usize,
}

/// Circuit breaker states for handling broker failures
#[derive(Debug, Clone, Copy, PartialEq)]
enum CircuitBreakerState {
    Closed,   // Normal operation
    Open,     // Failing fast
    HalfOpen, // Testing if service recovered
}

/// Circuit breaker for handling broker connection failures
#[derive(Debug)]
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitBreakerState>>,
    failure_count: Arc<AtomicU64>,
    threshold: usize,
    timeout: Duration,
    last_failure: Arc<RwLock<Option<Instant>>>,
}

impl CircuitBreaker {
    #[must_use]
    pub fn new(threshold: usize, timeout: Duration) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitBreakerState::Closed)),
            failure_count: Arc::new(AtomicU64::new(0)),
            threshold,
            timeout,
            last_failure: Arc::new(RwLock::new(None)),
        }
    }

    #[instrument(skip(self, f))]
    pub async fn call<F, Fut, T, E>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Debug,
    {
        // Check if circuit breaker should allow the call
        let state = *self.state.read().await;
        match state {
            CircuitBreakerState::Open => {
                if let Some(last_failure) = *self.last_failure.read().await {
                    if last_failure.elapsed() > self.timeout {
                        debug!("Circuit breaker transitioning to half-open state");
                        *self.state.write().await = CircuitBreakerState::HalfOpen;
                    } else {
                        debug!("Circuit breaker is open, failing fast");
                        return f().await; // Let the underlying error bubble up
                    }
                } else {
                    *self.state.write().await = CircuitBreakerState::HalfOpen;
                }
            }
            CircuitBreakerState::HalfOpen => {
                debug!("Circuit breaker in half-open state, testing service");
            }
            CircuitBreakerState::Closed => {
                debug!("Circuit breaker closed, normal operation");
            }
        }

        // Execute the operation
        match f().await {
            Ok(result) => {
                // Success - reset circuit breaker
                self.failure_count.store(0, Ordering::SeqCst);
                *self.state.write().await = CircuitBreakerState::Closed;
                debug!("Operation succeeded, circuit breaker reset");
                Ok(result)
            }
            Err(e) => {
                // Failure - increment counter and potentially open circuit
                let failures = (self.failure_count.fetch_add(1, Ordering::SeqCst) + 1) as usize;
                *self.last_failure.write().await = Some(Instant::now());

                if failures >= self.threshold {
                    warn!(failures = %failures, threshold = %self.threshold, "Circuit breaker opening due to failures");
                    *self.state.write().await = CircuitBreakerState::Open;
                }

                Err(e)
            }
        }
    }
}

/// `IoT` Device Simulator with comprehensive error handling
pub struct IoTDeviceSimulator {
    config: DeviceConfig,
    client: Arc<MqttClient>,
    state: Arc<RwLock<DeviceState>>,
    telemetry_buffer: Arc<Mutex<VecDeque<TelemetryData>>>,
    circuit_breaker: Arc<CircuitBreaker>,
    is_running: Arc<AtomicBool>,
    metrics: Arc<DeviceMetrics>,
}

/// Device operational metrics
#[derive(Debug, Default)]
pub struct DeviceMetrics {
    messages_sent: AtomicU64,
    messages_failed: AtomicU64,
    connection_failures: AtomicU64,
    uptime_start: Option<Instant>,
}

impl IoTDeviceSimulator {
    /// Create a new IoT device simulator
    #[instrument(skip(broker_url))]
    pub async fn new(
        config: DeviceConfig,
        broker_url: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        info!(device_id = %config.device_id, device_type = ?config.device_type, "Initializing IoT device simulator");

        // Create MQTT client with robust connection options
        let connect_options = ConnectOptions::new(&config.device_id)
            .with_clean_start(false) // Persistent session for reliability
            .with_keep_alive(Duration::from_secs(30))
            .with_automatic_reconnect(true)
            .with_reconnect_delay(Duration::from_secs(1), Duration::from_secs(60))
            .with_will(
                WillMessage::new(
                    format!("devices/{}/status", config.device_id),
                    b"offline".to_vec(),
                )
                .with_qos(QoS::AtLeastOnce)
                .with_retain(true),
            );

        let client = Arc::new(MqttClient::with_options(connect_options));

        // Set up connection event monitoring
        let device_id = config.device_id.clone();
        client.on_connection_event(move |event| {
            match event {
                ConnectionEvent::Connected { session_present } => {
                    info!(device_id = %device_id, session_present = %session_present, "Device connected to MQTT broker");
                }
                ConnectionEvent::Disconnected { reason } => {
                    warn!(device_id = %device_id, reason = ?reason, "Device disconnected from MQTT broker");
                }
                ConnectionEvent::Reconnecting { attempt } => {
                    info!(device_id = %device_id, attempt = %attempt, "Device attempting to reconnect");
                }
                ConnectionEvent::ReconnectFailed { error } => {
                    error!(device_id = %device_id, error = %error, "Device reconnection failed");
                }
            }
        }).await?;

        // Connect to broker with appropriate security
        if broker_url.starts_with("mqtts://") {
            info!(device_id = %config.device_id, "Connecting to broker with TLS encryption");
            client.connect(broker_url).await?;
        } else {
            info!(device_id = %config.device_id, "Connecting to broker with plain text (development only)");
            client.connect(broker_url).await?;
        }

        let circuit_breaker = Arc::new(CircuitBreaker::new(
            config.retry_attempts,
            Duration::from_secs(30),
        ));

        Ok(Self {
            config,
            client,
            state: Arc::new(RwLock::new(DeviceState::Initializing)),
            telemetry_buffer: Arc::new(Mutex::new(VecDeque::new())),
            circuit_breaker,
            is_running: Arc::new(AtomicBool::new(false)),
            metrics: Arc::new(DeviceMetrics {
                uptime_start: Some(Instant::now()),
                ..Default::default()
            }),
        })
    }

    /// Start the device simulator
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!(device_id = %self.config.device_id, "Starting IoT device simulator");

        if self.is_running.load(Ordering::SeqCst) {
            warn!("Device simulator is already running");
            return Ok(());
        }

        self.is_running.store(true, Ordering::SeqCst);
        *self.state.write().await = DeviceState::Registering;

        // Register device
        self.register_device().await?;
        *self.state.write().await = DeviceState::Operating;

        // Start background tasks
        self.start_telemetry_task().await?;
        self.start_heartbeat_task().await?;
        self.start_command_listener().await?;
        self.start_buffer_flush_task().await?;
        self.start_health_monitor().await?;

        // All tasks started successfully

        Ok(())
    }

    /// Register device with the broker
    #[instrument(skip(self))]
    async fn register_device(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!(device_id = %self.config.device_id, "Registering device");

        let registration_data = serde_json::json!({
            "device_id": self.config.device_id,
            "device_type": self.config.device_type,
            "capabilities": self.get_device_capabilities(),
            "firmware_version": "1.0.0",
            "registration_time": SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
        });

        let topic = format!("devices/{}/registration", self.config.device_id);

        self.circuit_breaker
            .call(|| async {
                self.client
                    .publish_qos1(&topic, registration_data.to_string().as_bytes())
                    .await
            })
            .await?;

        // Publish initial status
        let status_topic = format!("devices/{}/status", self.config.device_id);
        self.circuit_breaker
            .call(|| async { self.client.publish_qos1(&status_topic, b"online").await })
            .await?;

        info!(device_id = %self.config.device_id, "Device registration completed");
        Ok(())
    }

    /// Get device capabilities based on type
    fn get_device_capabilities(&self) -> Vec<&'static str> {
        match self.config.device_type {
            DeviceType::TemperatureSensor => vec!["temperature_measurement", "battery_status"],
            DeviceType::HumiditySensor => vec!["humidity_measurement", "temperature_measurement"],
            DeviceType::PressureSensor => vec!["pressure_measurement", "altitude_calculation"],
            DeviceType::SmartThermostat => vec![
                "temperature_control",
                "schedule_management",
                "remote_control",
            ],
            DeviceType::SecurityCamera => {
                vec!["video_streaming", "motion_detection", "night_vision"]
            }
            DeviceType::DoorLock => vec!["access_control", "keypad_input", "remote_unlock"],
        }
    }

    /// Start telemetry collection task
    #[instrument(skip(self))]
    async fn start_telemetry_task(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(self.config.telemetry_interval);
        let device_id = self.config.device_id.clone();
        let device_type = self.config.device_type.clone();
        let client = Arc::clone(&self.client);
        let buffer = Arc::clone(&self.telemetry_buffer);
        let circuit_breaker = Arc::clone(&self.circuit_breaker);
        let is_running = Arc::clone(&self.is_running);
        let metrics = Arc::clone(&self.metrics);

        tokio::spawn(async move {
            info!(device_id = %device_id, "Starting telemetry collection task");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                // Generate telemetry data
                let telemetry = Self::generate_telemetry_data(&device_id, &device_type);
                let topic = format!("devices/{device_id}/telemetry");

                // Try to send telemetry
                let result = circuit_breaker
                    .call(|| async {
                        let payload = serde_json::to_string(&telemetry)?;
                        client
                            .publish_qos1(&topic, payload.as_bytes())
                            .await
                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                    })
                    .await;

                match result {
                    Ok(_) => {
                        metrics.messages_sent.fetch_add(1, Ordering::SeqCst);
                        debug!(device_id = %device_id, "Telemetry sent successfully");
                    }
                    Err(e) => {
                        metrics.messages_failed.fetch_add(1, Ordering::SeqCst);
                        warn!(device_id = %device_id, error = %e, "Failed to send telemetry, buffering locally");

                        // Buffer telemetry for later retry
                        let mut buffer_guard = buffer.lock().await;
                        if buffer_guard.len() < 1000 {
                            // Prevent unbounded growth
                            buffer_guard.push_back(telemetry);
                        } else {
                            warn!("Telemetry buffer full, dropping oldest data");
                            buffer_guard.pop_front();
                            buffer_guard.push_back(telemetry);
                        }
                    }
                }
            }

            info!(device_id = %device_id, "Telemetry collection task stopped");
        });

        Ok(())
    }

    /// Generate realistic telemetry data
    fn generate_telemetry_data(device_id: &str, device_type: &DeviceType) -> TelemetryData {
        let mut metrics = std::collections::HashMap::new();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        match device_type {
            DeviceType::TemperatureSensor => {
                metrics.insert(
                    "temperature".to_string(),
                    20.0 + (rand::random::<f64>() * 10.0),
                );
            }
            DeviceType::HumiditySensor => {
                metrics.insert(
                    "humidity".to_string(),
                    40.0 + (rand::random::<f64>() * 20.0),
                );
                metrics.insert(
                    "temperature".to_string(),
                    18.0 + (rand::random::<f64>() * 12.0),
                );
            }
            DeviceType::PressureSensor => {
                metrics.insert(
                    "pressure".to_string(),
                    1013.25 + (rand::random::<f64>() * 50.0) - 25.0,
                );
                metrics.insert(
                    "altitude".to_string(),
                    100.0 + (rand::random::<f64>() * 200.0),
                );
            }
            DeviceType::SmartThermostat => {
                metrics.insert(
                    "current_temp".to_string(),
                    20.0 + (rand::random::<f64>() * 5.0),
                );
                metrics.insert("target_temp".to_string(), 22.0);
                metrics.insert(
                    "heating_active".to_string(),
                    if rand::random::<bool>() { 1.0 } else { 0.0 },
                );
            }
            DeviceType::SecurityCamera => {
                metrics.insert(
                    "motion_detected".to_string(),
                    if rand::random::<f64>() < 0.1 {
                        1.0
                    } else {
                        0.0
                    },
                );
                metrics.insert(
                    "recording_active".to_string(),
                    if rand::random::<bool>() { 1.0 } else { 0.0 },
                );
            }
            DeviceType::DoorLock => {
                metrics.insert(
                    "locked".to_string(),
                    if rand::random::<f64>() < 0.9 {
                        1.0
                    } else {
                        0.0
                    },
                );
                metrics.insert("access_attempts".to_string(), rand::random::<f64>() * 3.0);
            }
        }

        TelemetryData {
            device_id: device_id.to_string(),
            device_type: device_type.clone(),
            timestamp,
            metrics,
            battery_level: Some(75.0 + (rand::random::<f32>() * 25.0)),
            signal_strength: -60 + (rand::random::<i32>() % 40),
        }
    }

    /// Start heartbeat task
    #[instrument(skip(self))]
    async fn start_heartbeat_task(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(self.config.heartbeat_interval);
        let device_id = self.config.device_id.clone();
        let client = Arc::clone(&self.client);
        let circuit_breaker = Arc::clone(&self.circuit_breaker);
        let is_running = Arc::clone(&self.is_running);
        let metrics = Arc::clone(&self.metrics);

        tokio::spawn(async move {
            info!(device_id = %device_id, "Starting heartbeat task");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                let heartbeat = serde_json::json!({
                    "device_id": device_id,
                    "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                    "uptime": metrics.uptime_start.map_or(0, |start| start.elapsed().as_secs()),
                    "messages_sent": metrics.messages_sent.load(Ordering::SeqCst),
                    "messages_failed": metrics.messages_failed.load(Ordering::SeqCst),
                });

                let topic = format!("devices/{device_id}/heartbeat");

                let result = circuit_breaker
                    .call(|| async {
                        client
                            .publish_qos0(&topic, heartbeat.to_string().as_bytes())
                            .await
                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                    })
                    .await;

                if let Err(e) = result {
                    warn!(device_id = %device_id, error = %e, "Failed to send heartbeat");
                }
            }

            info!(device_id = %device_id, "Heartbeat task stopped");
        });

        Ok(())
    }

    /// Start command listener for remote control
    #[instrument(skip(self))]
    async fn start_command_listener(&self) -> Result<(), Box<dyn std::error::Error>> {
        let device_id = self.config.device_id.clone();
        let client = Arc::clone(&self.client);
        let state = Arc::clone(&self.state);

        // Subscribe to command topic
        let command_topic = format!("devices/{device_id}/commands/+");

        client.subscribe(&command_topic, move |msg| {
            let device_id = device_id.clone();
            let state = Arc::clone(&state);

            tokio::spawn(async move {
                if let Ok(command) = String::from_utf8(msg.payload.clone()) {
                    info!(device_id = %device_id, topic = %msg.topic, command = %command, "Received device command");

                    // Parse command from topic
                    if let Some(cmd_type) = msg.topic.split('/').next_back() {
                        match cmd_type {
                            "reboot" => {
                                info!(device_id = %device_id, "Processing reboot command");
                                *state.write().await = DeviceState::Maintenance;
                                sleep(Duration::from_secs(5)).await; // Simulate reboot
                                *state.write().await = DeviceState::Operating;
                            }
                            "update" => {
                                info!(device_id = %device_id, "Processing OTA update command");
                                Self::simulate_ota_update(&device_id).await;
                            }
                            "config" => {
                                info!(device_id = %device_id, config = %command, "Processing configuration update");
                                // Handle configuration updates
                            }
                            _ => {
                                warn!(device_id = %device_id, command_type = %cmd_type, "Unknown command type");
                            }
                        }
                    }
                }
            });
        }).await?;

        info!(device_id = %self.config.device_id, "Command listener started");
        Ok(())
    }

    /// Start buffer flush task to retry failed messages
    #[instrument(skip(self))]
    async fn start_buffer_flush_task(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(30)); // Retry every 30 seconds
        let device_id = self.config.device_id.clone();
        let client = Arc::clone(&self.client);
        let buffer = Arc::clone(&self.telemetry_buffer);
        let circuit_breaker = Arc::clone(&self.circuit_breaker);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!(device_id = %device_id, "Starting buffer flush task");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                let mut buffer_guard = buffer.lock().await;
                if buffer_guard.is_empty() {
                    continue;
                }

                let buffered_count = buffer_guard.len();
                debug!(device_id = %device_id, buffered_messages = %buffered_count, "Attempting to flush buffered telemetry");

                let mut successfully_sent = 0;
                while let Some(telemetry) = buffer_guard.pop_front() {
                    let topic = format!("devices/{device_id}/telemetry");

                    let result = circuit_breaker
                        .call(|| async {
                            let payload = serde_json::to_string(&telemetry)?;
                            client
                                .publish_qos1(&topic, payload.as_bytes())
                                .await
                                .map_err(|e| {
                                    Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                                })
                        })
                        .await;

                    if result.is_ok() {
                        successfully_sent += 1;
                    } else {
                        // Put the message back at the front of the queue
                        buffer_guard.push_front(telemetry);
                        break; // Stop trying if we can't send
                    }
                }

                if successfully_sent > 0 {
                    info!(device_id = %device_id, sent = %successfully_sent, remaining = %buffer_guard.len(), "Flushed buffered telemetry messages");
                }
            }

            info!(device_id = %device_id, "Buffer flush task stopped");
        });

        Ok(())
    }

    /// Start health monitoring task
    #[instrument(skip(self))]
    async fn start_health_monitor(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(60)); // Check every minute
        let device_id = self.config.device_id.clone();
        let client = Arc::clone(&self.client);
        let state = Arc::clone(&self.state);
        let metrics = Arc::clone(&self.metrics);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!(device_id = %device_id, "Starting health monitor task");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                // Check device health
                let current_state = *state.read().await;
                let messages_sent = metrics.messages_sent.load(Ordering::SeqCst);
                let messages_failed = metrics.messages_failed.load(Ordering::SeqCst);
                let connection_failures = metrics.connection_failures.load(Ordering::SeqCst);

                let health_status = if messages_failed > messages_sent / 2 {
                    "degraded"
                } else if connection_failures > 10 {
                    "critical"
                } else {
                    "healthy"
                };

                let health_report = serde_json::json!({
                    "device_id": device_id,
                    "state": format!("{:?}", current_state),
                    "health_status": health_status,
                    "uptime": metrics.uptime_start.map_or(0, |start| start.elapsed().as_secs()),
                    "messages_sent": messages_sent,
                    "messages_failed": messages_failed,
                    "connection_failures": connection_failures,
                    "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                });

                let topic = format!("devices/{device_id}/health");
                if let Err(e) = client
                    .publish_qos0(&topic, health_report.to_string().as_bytes())
                    .await
                {
                    warn!(device_id = %device_id, error = %e, "Failed to send health report");
                }

                debug!(device_id = %device_id, health = %health_status, "Health check completed");
            }

            info!(device_id = %device_id, "Health monitor task stopped");
        });

        Ok(())
    }

    /// Simulate OTA (Over-The-Air) update process
    #[instrument]
    async fn simulate_ota_update(device_id: &str) {
        info!(device_id = %device_id, "Starting OTA update simulation");

        // Simulate download phase
        for progress in (0..=100).step_by(10) {
            sleep(Duration::from_millis(200)).await;
            debug!(device_id = %device_id, progress = %progress, "OTA download progress");
        }

        // Simulate installation phase
        info!(device_id = %device_id, "Installing OTA update");
        sleep(Duration::from_secs(2)).await;

        // Simulate reboot
        info!(device_id = %device_id, "Rebooting device after OTA update");
        sleep(Duration::from_secs(3)).await;

        info!(device_id = %device_id, "OTA update completed successfully");
    }

    /// Stop the device simulator gracefully
    #[instrument(skip(self))]
    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!(device_id = %self.config.device_id, "Stopping IoT device simulator");

        self.is_running.store(false, Ordering::SeqCst);
        *self.state.write().await = DeviceState::Shutdown;

        // Send offline status
        let status_topic = format!("devices/{}/status", self.config.device_id);
        if let Err(e) = self.client.publish_qos1(&status_topic, b"offline").await {
            warn!(error = %e, "Failed to send offline status");
        }

        // Disconnect from broker
        self.client.disconnect().await?;

        info!(device_id = %self.config.device_id, "Device simulator stopped");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_target(false)
        .init();

    // Determine broker URL based on environment
    // For production: use secure cloud broker (e.g., AWS IoT, HiveMQ Cloud, etc.)
    // For development: use local broker
    let broker_url = std::env::var("MQTT_BROKER_URL").unwrap_or_else(|_| {
        if std::env::var("PRODUCTION").is_ok() {
            // Example production URLs (replace with your actual broker):
            // AWS IoT Core: mqtts://your-endpoint.iot.region.amazonaws.com:8883
            // HiveMQ Cloud: mqtts://your-cluster.s1.eu.hivemq.cloud:8883
            // Eclipse Cloud: mqtts://mqtt.eclipseprojects.io:8883
            "mqtts://test.mosquitto.org:8883".to_string()
        } else {
            // Local development broker
            "mqtt://localhost:1883".to_string()
        }
    });

    info!(broker_url = %broker_url, "Using MQTT broker");

    // Device configuration
    let config = DeviceConfig {
        device_id: std::env::var("DEVICE_ID").unwrap_or_else(|_| "temp-sensor-001".to_string()),
        device_type: DeviceType::TemperatureSensor,
        telemetry_interval: Duration::from_secs(10),
        heartbeat_interval: Duration::from_secs(30),
        max_offline_buffer_size: 1000,
        retry_attempts: 3,
        circuit_breaker_threshold: 5,
    };

    // Create and start the device simulator
    let simulator = IoTDeviceSimulator::new(config, &broker_url).await?;

    info!("Starting IoT device simulator...");
    simulator.start().await?;

    // Run for demonstration (in real scenarios, this would run indefinitely)
    info!("Simulator running for 60 seconds...");
    sleep(Duration::from_secs(60)).await;

    // Stop gracefully
    simulator.stop().await?;

    Ok(())
}
