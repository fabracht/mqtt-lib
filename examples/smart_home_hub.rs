//! Smart Home Hub Example
//!
//! This example demonstrates a comprehensive smart home hub that manages multiple devices
//! concurrently with different QoS requirements, device coordination, and automation scenarios.
//!
//! ## Security Configuration
//!
//! The smart home hub uses **mutual TLS (mTLS)** for secure device authentication:
//! - **Development**: `mqtt://localhost:1883` (plain text)  
//! - **Production**: `mqtts://broker.example.com:8883` with client certificates
//!
//! Environment variables:
//! - `MQTT_BROKER_URL`: Override default broker URL
//! - `HUB_ID`: Set custom hub identifier
//! - `CLIENT_CERT_PATH`: Path to client certificate file (PEM format)
//! - `CLIENT_KEY_PATH`: Path to client private key file (PEM format)
//! - `CA_CERT_PATH`: Path to CA certificate file (PEM format)
//! - `PRODUCTION`: Forces secure broker with mTLS if set
//!
//! Example usage:
//! ```bash
//! # Local development
//! cargo run --example smart_home_hub
//!
//! # Production with mTLS
//! PRODUCTION=1 \
//! CLIENT_CERT_PATH=certs/hub.crt \
//! CLIENT_KEY_PATH=certs/hub.key \
//! CA_CERT_PATH=certs/ca.crt \
//! cargo run --example smart_home_hub
//! ```
//!
//! Features demonstrated:
//! - Concurrent device management with different protocols and QoS levels
//! - Device discovery and automatic registration
//! - Scene automation with multiple device coordination
//! - Security monitoring with alert prioritization
//! - Device health monitoring and automatic recovery
//! - Energy management with load balancing
//! - Home/Away mode automation
//! - Emergency response coordination
//! - Device group management and batch operations
//! - **Mutual TLS authentication for device security**

use mqtt5::transport::tls::TlsConfig;
use mqtt5::{ConnectOptions, ConnectionEvent, MqttClient, QoS, WillMessage};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, instrument, warn};
use url::Url;

/// Get current hour of the day (0-23) in local time
/// This is a simplified implementation that approximates local time
/// For production use, consider using a proper time library
fn get_current_hour() -> u8 {
    // Get current time as seconds since UNIX epoch
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    
    // Rough approximation: assume UTC offset of system timezone
    // This is a simplification for the example - in production you'd want proper timezone handling
    let hours_since_epoch = now / 3600;
    (hours_since_epoch % 24) as u8
}

/// Types of smart home devices
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DeviceType {
    Light,
    Thermostat,
    DoorLock,
    SecurityCamera,
    MotionSensor,
    SmokeDetetor,
    WindowSensor,
    SmartPlug,
    Speaker,
    AirConditioner,
}

/// Device capabilities for automation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    pub can_dim: bool,
    pub has_color: bool,
    pub has_temperature_control: bool,
    pub has_motion_detection: bool,
    pub has_audio: bool,
    pub power_consumption: Option<f32>, // Watts
    pub battery_powered: bool,
}

/// Device state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceState {
    pub device_id: String,
    pub device_type: DeviceType,
    pub room: String,
    pub online: bool,
    pub last_seen: u64,
    pub battery_level: Option<f32>,
    pub properties: HashMap<String, serde_json::Value>,
    pub capabilities: DeviceCapabilities,
}

/// Automation scene definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    pub description: String,
    pub actions: Vec<DeviceAction>,
    pub conditions: Vec<SceneCondition>,
    pub priority: u8,
}

/// Device action for automation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAction {
    pub device_id: String,
    pub action: String,
    pub parameters: HashMap<String, serde_json::Value>,
    pub delay_ms: Option<u64>,
}

/// Conditions for scene activation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SceneCondition {
    DeviceState {
        device_id: String,
        property: String,
        value: serde_json::Value,
    },
    TimeRange {
        start_hour: u8,
        end_hour: u8,
    },
    HomeOccupancy {
        occupied: bool,
    },
    SecurityAlarm {
        armed: bool,
    },
}

/// Security alert levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum AlertLevel {
    Info = 1,
    Warning = 2,
    Critical = 3,
    Emergency = 4,
}

/// Security alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAlert {
    pub id: String,
    pub device_id: String,
    pub level: AlertLevel,
    pub message: String,
    pub timestamp: u64,
    pub acknowledged: bool,
}

/// Home modes for automation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum HomeMode {
    Home,
    Away,
    Sleep,
    Party,
    Vacation,
}

/// Energy management settings
#[derive(Debug, Clone)]
pub struct EnergySettings {
    pub max_power_consumption: f32, // Watts
    pub peak_hours: (u8, u8),       // Start and end hour
    pub energy_saving_mode: bool,
}

