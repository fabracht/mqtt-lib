//! Bridge configuration types

use crate::QoS;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Bridge connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    /// Unique name for this bridge
    pub name: String,

    /// Remote broker address (hostname:port)
    pub remote_address: String,

    /// Client ID to use when connecting to remote broker
    pub client_id: String,

    /// Username for authentication (optional)
    pub username: Option<String>,

    /// Password for authentication (optional)
    pub password: Option<String>,

    /// Whether to use TLS (optional)
    #[serde(default)]
    pub use_tls: bool,

    /// TLS server name for verification (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_server_name: Option<String>,

    /// Indicate this is a bridge connection (mosquitto compatibility)
    #[serde(default = "default_try_private")]
    pub try_private: bool,

    /// Whether to start with a clean session
    #[serde(default = "default_clean_start")]
    pub clean_start: bool,

    /// Keep-alive interval in seconds
    #[serde(default = "default_keepalive")]
    pub keepalive: u16,

    /// MQTT protocol version
    #[serde(default)]
    pub protocol_version: MqttVersion,

    /// Delay between reconnection attempts
    #[serde(with = "humantime_serde", default = "default_reconnect_delay")]
    pub reconnect_delay: Duration,

    /// Maximum reconnection attempts (None = infinite)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_reconnect_attempts: Option<u32>,

    /// Backup broker addresses for failover
    #[serde(default)]
    pub backup_brokers: Vec<String>,

    /// Topic mappings for this bridge
    #[serde(default)]
    pub topics: Vec<TopicMapping>,
}

/// Topic mapping configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicMapping {
    /// Topic pattern (can include MQTT wildcards)
    pub pattern: String,

    /// Direction of message flow
    pub direction: BridgeDirection,

    /// Quality of Service level
    #[serde(default = "default_qos")]
    pub qos: QoS,

    /// Prefix to add to local topics (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_prefix: Option<String>,

    /// Prefix to add to remote topics (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_prefix: Option<String>,
}

/// Direction of message flow for a bridge topic
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BridgeDirection {
    /// Messages flow from remote broker to local broker
    In,
    /// Messages flow from local broker to remote broker
    Out,
    /// Messages flow in both directions
    Both,
}

/// MQTT protocol version
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MqttVersion {
    /// MQTT 3.1
    #[serde(rename = "mqttv31")]
    V31,
    /// MQTT 3.1.1
    #[serde(rename = "mqttv311")]
    V311,
    /// MQTT 5.0
    #[serde(rename = "mqttv50")]
    V50,
}

impl Default for MqttVersion {
    fn default() -> Self {
        Self::V50
    }
}

// Default value functions for serde
fn default_try_private() -> bool {
    true
}

fn default_clean_start() -> bool {
    false
}

fn default_keepalive() -> u16 {
    60
}

fn default_reconnect_delay() -> Duration {
    Duration::from_secs(5)
}

fn default_qos() -> QoS {
    QoS::AtMostOnce
}

impl BridgeConfig {
    /// Creates a new bridge configuration
    pub fn new(name: impl Into<String>, remote_address: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            remote_address: remote_address.into(),
            client_id: format!("bridge-{}", name),
            username: None,
            password: None,
            use_tls: false,
            tls_server_name: None,
            try_private: true,
            clean_start: false,
            keepalive: 60,
            protocol_version: MqttVersion::V50,
            reconnect_delay: Duration::from_secs(5),
            max_reconnect_attempts: None,
            backup_brokers: Vec::new(),
            topics: Vec::new(),
        }
    }

    /// Adds a topic mapping to the bridge
    #[must_use]
    pub fn add_topic(
        mut self,
        pattern: impl Into<String>,
        direction: BridgeDirection,
        qos: QoS,
    ) -> Self {
        self.topics.push(TopicMapping {
            pattern: pattern.into(),
            direction,
            qos,
            local_prefix: None,
            remote_prefix: None,
        });
        self
    }

    /// Adds a backup broker address
    #[must_use]
    pub fn add_backup_broker(mut self, address: impl Into<String>) -> Self {
        self.backup_brokers.push(address.into());
        self
    }

    /// Validates the configuration
    pub fn validate(&self) -> crate::Result<()> {
        use crate::error::MqttError;

        if self.name.is_empty() {
            return Err(MqttError::Configuration(
                "Bridge name cannot be empty".into(),
            ));
        }

        if self.client_id.is_empty() {
            return Err(MqttError::Configuration("Client ID cannot be empty".into()));
        }

        if self.topics.is_empty() {
            return Err(MqttError::Configuration(
                "Bridge must have at least one topic mapping".into(),
            ));
        }

        // Validate topic patterns
        for topic in &self.topics {
            if topic.pattern.is_empty() {
                return Err(MqttError::Configuration(
                    "Topic pattern cannot be empty".into(),
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_config_creation() {
        let config = BridgeConfig::new("test-bridge", "remote.broker:1883")
            .add_topic("sensors/+/data", BridgeDirection::Out, QoS::AtLeastOnce)
            .add_backup_broker("backup.broker:1883");

        assert_eq!(config.name, "test-bridge");
        assert_eq!(config.client_id, "bridge-test-bridge");
        assert_eq!(config.topics.len(), 1);
        assert_eq!(config.backup_brokers.len(), 1);
    }

    #[test]
    fn test_bridge_config_validation() {
        let mut config = BridgeConfig::new("test", "broker:1883");

        // Should fail without topics
        assert!(config.validate().is_err());

        // Should pass with topics
        config = config.add_topic("test/#", BridgeDirection::Both, QoS::AtMostOnce);
        assert!(config.validate().is_ok());
    }
}
