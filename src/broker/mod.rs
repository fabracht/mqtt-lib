//! MQTT v5.0 Broker Implementation
//!
//! A high-performance, async MQTT broker using direct async/await patterns.
//!
//! # Example
//!
//! ```rust,no_run
//! use mqtt5::broker::{MqttBroker, BrokerConfig};
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
pub mod bridge;
pub mod client_handler;
pub mod config;
pub mod connection_pool;
pub mod hot_reload;
pub mod optimized_router;
pub mod resource_monitor;
pub mod router;
pub mod server;
pub mod storage;
pub mod sys_topics;
mod tcp_stream_wrapper;
pub mod tls_acceptor;
pub mod transport;
pub mod websocket_server;

pub use acl::{AclManager, AclRule, Permission};
pub use auth::{
    AllowAllAuthProvider, AuthProvider, AuthResult, CertificateAuthProvider,
    ComprehensiveAuthProvider, PasswordAuthProvider,
};
pub use config::{BrokerConfig, StorageBackend as StorageBackendType, StorageConfig};
pub use resource_monitor::{ResourceLimits, ResourceMonitor, ResourceStats};
pub use server::MqttBroker;
pub use storage::{DynamicStorage, FileBackend, MemoryBackend, Storage, StorageBackend};

// Re-export key types for convenience
pub use crate::QoS;