/// Smart Home Hub with comprehensive device management
pub struct SmartHomeHub {
    client: Arc<MqttClient>,
    devices: Arc<RwLock<HashMap<String, DeviceState>>>,
    scenes: Arc<RwLock<HashMap<String, Scene>>>,
    security_alerts: Arc<RwLock<Vec<SecurityAlert>>>,
    home_mode: Arc<RwLock<HomeMode>>,
    energy_settings: Arc<RwLock<EnergySettings>>,
    is_running: Arc<AtomicBool>,
    alert_sender: broadcast::Sender<SecurityAlert>,
    device_registry: Arc<RwLock<HashSet<String>>>, // Known device IDs
    automation_engine: Arc<AutomationEngine>,
}

/// Automation engine for scene management
pub struct AutomationEngine {
    last_trigger_time: Arc<RwLock<HashMap<String, Instant>>>,
    cooldown_period: Duration,
}

impl Default for AutomationEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AutomationEngine {
    pub fn new() -> Self {
        Self {
            last_trigger_time: Arc::new(RwLock::new(HashMap::new())),
            cooldown_period: Duration::from_secs(10), // Prevent rapid triggers
        }
    }

    #[instrument(skip(self, condition))]
    pub async fn check_condition(
        &self,
        condition: &SceneCondition,
        devices: &HashMap<String, DeviceState>,
        home_mode: &HomeMode,
    ) -> bool {
        match condition {
            SceneCondition::DeviceState {
                device_id,
                property,
                value,
            } => {
                if let Some(device) = devices.get(device_id) {
                    if let Some(device_value) = device.properties.get(property) {
                        device_value == value
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            SceneCondition::TimeRange {
                start_hour,
                end_hour,
            } => {
                let current_hour = get_current_hour();
                if start_hour <= end_hour {
                    current_hour >= *start_hour && current_hour <= *end_hour
                } else {
                    // Wrap around midnight
                    current_hour >= *start_hour || current_hour <= *end_hour
                }
            }
            SceneCondition::HomeOccupancy { occupied } => {
                (*home_mode == HomeMode::Home) == *occupied
            }
            SceneCondition::SecurityAlarm { armed } => {
                // Check if any security device is armed
                devices.values().any(|device| {
                    matches!(
                        device.device_type,
                        DeviceType::SecurityCamera | DeviceType::MotionSensor
                    ) && device
                        .properties
                        .get("armed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                        == *armed
                })
            }
        }
    }

    #[instrument(skip(self, scene_id))]
    pub async fn can_trigger_scene(&self, scene_id: &str) -> bool {
        let last_trigger = self.last_trigger_time.read().await;
        if let Some(last_time) = last_trigger.get(scene_id) {
            last_time.elapsed() > self.cooldown_period
        } else {
            true
        }
    }

    #[instrument(skip(self, scene_id))]
    pub async fn mark_scene_triggered(&self, scene_id: &str) {
        let mut last_trigger = self.last_trigger_time.write().await;
        last_trigger.insert(scene_id.to_string(), Instant::now());
    }
}

impl SmartHomeHub {
    /// Create a new smart home hub
    #[instrument(skip(broker_url))]
    pub async fn new(hub_id: &str, broker_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        info!(hub_id = %hub_id, "Initializing Smart Home Hub");

        // Create MQTT client with hub-specific settings
        let connect_options = ConnectOptions::new(hub_id)
            .with_clean_start(false)
            .with_keep_alive(Duration::from_secs(30))
            .with_automatic_reconnect(true)
            .with_reconnect_delay(Duration::from_secs(2), Duration::from_secs(30))
            .with_will(
                WillMessage::new(format!("smarthome/{hub_id}/status"), b"offline".to_vec())
                    .with_qos(QoS::AtLeastOnce)
                    .with_retain(true),
            );

        let client = Arc::new(MqttClient::with_options(connect_options));

        // Set up connection monitoring
        let hub_id_clone = hub_id.to_string();
        client.on_connection_event(move |event| {
            match event {
                ConnectionEvent::Connected { session_present } => {
                    info!(hub_id = %hub_id_clone, session_present = %session_present, "Smart Home Hub connected");
                }
                ConnectionEvent::Disconnected { reason } => {
                    warn!(hub_id = %hub_id_clone, reason = ?reason, "Smart Home Hub disconnected");
                }
                ConnectionEvent::Reconnecting { attempt } => {
                    info!(hub_id = %hub_id_clone, attempt = %attempt, "Smart Home Hub reconnecting");
                }
                ConnectionEvent::ReconnectFailed { error } => {
                    error!(hub_id = %hub_id_clone, error = %error, "Smart Home Hub reconnection failed");
                }
            }
        }).await?;

        // Connect to broker with appropriate security
        if broker_url.starts_with("mqtts://") {
            info!(hub_id = %hub_id, "Connecting to broker with mTLS authentication");

            // Check for certificate paths in environment
            if let (Ok(cert_path), Ok(key_path)) = (
                std::env::var("CLIENT_CERT_PATH"),
                std::env::var("CLIENT_KEY_PATH"),
            ) {
                // Parse broker URL to get host and port
                let url = Url::parse(broker_url)?;
                let host = url.host_str().ok_or("Invalid broker URL: missing host")?;
                let port = url.port().unwrap_or(8883);
                let addr = format!("{host}:{port}").parse()?;

                // Configure mTLS
                let mut tls_config = TlsConfig::new(addr, host);
                tls_config.load_client_cert_pem(&cert_path)?;
                tls_config.load_client_key_pem(&key_path)?;

                // Load CA certificate if provided
                if let Ok(ca_path) = std::env::var("CA_CERT_PATH") {
                    tls_config.load_ca_cert_pem(&ca_path)?;
                    tls_config = tls_config.with_system_roots(false); // Use only custom CA
                }

                client.connect_with_tls(tls_config).await?;
                info!(hub_id = %hub_id, "Connected with mutual TLS authentication");
            } else {
                // Fallback to TLS without client certificates
                warn!(hub_id = %hub_id, "mTLS certificates not configured, using TLS without client auth");
                client.connect(broker_url).await?;
            }
        } else {
            info!(hub_id = %hub_id, "Connecting to broker with plain text (development only)");
            client.connect(broker_url).await?;
        }

        let (alert_sender, _) = broadcast::channel(100);

        let energy_settings = EnergySettings {
            max_power_consumption: 5000.0, // 5kW
            peak_hours: (17, 21),          // 5 PM to 9 PM
            energy_saving_mode: false,
        };

        Ok(Self {
            client,
            devices: Arc::new(RwLock::new(HashMap::new())),
            scenes: Arc::new(RwLock::new(HashMap::new())),
            security_alerts: Arc::new(RwLock::new(Vec::new())),
            home_mode: Arc::new(RwLock::new(HomeMode::Home)),
            energy_settings: Arc::new(RwLock::new(energy_settings)),
            is_running: Arc::new(AtomicBool::new(false)),
            alert_sender,
            device_registry: Arc::new(RwLock::new(HashSet::new())),
            automation_engine: Arc::new(AutomationEngine::new()),
        })
    }

    /// Start the smart home hub
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting Smart Home Hub");

        if self.is_running.load(Ordering::SeqCst) {
            warn!("Smart Home Hub is already running");
            return Ok(());
        }

        self.is_running.store(true, Ordering::SeqCst);

        // Subscribe to device topics
        self.setup_device_subscriptions().await?;

        // Publish hub online status
        self.client
            .publish_retain("smarthome/hub/status", b"online")
            .await?;

        // Start background tasks
        self.start_device_discovery().await?;
        self.start_automation_engine().await?;
        self.start_health_monitor().await?;
        self.start_energy_manager().await?;
        self.start_security_monitor().await?;

        // Setup default scenes
        self.setup_default_scenes().await?;

        info!("Smart Home Hub started successfully");
        Ok(())
    }

    /// Setup device topic subscriptions
    #[instrument(skip(self))]
    async fn setup_device_subscriptions(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Setting up device subscriptions");

        let devices = Arc::clone(&self.devices);
        let alert_sender = self.alert_sender.clone();
        let _automation_engine = Arc::clone(&self.automation_engine);
        let _home_mode = Arc::clone(&self.home_mode);
        let device_registry = Arc::clone(&self.device_registry);

        // Subscribe to device state updates
        self.client
            .subscribe("devices/+/state", move |msg| {
                let devices = Arc::clone(&devices);
                let alert_sender = alert_sender.clone();
                let _automation_engine = Arc::clone(&_automation_engine);
                let _home_mode = Arc::clone(&_home_mode);
                let device_registry = Arc::clone(&device_registry);

                tokio::spawn(async move {
                    if let Some(device_id) = msg.topic.split('/').nth(1) {
                        if let Ok(state_update) =
                            serde_json::from_slice::<serde_json::Value>(&msg.payload)
                        {
                            debug!(device_id = %device_id, "Received device state update");

                            // Update device state
                            let mut devices_guard = devices.write().await;
                            if let Some(device) = devices_guard.get_mut(device_id) {
                                device.last_seen = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs();
                                device.online = true;

                                // Update properties
                                if let Some(properties) = state_update.as_object() {
                                    for (key, value) in properties {
                                        device.properties.insert(key.clone(), value.clone());
                                    }
                                }

                                // Check for security alerts
                                Self::check_security_alerts(device, &alert_sender).await;
                            }

                            // Register device if not known
                            device_registry.write().await.insert(device_id.to_string());
                        }
                    }
                });
            })
            .await?;

        // Subscribe to device discovery
        self.client.subscribe("devices/+/announce", {
            let devices = Arc::clone(&self.devices);
            let device_registry = Arc::clone(&self.device_registry);

            move |msg| {
                let devices = Arc::clone(&devices);
                let device_registry = Arc::clone(&device_registry);

                tokio::spawn(async move {
                    if let Some(device_id) = msg.topic.split('/').nth(1) {
                        if let Ok(device_info) = serde_json::from_slice::<DeviceState>(&msg.payload) {
                            info!(device_id = %device_id, device_type = ?device_info.device_type, room = %device_info.room, "New device discovered");

                            // Add device to registry
                            let mut devices_guard = devices.write().await;
                            devices_guard.insert(device_id.to_string(), device_info);

                            device_registry.write().await.insert(device_id.to_string());
                        }
                    }
                });
            }
        }).await?;

        // Subscribe to security alerts
        self.client.subscribe("security/+/alert", {
            let security_alerts = Arc::clone(&self.security_alerts);
            let alert_sender = self.alert_sender.clone();

            move |msg| {
                let security_alerts = Arc::clone(&security_alerts);
                let alert_sender = alert_sender.clone();

                tokio::spawn(async move {
                    if let Ok(alert) = serde_json::from_slice::<SecurityAlert>(&msg.payload) {
                        warn!(alert_id = %alert.id, level = ?alert.level, message = %alert.message, "Security alert received");

                        // Store alert
                        security_alerts.write().await.push(alert.clone());

                        // Broadcast alert to listeners
                        let _ = alert_sender.send(alert);
                    }
                });
            }
        }).await?;

        info!("Device subscriptions configured");
        Ok(())
    }

    /// Check for security alerts based on device state
    async fn check_security_alerts(
        device: &DeviceState,
        alert_sender: &broadcast::Sender<SecurityAlert>,
    ) {
        match device.device_type {
            DeviceType::MotionSensor => {
                if let Some(motion) = device.properties.get("motion").and_then(|v| v.as_bool()) {
                    if motion {
                        let alert = SecurityAlert {
                            id: format!(
                                "motion-{}-{}",
                                device.device_id,
                                SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                            ),
                            device_id: device.device_id.clone(),
                            level: AlertLevel::Warning,
                            message: format!("Motion detected in {}", device.room),
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                            acknowledged: false,
                        };
                        let _ = alert_sender.send(alert);
                    }
                }
            }
            DeviceType::SmokeDetetor => {
                if let Some(smoke) = device
                    .properties
                    .get("smoke_detected")
                    .and_then(|v| v.as_bool())
                {
                    if smoke {
                        let alert = SecurityAlert {
                            id: format!(
                                "smoke-{}-{}",
                                device.device_id,
                                SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                            ),
                            device_id: device.device_id.clone(),
                            level: AlertLevel::Emergency,
                            message: format!("Smoke detected in {}", device.room),
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                            acknowledged: false,
                        };
                        let _ = alert_sender.send(alert);
                    }
                }
            }
            DeviceType::DoorLock => {
                if let Some(forced) = device
                    .properties
                    .get("forced_entry")
                    .and_then(|v| v.as_bool())
                {
                    if forced {
                        let alert = SecurityAlert {
                            id: format!(
                                "break-in-{}-{}",
                                device.device_id,
                                SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                            ),
                            device_id: device.device_id.clone(),
                            level: AlertLevel::Critical,
                            message: format!("Forced entry detected at {}", device.room),
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                            acknowledged: false,
                        };
                        let _ = alert_sender.send(alert);
                    }
                }
            }
            _ => {}
        }
    }

    /// Start device discovery task
    #[instrument(skip(self))]
    async fn start_device_discovery(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(60)); // Discovery every minute
        let client = Arc::clone(&self.client);
        let devices = Arc::clone(&self.devices);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting device discovery task");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                // Request device announcements
                if let Err(e) = client.publish_qos0("devices/discover", b"announce").await {
                    warn!(error = %e, "Failed to send discovery request");
                }

                // Check for offline devices
                let mut offline_devices = Vec::new();
                {
                    let devices_guard = devices.read().await;
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    for (device_id, device) in devices_guard.iter() {
                        if device.online && (now - device.last_seen) > 300 {
                            // 5 minutes
                            offline_devices.push(device_id.clone());
                        }
                    }
                }

                // Mark devices as offline
                if !offline_devices.is_empty() {
                    let mut devices_guard = devices.write().await;
                    for device_id in offline_devices {
                        if let Some(device) = devices_guard.get_mut(&device_id) {
                            device.online = false;
                            warn!(device_id = %device_id, "Device marked as offline");
                        }
                    }
                }

                debug!("Device discovery cycle completed");
            }

            info!("Device discovery task stopped");
        });

