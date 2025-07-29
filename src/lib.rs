//! # MQTT v5.0 Client Library
//!
//! A complete MQTT v5.0 client library with full protocol compliance and a simple, callback-based API.
//!
//! ## CRITICAL: NO EVENT LOOPS
//!
//! **This is a Rust async library. We do NOT use event loops.**
//!
//! This library uses direct async/await patterns throughout. Event loops are an anti-pattern
//! in Rust async programming. Instead, we use:
//! - Direct async methods for all operations
//! - Background async tasks for continuous operations (packet reading, keepalive)
//! - The Tokio runtime for task scheduling
//!
//! If you're contributing to this library and thinking about implementing an event loop - STOP.
//! Read ARCHITECTURE.md first. Event loops, command channels, and actor patterns are explicitly
//! forbidden in this codebase.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use mqtt_v5::{MqttClient, ConnectOptions, QoS};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = MqttClient::new("test-client");
//!     
//!     // Direct async connect - no event loops
//!     client.connect("mqtt://test.mosquitto.org:1883").await?;
//!     
//!     // Direct async subscribe with callback
//!     client.subscribe("sensors/+/data", |msg| {
//!         println!("Received {} on {}",
//!                  String::from_utf8_lossy(&msg.payload),
//!                  msg.topic);
//!     }).await?;
//!     
//!     // Direct async publish
//!     client.publish("sensors/temp/data", b"25.5").await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Advanced Example
//!
//! ```rust,no_run
//! use mqtt_v5::{MqttClient, ConnectOptions, PublishOptions, QoS, ConnectionEvent};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Configure connection options
//!     let options = ConnectOptions::new("weather-station")
//!         .with_clean_start(false)  // Resume previous session
//!         .with_keep_alive(Duration::from_secs(30))
//!         .with_automatic_reconnect(true)
//!         .with_reconnect_delay(Duration::from_secs(5), Duration::from_secs(60));
//!     
//!     let client = MqttClient::with_options(options);
//!     
//!     // Monitor connection events
//!     client.on_connection_event(|event| {
//!         match event {
//!             ConnectionEvent::Connected { session_present } => {
//!                 println!("Connected! Session present: {}", session_present);
//!             }
//!             ConnectionEvent::Disconnected { reason } => {
//!                 println!("Disconnected: {:?}", reason);
//!             }
//!             ConnectionEvent::Reconnecting { attempt } => {
//!                 println!("Reconnecting... attempt {}", attempt);
//!             }
//!             ConnectionEvent::ReconnectFailed { error } => {
//!                 println!("Reconnection failed: {}", error);
//!             }
//!         }
//!     }).await?;
//!     
//!     // Connect to broker
//!     client.connect("mqtts://broker.example.com:8883").await?;
//!     
//!     // Subscribe with QoS 2 for critical data
//!     client.subscribe("weather/+/alerts", |msg| {
//!         if msg.retain {
//!             println!("Retained alert: {}", String::from_utf8_lossy(&msg.payload));
//!         } else {
//!             println!("New alert: {}", String::from_utf8_lossy(&msg.payload));
//!         }
//!     }).await?;
//!     
//!     // Publish with custom options
//!     let mut pub_opts = PublishOptions::default();
//!     pub_opts.qos = QoS::ExactlyOnce;
//!     pub_opts.retain = true;
//!     pub_opts.properties.message_expiry_interval = Some(3600); // 1 hour
//!     
//!     client.publish_with_options(
//!         "weather/station01/temperature",
//!         b"25.5",
//!         pub_opts
//!     ).await?;
//!     
//!     // Keep running
//!     tokio::time::sleep(Duration::from_secs(3600)).await;
//!     
//!     Ok(())
//! }
//! ```

#![warn(clippy::pedantic)]

pub mod callback;
pub mod client;
pub mod constants;
pub mod encoding;
pub mod error;
pub mod flags;
pub mod packet;
pub mod packet_id;
pub mod protocol;
pub mod session;
pub mod tasks; // Direct async tasks - NO event loops
pub mod test_utils;
pub mod topic_matching;
pub mod transport;
pub mod types;
pub mod validation;

pub use client::{
    ConnectionEvent, DisconnectReason, MockCall, MockMqttClient, MqttClient, MqttClientTrait,
};
pub use error::{MqttError, Result};
pub use packet::publish::PublishPacket;
pub use packet::{FixedHeader, Packet, PacketType};
pub use protocol::v5::properties::{Properties, PropertyId, PropertyValue, PropertyValueType};
pub use types::{
    ConnectOptions, ConnectProperties, ConnectResult, ConnectionStats, Message, MessageProperties,
    PublishOptions, PublishProperties, PublishResult, RetainHandling, SubscribeOptions,
    WillMessage, WillProperties,
};
pub use validation::{
    is_valid_client_id, is_valid_topic_filter, is_valid_topic_name, topic_matches_filter,
    validate_client_id, validate_topic_filter, validate_topic_name, RestrictiveValidator,
    StandardValidator, TopicValidator,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

impl From<u8> for QoS {
    fn from(value: u8) -> Self {
        match value {
            1 => QoS::AtLeastOnce,
            2 => QoS::ExactlyOnce,
            _ => QoS::AtMostOnce, // Default to QoS 0 for invalid values (including 0)
        }
    }
}

impl From<QoS> for u8 {
    fn from(qos: QoS) -> Self {
        qos as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qos_values() {
        assert_eq!(QoS::AtMostOnce as u8, 0);
        assert_eq!(QoS::AtLeastOnce as u8, 1);
        assert_eq!(QoS::ExactlyOnce as u8, 2);
    }

    #[test]
    fn test_qos_from_u8() {
        assert_eq!(QoS::from(0), QoS::AtMostOnce);
        assert_eq!(QoS::from(1), QoS::AtLeastOnce);
        assert_eq!(QoS::from(2), QoS::ExactlyOnce);

        // Invalid values default to AtMostOnce
        assert_eq!(QoS::from(3), QoS::AtMostOnce);
        assert_eq!(QoS::from(255), QoS::AtMostOnce);
    }

    #[test]
    fn test_qos_into_u8() {
        assert_eq!(u8::from(QoS::AtMostOnce), 0);
        assert_eq!(u8::from(QoS::AtLeastOnce), 1);
        assert_eq!(u8::from(QoS::ExactlyOnce), 2);
    }
}
