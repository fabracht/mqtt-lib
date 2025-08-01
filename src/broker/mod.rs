//! MQTT v5.0 Broker Implementation
//!
//! A high-performance, async MQTT broker that follows the same architectural
//! principles as the client: no event loops, direct async/await patterns.
//!
//! # Example
//!
//! ```rust,no_run
//! use mqtt_v5::broker::{MqttBroker, BrokerConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Start a simple broker
//!     let broker = MqttBroker::bind("0.0.0.0:1883").await?;
//!     
//!     // Run until shutdown signal
//!     tokio::signal::ctrl_c().await?;
//!     broker.shutdown().await?;
//!     
//!     Ok(())
//! }
//! ```

pub mod acl;
pub mod auth;
pub mod client_handler;
pub mod config;
pub mod router;
pub mod server;
pub mod storage;
pub mod sys_topics;
mod tcp_stream_wrapper;

pub use acl::{AclManager, AclRule, Permission};
pub use auth::{
    AllowAllAuthProvider, AuthProvider, AuthResult, CertificateAuthProvider,
    ComprehensiveAuthProvider, PasswordAuthProvider,
};
pub use config::{BrokerConfig, StorageBackend as StorageBackendType, StorageConfig};
pub use server::MqttBroker;
pub use storage::{DynamicStorage, FileBackend, MemoryBackend, Storage, StorageBackend};

// Re-export key types for convenience
pub use crate::QoS;