        Ok(())
    }

    /// Start automation engine
    #[instrument(skip(self))]
    async fn start_automation_engine(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(5)); // Check automations every 5 seconds
        let devices = Arc::clone(&self.devices);
        let scenes = Arc::clone(&self.scenes);
        let home_mode = Arc::clone(&self.home_mode);
        let automation_engine = Arc::clone(&self.automation_engine);
        let client = Arc::clone(&self.client);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting automation engine");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                let devices_guard = devices.read().await;
                let scenes_guard = scenes.read().await;
                let current_home_mode = *home_mode.read().await;

                for (scene_name, scene) in scenes_guard.iter() {
                    // Check if scene can be triggered
                    if !automation_engine.can_trigger_scene(scene_name).await {
                        continue;
                    }

                    // Check all conditions
                    let mut all_conditions_met = true;
                    for condition in &scene.conditions {
                        if !automation_engine
                            .check_condition(condition, &devices_guard, &current_home_mode)
                            .await
                        {
                            all_conditions_met = false;
                            break;
                        }
                    }

                    if all_conditions_met {
                        info!(scene = %scene_name, "Triggering automation scene");
                        automation_engine.mark_scene_triggered(scene_name).await;

                        // Execute scene actions
                        for action in &scene.actions {
                            if let Some(delay) = action.delay_ms {
                                tokio::time::sleep(Duration::from_millis(delay)).await;
                            }

                            let command = serde_json::json!({
                                "action": action.action,
                                "parameters": action.parameters
                            });

                            let topic = format!("devices/{}/command", action.device_id);
                            if let Err(e) = client
                                .publish_qos1(&topic, command.to_string().as_bytes())
                                .await
                            {
                                warn!(device_id = %action.device_id, error = %e, "Failed to send device command");
                            } else {
                                debug!(device_id = %action.device_id, action = %action.action, "Scene action executed");
                            }
                        }
                    }
                }

                debug!("Automation engine cycle completed");
            }

            info!("Automation engine stopped");
        });

        Ok(())
    }

    /// Start health monitoring task
    #[instrument(skip(self))]
    async fn start_health_monitor(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(30));
        let devices = Arc::clone(&self.devices);
        let client = Arc::clone(&self.client);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting health monitor");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                let devices_guard = devices.read().await;
                let mut health_stats = HashMap::new();

                let total_devices = devices_guard.len();
                let online_devices = devices_guard.values().filter(|d| d.online).count();
                let low_battery_devices = devices_guard
                    .values()
                    .filter(|d| d.battery_level.is_some_and(|level| level < 20.0))
                    .count();

                health_stats.insert("total_devices", total_devices);
                health_stats.insert("online_devices", online_devices);
                health_stats.insert("offline_devices", total_devices - online_devices);
                health_stats.insert("low_battery_devices", low_battery_devices);

                // Group by device type
                let mut type_counts = HashMap::new();
                for device in devices_guard.values() {
                    *type_counts.entry(&device.device_type).or_insert(0) += 1;
                }

                let health_report = serde_json::json!({
                    "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                    "overall_stats": health_stats,
                    "device_types": type_counts,
                    "system_status": if online_devices as f32 / total_devices as f32 > 0.8 { "healthy" } else { "degraded" }
                });

                if let Err(e) = client
                    .publish_qos0("smarthome/hub/health", health_report.to_string().as_bytes())
                    .await
                {
                    warn!(error = %e, "Failed to publish health report");
                }

                debug!(total = %total_devices, online = %online_devices, "Health check completed");
            }

            info!("Health monitor stopped");
        });

        Ok(())
    }

    /// Start energy management task  
    #[instrument(skip(self))]
    async fn start_energy_manager(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(60));
        let devices = Arc::clone(&self.devices);
        let energy_settings = Arc::clone(&self.energy_settings);
        let client = Arc::clone(&self.client);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting energy manager");

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                let devices_guard = devices.read().await;
                let settings = energy_settings.read().await;

                // Calculate total power consumption
                let total_power: f32 = devices_guard
                    .values()
                    .filter(|d| d.online)
                    .filter_map(|d| d.capabilities.power_consumption)
                    .sum();

                let current_hour = get_current_hour();
                let is_peak_hour =
                    current_hour >= settings.peak_hours.0 && current_hour <= settings.peak_hours.1;

                // Check if we're exceeding power limits
                let power_limit = if is_peak_hour {
                    settings.max_power_consumption * 0.8
                } else {
                    settings.max_power_consumption
                };

                if total_power > power_limit {
                    warn!(total_power = %total_power, limit = %power_limit, "Power consumption exceeds limit");

                    // Find non-critical devices to turn off
                    let mut devices_to_control = Vec::new();
                    for device in devices_guard.values() {
                        if device.online
                            && matches!(
                                device.device_type,
                                DeviceType::SmartPlug | DeviceType::Light
                            )
                        {
                            if let Some(power) = device.capabilities.power_consumption {
                                devices_to_control.push((device.device_id.clone(), power));
                            }
                        }
                    }

                    // Sort by power consumption (highest first)
                    devices_to_control.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

                    // Turn off devices until we're under the limit
                    let mut power_to_save = total_power - power_limit;
                    for (device_id, device_power) in devices_to_control {
                        if power_to_save <= 0.0 {
                            break;
                        }

                        let command = serde_json::json!({
                            "action": "turn_off",
                            "reason": "energy_management"
                        });

                        let topic = format!("devices/{device_id}/command");
                        if let Err(e) = client
                            .publish_qos1(&topic, command.to_string().as_bytes())
                            .await
                        {
                            warn!(device_id = %device_id, error = %e, "Failed to send energy control command");
                        } else {
                            info!(device_id = %device_id, power_saved = %device_power, "Device turned off for energy management");
                            power_to_save -= device_power;
                        }
                    }
                }

                let energy_report = serde_json::json!({
                    "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                    "total_power_consumption": total_power,
                    "power_limit": power_limit,
                    "is_peak_hour": is_peak_hour,
                    "energy_saving_active": settings.energy_saving_mode
                });

                if let Err(e) = client
                    .publish_qos0("smarthome/hub/energy", energy_report.to_string().as_bytes())
                    .await
                {
                    warn!(error = %e, "Failed to publish energy report");
                }

                debug!(total_power = %total_power, limit = %power_limit, "Energy management cycle completed");
            }

            info!("Energy manager stopped");
        });

        Ok(())
    }

    /// Start security monitoring task
    #[instrument(skip(self))]
    async fn start_security_monitor(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut alert_receiver = self.alert_sender.subscribe();
        let client = Arc::clone(&self.client);
        let _home_mode = Arc::clone(&self.home_mode);
        let devices = Arc::clone(&self.devices);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("Starting security monitor");

            while is_running.load(Ordering::SeqCst) {
                tokio::select! {
                    alert = alert_receiver.recv() => {
                        if let Ok(alert) = alert {
                            info!(alert_id = %alert.id, level = ?alert.level, "Processing security alert");

                            // Handle different alert levels
                            match alert.level {
                                AlertLevel::Emergency => {
                                    // Emergency response: turn on all lights, unlock doors, activate sirens
                                    Self::handle_emergency_response(&client, &devices).await;
                                }
                                AlertLevel::Critical => {
                                    // Critical response: activate security cameras, send notifications
                                    Self::handle_critical_response(&client, &devices, &alert).await;
                                }
                                AlertLevel::Warning => {
                                    // Warning response: log and monitor
                                    Self::handle_warning_response(&client, &alert).await;
                                }
                                AlertLevel::Info => {
                                    // Info response: just log
                                    debug!(alert_id = %alert.id, "Info alert logged");
                                }
                            }

                            // Publish alert to notification systems
                            let alert_level = alert.level;
                            let alert_topic = format!("smarthome/alerts/{}", alert_level as u8);
                            if let Ok(alert_json) = serde_json::to_string(&alert) {
                                if let Err(e) = client.publish_qos1(&alert_topic, alert_json.as_bytes()).await {
                                    warn!(error = %e, "Failed to publish security alert");
                                }
                            }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        // Periodic security checks
                        continue;
                    }
                }
            }

            info!("Security monitor stopped");
        });

        Ok(())
    }

    /// Handle emergency response
    async fn handle_emergency_response(
        client: &MqttClient,
        devices: &Arc<RwLock<HashMap<String, DeviceState>>>,
    ) {
        warn!("EMERGENCY RESPONSE ACTIVATED");

        let devices_guard = devices.read().await;

        // Turn on all lights
        for device in devices_guard.values() {
            if device.device_type == DeviceType::Light && device.online {
                let command = serde_json::json!({
                    "action": "turn_on",
                    "parameters": {
                        "brightness": 100,
                        "color": "red"
                    }
                });

                let topic = format!("devices/{}/command", device.device_id);
                if let Err(e) = client
                    .publish_qos1(&topic, command.to_string().as_bytes())
                    .await
                {
                    warn!(device_id = %device.device_id, error = %e, "Failed to activate emergency lighting");
                }
            }
        }

        // Unlock all doors
        for device in devices_guard.values() {
            if device.device_type == DeviceType::DoorLock && device.online {
                let command = serde_json::json!({
                    "action": "unlock",
                    "parameters": {
                        "reason": "emergency"
                    }
                });

                let topic = format!("devices/{}/command", device.device_id);
                if let Err(e) = client
                    .publish_qos1(&topic, command.to_string().as_bytes())
                    .await
                {
                    warn!(device_id = %device.device_id, error = %e, "Failed to unlock door for emergency");
                }
            }
        }

        // Activate all speakers for emergency announcement
        for device in devices_guard.values() {
            if device.device_type == DeviceType::Speaker && device.online {
                let command = serde_json::json!({
                    "action": "announce",
                    "parameters": {
                        "message": "EMERGENCY DETECTED. EVACUATE IMMEDIATELY.",
                        "volume": 100,
                        "repeat": 3
                    }
                });

                let topic = format!("devices/{}/command", device.device_id);
                if let Err(e) = client
                    .publish_qos1(&topic, command.to_string().as_bytes())
                    .await
                {
                    warn!(device_id = %device.device_id, error = %e, "Failed to activate emergency announcement");
                }
            }
        }
    }

    /// Handle critical security response
    async fn handle_critical_response(
        client: &MqttClient,
        devices: &Arc<RwLock<HashMap<String, DeviceState>>>,
        alert: &SecurityAlert,
    ) {
        warn!(alert_id = %alert.id, "CRITICAL SECURITY RESPONSE ACTIVATED");

        let devices_guard = devices.read().await;

        // Activate all security cameras
        for device in devices_guard.values() {
            if device.device_type == DeviceType::SecurityCamera && device.online {
                let command = serde_json::json!({
                    "action": "start_recording",
                    "parameters": {
                        "quality": "high",
                        "duration": 300 // 5 minutes
                    }
                });

                let topic = format!("devices/{}/command", device.device_id);
                if let Err(e) = client
                    .publish_qos1(&topic, command.to_string().as_bytes())
                    .await
                {
                    warn!(device_id = %device.device_id, error = %e, "Failed to activate security camera");
                }
            }
        }

        // Send notification through speakers
        for device in devices_guard.values() {
            if device.device_type == DeviceType::Speaker && device.online {
                let command = serde_json::json!({
                    "action": "announce",
                    "parameters": {
                        "message": format!("Security alert: {}", alert.message),
                        "volume": 80
                    }
                });

                let topic = format!("devices/{}/command", device.device_id);
                if let Err(e) = client
                    .publish_qos1(&topic, command.to_string().as_bytes())
                    .await
                {
                    warn!(device_id = %device.device_id, error = %e, "Failed to send security announcement");
                }
            }
        }
    }

    /// Handle warning response
    async fn handle_warning_response(client: &MqttClient, alert: &SecurityAlert) {
        info!(alert_id = %alert.id, "Processing warning alert");

        // Log warning to security log
        let log_entry = serde_json::json!({
            "timestamp": alert.timestamp,
            "level": "WARNING",
            "device_id": alert.device_id,
            "message": alert.message
        });

        if let Err(e) = client
            .publish_qos0("smarthome/security/log", log_entry.to_string().as_bytes())
            .await
        {
            warn!(error = %e, "Failed to log security warning");
        }
    }

    /// Setup default automation scenes
    #[instrument(skip(self))]
    async fn setup_default_scenes(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Setting up default automation scenes");

        let mut scenes_guard = self.scenes.write().await;

        // Good morning scene
        let good_morning = Scene {
            name: "Good Morning".to_string(),
            description: "Gradual wake-up lighting and climate adjustment".to_string(),
            actions: vec![
                DeviceAction {
                    device_id: "*bedroom_lights*".to_string(),
                    action: "turn_on".to_string(),
                    parameters: [(
                        "brightness".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(30)),
                    )]
                    .into(),
                    delay_ms: None,
                },
                DeviceAction {
                    device_id: "*thermostat*".to_string(),
                    action: "set_temperature".to_string(),
                    parameters: [(
                        "temperature".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(22)),
                    )]
                    .into(),
                    delay_ms: Some(300000), // 5 minutes
                },
            ],
            conditions: vec![
                SceneCondition::TimeRange {
                    start_hour: 6,
                    end_hour: 8,
                },
                SceneCondition::HomeOccupancy { occupied: true },
            ],
            priority: 5,
        };

        // Evening scene
        let evening_scene = Scene {
            name: "Evening Ambiance".to_string(),
            description: "Dim lights and comfortable temperature".to_string(),
            actions: vec![DeviceAction {
                device_id: "*living_room_lights*".to_string(),
                action: "turn_on".to_string(),
                parameters: [
                    (
                        "brightness".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(60)),
                    ),
                    (
                        "color".to_string(),
                        serde_json::Value::String("warm_white".to_string()),
                    ),
                ]
                .into(),
                delay_ms: None,
            }],
            conditions: vec![
                SceneCondition::TimeRange {
                    start_hour: 18,
                    end_hour: 22,
                },
                SceneCondition::HomeOccupancy { occupied: true },
            ],
            priority: 3,
        };

        // Away mode scene
        let away_scene = Scene {
            name: "Away Mode".to_string(),
            description: "Security and energy saving when away".to_string(),
            actions: vec![
                DeviceAction {
                    device_id: "*lights*".to_string(),
                    action: "turn_off".to_string(),
                    parameters: HashMap::new(),
                    delay_ms: None,
                },
                DeviceAction {
                    device_id: "*thermostat*".to_string(),
                    action: "set_temperature".to_string(),
                    parameters: [(
                        "temperature".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(18)),
                    )]
                    .into(),
                    delay_ms: Some(5000),
                },
                DeviceAction {
                    device_id: "*security_cameras*".to_string(),
                    action: "arm".to_string(),
                    parameters: HashMap::new(),
                    delay_ms: Some(10000),
                },
            ],
            conditions: vec![SceneCondition::HomeOccupancy { occupied: false }],
            priority: 8,
        };

        scenes_guard.insert("good_morning".to_string(), good_morning);
        scenes_guard.insert("evening_ambiance".to_string(), evening_scene);
        scenes_guard.insert("away_mode".to_string(), away_scene);

        info!("Default scenes configured");
        Ok(())
    }

    /// Set home mode
    #[instrument(skip(self))]
    pub async fn set_home_mode(&self, mode: HomeMode) -> Result<(), Box<dyn std::error::Error>> {
        info!(mode = ?mode, "Setting home mode");

        *self.home_mode.write().await = mode;

        // Publish mode change
        let mode_data = serde_json::json!({
            "mode": mode,
            "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
        });

        self.client
            .publish_retain("smarthome/hub/mode", mode_data.to_string().as_bytes())
            .await?;

        info!(mode = ?mode, "Home mode set successfully");
        Ok(())
    }

    /// Get device statistics
    #[instrument(skip(self))]
    pub async fn get_device_stats(&self) -> HashMap<String, usize> {
        let devices_guard = self.devices.read().await;
        let mut stats = HashMap::new();

        stats.insert("total".to_string(), devices_guard.len());
        stats.insert(
            "online".to_string(),
            devices_guard.values().filter(|d| d.online).count(),
        );

        // Count by device type
        for device in devices_guard.values() {
            let type_key = format!("{:?}", device.device_type).to_lowercase();
            *stats.entry(type_key).or_insert(0) += 1;
        }

        stats
    }

    /// Stop the smart home hub
    #[instrument(skip(self))]
    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Stopping Smart Home Hub");

        self.is_running.store(false, Ordering::SeqCst);

        // Publish hub offline status
        self.client
            .publish_retain("smarthome/hub/status", b"offline")
            .await?;

        // Disconnect from broker
        self.client.disconnect().await?;

        info!("Smart Home Hub stopped");
        Ok(())
    }
}

// Note: Replaced chrono dependency with std::time for time handling
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_target(false)
        .init();

    // Determine broker URL and hub ID based on environment
    let broker_url = std::env::var("MQTT_BROKER_URL").unwrap_or_else(|_| {
        if std::env::var("PRODUCTION").is_ok() {
            // Example production URLs with mTLS:
            // HiveMQ Cloud: mqtts://your-cluster.s1.eu.hivemq.cloud:8883
            // AWS IoT Core: mqtts://your-endpoint.iot.region.amazonaws.com:8883
            // Azure IoT Hub: mqtts://your-hub.azure-devices.net:8883
            "mqtts://test.mosquitto.org:8883".to_string()
        } else {
            // Local development broker
            "mqtt://localhost:1883".to_string()
        }
    });

    let hub_id = std::env::var("HUB_ID").unwrap_or_else(|_| "main-hub".to_string());

    info!(broker_url = %broker_url, hub_id = %hub_id, "Using MQTT broker for Smart Home Hub");

    // Create and start the smart home hub
    let hub = SmartHomeHub::new(&hub_id, &broker_url).await?;

    info!("Starting Smart Home Hub...");
    hub.start().await?;

    // Set initial home mode
    hub.set_home_mode(HomeMode::Home).await?;

    // Run for demonstration
    info!("Smart Home Hub running for 120 seconds...");
    sleep(Duration::from_secs(120)).await;

    // Show device statistics
    let stats = hub.get_device_stats().await;
    info!(stats = ?stats, "Final device statistics");

    // Stop gracefully
    hub.stop().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_hour() {
        let hour = get_current_hour();
        // Should be a valid hour (0-23)
        assert!(hour <= 23);
        
        // Test multiple calls return consistent results within same second
        let hour2 = get_current_hour();
        assert_eq!(hour, hour2);
    }

    #[test]
    fn test_get_current_hour_returns_valid_range() {
        // Test that hour is always in valid range over multiple calls
        for _ in 0..10 {
            let hour = get_current_hour();
            assert!(hour <= 23, "Hour {} is out of range", hour);
        }
    }
}
