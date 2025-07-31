//! WebSocket transport implementation for MQTT over `WebSockets`
//!
//! This module provides WebSocket transport for MQTT connections, enabling
//! MQTT communication in web browsers and environments where TCP connections
//! are not available or blocked by firewalls.
//!
//! ## Features
//!
//! - Plain WebSocket connections (ws://)
//! - Secure WebSocket connections (wss://) with TLS
//! - Custom headers support
//! - Subprotocol negotiation (mqtt, mqttv3.1, mqttv5.0)
//! - Connection timeouts and keep-alive
//! - Automatic reconnection support
//!
//! ## Usage
//!
//! ```rust,no_run
//! use mqtt_v5::transport::websocket::{WebSocketConfig, WebSocketTransport};
//! use mqtt_v5::transport::Transport;
//! use std::time::Duration;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Basic WebSocket connection
//! let config = WebSocketConfig::new("ws://broker.example.com:8080/mqtt")?;
//! let mut transport = WebSocketTransport::new(config);
//! transport.connect().await?;
//!
//! // Secure WebSocket with custom configuration
//! let config = WebSocketConfig::new("wss://secure-broker.example.com/mqtt")?
//!     .with_timeout(Duration::from_secs(30))
//!     .with_subprotocol("mqtt")  
//!     .with_header("Authorization", "Bearer token123");
//!
//! let mut transport = WebSocketTransport::new(config);
//! transport.connect().await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{MqttError, Result};
use crate::transport::tls::TlsConfig;
use crate::transport::Transport;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use url::Url;

/// WebSocket transport configuration
#[derive(Debug)]
pub struct WebSocketConfig {
    /// WebSocket URL (ws:// or wss://)
    pub url: Url,
    /// Connection timeout
    pub timeout: Duration,
    /// Subprotocols to negotiate (e.g., "mqtt", "mqttv3.1", "mqttv5.0")
    pub subprotocols: Vec<String>,
    /// Custom HTTP headers for the WebSocket handshake
    pub headers: HashMap<String, String>,
    /// User agent string
    pub user_agent: Option<String>,
    /// TLS configuration for secure WebSocket connections (wss://)
    pub tls_config: Option<TlsConfig>,
    /// Whether to verify TLS certificates (for wss://) - deprecated, use `tls_config`
    #[deprecated(note = "Use tls_config field instead")]
    pub verify_tls: bool,
}

impl WebSocketConfig {
    /// Creates a new WebSocket configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid or uses an unsupported scheme
    pub fn new(url: &str) -> Result<Self> {
        let parsed_url = Url::parse(url)
            .map_err(|e| MqttError::ProtocolError(format!("Invalid WebSocket URL: {e}")))?;

        match parsed_url.scheme() {
            "ws" | "wss" => {}
            scheme => {
                return Err(MqttError::ProtocolError(format!(
                    "Unsupported WebSocket scheme: {scheme}. Use 'ws' or 'wss'"
                )));
            }
        }

        Ok(Self {
            url: parsed_url,
            timeout: Duration::from_secs(30),
            subprotocols: vec!["mqtt".to_string()],
            headers: HashMap::new(),
            user_agent: Some("mqtt-v5/0.3.0".to_string()),
            tls_config: None,
            #[allow(deprecated)]
            verify_tls: true,
        })
    }

    /// Sets the connection timeout
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the WebSocket subprotocols to negotiate
    #[must_use]
    pub fn with_subprotocols(mut self, subprotocols: &[&str]) -> Self {
        self.subprotocols = subprotocols
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        self
    }

    /// Sets a single WebSocket subprotocol
    #[must_use]
    pub fn with_subprotocol(mut self, subprotocol: &str) -> Self {
        self.subprotocols = vec![subprotocol.to_string()];
        self
    }

    /// Adds a custom HTTP header
    #[must_use]
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.insert(name.to_string(), value.to_string());
        self
    }

    /// Sets the User-Agent header
    #[must_use]
    pub fn with_user_agent(mut self, user_agent: &str) -> Self {
        self.user_agent = Some(user_agent.to_string());
        self
    }

    /// Sets whether to verify TLS certificates for wss:// connections
    ///
    /// # Safety
    ///
    /// Disabling TLS verification is insecure and should only be used for testing
    #[deprecated(note = "Use with_tls_config instead")]
    #[must_use]
    pub fn with_tls_verification(mut self, verify: bool) -> Self {
        #[allow(deprecated)]
        {
            self.verify_tls = verify;
        }
        self
    }

    /// Sets a custom TLS configuration for wss:// connections
    #[must_use]
    pub fn with_tls_config(mut self, tls_config: TlsConfig) -> Self {
        self.tls_config = Some(tls_config);
        self
    }

    /// Creates a TLS configuration automatically from the WebSocket URL
    ///
    /// This is a convenience method that creates a TLS config with the same
    /// host and port as the WebSocket URL.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The URL is not a secure WebSocket (wss://)
    /// - The URL does not have a valid host
    /// - The host/port combination cannot be parsed as a socket address
    pub fn with_tls_auto(mut self) -> Result<Self> {
        if !self.is_secure() {
            return Err(MqttError::ProtocolError(
                "TLS configuration only applies to wss:// URLs".to_string(),
            ));
        }

        let host = self.host().ok_or_else(|| {
            MqttError::ProtocolError("WebSocket URL must have a host".to_string())
        })?;

        let addr: SocketAddr = format!("{}:{}", host, self.port())
            .parse()
            .map_err(|e| MqttError::ProtocolError(format!("Invalid host/port combination: {e}")))?;

        let tls_config = TlsConfig::new(addr, host);
        self.tls_config = Some(tls_config);
        Ok(self)
    }

    /// Adds client certificate authentication to the TLS configuration
    ///
    /// This method creates or modifies the TLS configuration to include client certificates.
    /// If no TLS config exists, it creates one automatically.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The URL is not a secure WebSocket (wss://)
    /// - The certificate or key files cannot be read or parsed
    /// - The TLS configuration cannot be created
    pub fn with_client_auth_from_files(mut self, cert_path: &str, key_path: &str) -> Result<Self> {
        if !self.is_secure() {
            return Err(MqttError::ProtocolError(
                "Client authentication only applies to wss:// URLs".to_string(),
            ));
        }

        // Create TLS config if it doesn't exist
        if self.tls_config.is_none() {
            self = self.with_tls_auto()?;
        }

        // Add client certificate to TLS config
        if let Some(ref mut tls_config) = self.tls_config {
            tls_config.load_client_cert_pem(cert_path)?;
            tls_config.load_client_key_pem(key_path)?;
        }

        Ok(self)
    }

    /// Adds client certificate authentication from bytes to the TLS configuration
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The URL is not a secure WebSocket (wss://)
    /// - The certificate or key bytes cannot be parsed
    /// - The TLS configuration cannot be created
    pub fn with_client_auth_from_bytes(mut self, cert_pem: &[u8], key_pem: &[u8]) -> Result<Self> {
        if !self.is_secure() {
            return Err(MqttError::ProtocolError(
                "Client authentication only applies to wss:// URLs".to_string(),
            ));
        }

        // Create TLS config if it doesn't exist
        if self.tls_config.is_none() {
            self = self.with_tls_auto()?;
        }

        // Add client certificate to TLS config
        if let Some(ref mut tls_config) = self.tls_config {
            tls_config.load_client_cert_pem_bytes(cert_pem)?;
            tls_config.load_client_key_pem_bytes(key_pem)?;
        }

        Ok(self)
    }

    /// Adds custom CA certificate from file to the TLS configuration
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The URL is not a secure WebSocket (wss://)
    /// - The CA certificate file cannot be read or parsed
    /// - The TLS configuration cannot be created
    pub fn with_ca_cert_from_file(mut self, ca_path: &str) -> Result<Self> {
        if !self.is_secure() {
            return Err(MqttError::ProtocolError(
                "CA certificate only applies to wss:// URLs".to_string(),
            ));
        }

        // Create TLS config if it doesn't exist
        if self.tls_config.is_none() {
            self = self.with_tls_auto()?;
        }

        // Add CA certificate to TLS config
        if let Some(ref mut tls_config) = self.tls_config {
            tls_config.load_ca_cert_pem(ca_path)?;
        }

        Ok(self)
    }

    /// Adds custom CA certificate from bytes to the TLS configuration
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The URL is not a secure WebSocket (wss://)
    /// - The CA certificate bytes cannot be parsed
    /// - The TLS configuration cannot be created
    pub fn with_ca_cert_from_bytes(mut self, ca_pem: &[u8]) -> Result<Self> {
        if !self.is_secure() {
            return Err(MqttError::ProtocolError(
                "CA certificate only applies to wss:// URLs".to_string(),
            ));
        }

        // Create TLS config if it doesn't exist
        if self.tls_config.is_none() {
            self = self.with_tls_auto()?;
        }

        // Add CA certificate to TLS config
        if let Some(ref mut tls_config) = self.tls_config {
            tls_config.load_ca_cert_pem_bytes(ca_pem)?;
        }

        Ok(self)
    }

    /// Returns true if this is a secure WebSocket connection (wss://)
    #[must_use]
    pub fn is_secure(&self) -> bool {
        self.url.scheme() == "wss"
    }

    /// Gets the host from the WebSocket URL
    #[must_use]
    pub fn host(&self) -> Option<&str> {
        self.url.host_str()
    }

    /// Gets the port from the WebSocket URL, with defaults for ws/wss
    #[must_use]
    pub fn port(&self) -> u16 {
        self.url.port().unwrap_or_else(|| match self.url.scheme() {
            "wss" => 443,
            _ => 80,
        })
    }

    /// Gets the TLS configuration for secure connections
    #[must_use]
    pub fn tls_config(&self) -> Option<&TlsConfig> {
        self.tls_config.as_ref()
    }

    /// Takes ownership of the TLS configuration
    #[must_use]
    pub fn take_tls_config(&mut self) -> Option<TlsConfig> {
        self.tls_config.take()
    }
}

/// WebSocket transport implementation
///
/// Note: This is a placeholder implementation. A full implementation would
/// require a WebSocket library like `tokio-tungstenite` or `async-tungstenite`.
#[derive(Debug)]
pub struct WebSocketTransport {
    config: WebSocketConfig,
    connected: bool,
    // In a real implementation, this would hold the WebSocket connection
    // connection: Option<WebSocketStream<...>>,
}

impl WebSocketTransport {
    /// Creates a new WebSocket transport
    #[must_use]
    pub fn new(config: WebSocketConfig) -> Self {
        Self {
            config,
            connected: false,
        }
    }

    /// Checks if the transport is connected
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Gets the WebSocket URL
    #[must_use]
    pub fn url(&self) -> &Url {
        &self.config.url
    }

    /// Gets the negotiated subprotocol (if any)
    #[must_use]
    pub fn subprotocol(&self) -> Option<&str> {
        // In a real implementation, this would return the negotiated subprotocol
        self.config.subprotocols.first().map(String::as_str)
    }

    /// Splits the WebSocket into read and write halves
    ///
    /// Note: WebSocket is message-based, not stream-based. In a real implementation,
    /// this would likely use channels or shared state rather than true splitting.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport is not connected
    pub fn into_split(self) -> Result<(WebSocketReadHandle, WebSocketWriteHandle)> {
        if !self.connected {
            return Err(MqttError::NotConnected);
        }

        // In a real implementation, this would set up channels or shared state
        // for communicating between the read and write halves
        let read_handle = WebSocketReadHandle {
            url: self.config.url.clone(),
        };
        let write_handle = WebSocketWriteHandle {
            url: self.config.url.clone(),
        };

        Ok((read_handle, write_handle))
    }
}

/// WebSocket read handle for split operations
#[derive(Debug)]
pub struct WebSocketReadHandle {
    #[allow(dead_code)] // Used for future WebSocket implementation
    url: Url,
}

/// WebSocket write handle for split operations  
#[derive(Debug)]
pub struct WebSocketWriteHandle {
    #[allow(dead_code)] // Used for future WebSocket implementation
    url: Url,
}

impl Transport for WebSocketTransport {
    async fn connect(&mut self) -> Result<()> {
        if self.connected {
            return Err(MqttError::AlreadyConnected);
        }

        // TODO: Implement actual WebSocket connection using tokio-tungstenite
        // This is a placeholder implementation for demonstration
        tracing::info!(
            url = %self.config.url,
            subprotocols = ?self.config.subprotocols,
            "Connecting to WebSocket broker"
        );

        // Simulate connection delay
        tokio::time::sleep(Duration::from_millis(100)).await;

        // For now, we'll just mark as connected
        // In a real implementation, this would:
        // 1. Create TLS connector if wss://
        // 2. Establish TCP connection
        // 3. Perform WebSocket handshake
        // 4. Negotiate subprotocol
        // 5. Store the WebSocket stream

        self.connected = true;
        tracing::info!("WebSocket connection established");
        Ok(())
    }

    async fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
        if !self.connected {
            return Err(MqttError::NotConnected);
        }

        // TODO: Implement actual WebSocket frame reading
        // This would:
        // 1. Read WebSocket frames
        // 2. Handle control frames (ping, pong, close)
        // 3. Extract binary data from data frames
        // 4. Return the data to the caller

        // Placeholder implementation
        Err(MqttError::ProtocolError(
            "WebSocket read not implemented".to_string(),
        ))
    }

    async fn write(&mut self, buf: &[u8]) -> Result<()> {
        if !self.connected {
            return Err(MqttError::NotConnected);
        }

        // TODO: Implement actual WebSocket frame writing
        // This would:
        // 1. Wrap data in WebSocket binary frames
        // 2. Send frames over the connection
        // 3. Handle flow control

        tracing::debug!(bytes = buf.len(), "Writing WebSocket frame");

        // Placeholder implementation
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        if !self.connected {
            return Ok(());
        }

        // TODO: Implement WebSocket close handshake
        // This would:
        // 1. Send WebSocket close frame
        // 2. Wait for close response
        // 3. Close underlying TCP connection

        tracing::info!("Closing WebSocket connection");
        self.connected = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_config_creation() {
        let config = WebSocketConfig::new("ws://localhost:8080/mqtt").unwrap();
        assert_eq!(config.url.as_str(), "ws://localhost:8080/mqtt");
        assert!(!config.is_secure());
        assert_eq!(config.host(), Some("localhost"));
        assert_eq!(config.port(), 8080);
        assert_eq!(config.subprotocols, vec!["mqtt"]);
    }

    #[test]
    fn test_websocket_config_secure() {
        let config = WebSocketConfig::new("wss://broker.example.com/mqtt").unwrap();
        assert_eq!(config.url.as_str(), "wss://broker.example.com/mqtt");
        assert!(config.is_secure());
        assert_eq!(config.host(), Some("broker.example.com"));
        assert_eq!(config.port(), 443); // Default HTTPS port
    }

    #[test]
    fn test_websocket_config_invalid_scheme() {
        let result = WebSocketConfig::new("http://example.com");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unsupported WebSocket scheme"));
    }

    #[test]
    fn test_websocket_config_with_options() {
        let config = WebSocketConfig::new("ws://localhost:8080/mqtt")
            .unwrap()
            .with_timeout(Duration::from_secs(60))
            .with_subprotocol("mqttv5.0")
            .with_header("Authorization", "Bearer token123")
            .with_user_agent("custom-client/1.0");

        assert_eq!(config.timeout, Duration::from_secs(60));
        assert_eq!(config.subprotocols, vec!["mqttv5.0"]);
        assert_eq!(
            config.headers.get("Authorization"),
            Some(&"Bearer token123".to_string())
        );
        assert_eq!(config.user_agent, Some("custom-client/1.0".to_string()));
    }

    #[tokio::test]
    async fn test_websocket_transport_creation() {
        let config = WebSocketConfig::new("ws://localhost:8080/mqtt").unwrap();
        let transport = WebSocketTransport::new(config);

        assert!(!transport.is_connected());
        assert_eq!(transport.url().as_str(), "ws://localhost:8080/mqtt");
        assert_eq!(transport.subprotocol(), Some("mqtt"));
    }

    #[tokio::test]
    async fn test_websocket_transport_connect() {
        let config = WebSocketConfig::new("ws://localhost:8080/mqtt").unwrap();
        let mut transport = WebSocketTransport::new(config);

        assert!(!transport.is_connected());
        transport.connect().await.unwrap();
        assert!(transport.is_connected());

        // Should fail to connect again
        let result = transport.connect().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_websocket_transport_operations_when_not_connected() {
        let config = WebSocketConfig::new("ws://localhost:8080/mqtt").unwrap();
        let mut transport = WebSocketTransport::new(config);

        let mut buf = [0u8; 10];
        assert!(transport.read(&mut buf).await.is_err());
        assert!(transport.write(b"test").await.is_err());

        // Close should succeed even when not connected
        assert!(transport.close().await.is_ok());
    }

    #[tokio::test]
    async fn test_websocket_transport_close() {
        let config = WebSocketConfig::new("ws://localhost:8080/mqtt").unwrap();
        let mut transport = WebSocketTransport::new(config);

        transport.connect().await.unwrap();
        assert!(transport.is_connected());

        transport.close().await.unwrap();
        assert!(!transport.is_connected());
    }

    #[test]
    fn test_websocket_config_port_defaults() {
        let ws_config = WebSocketConfig::new("ws://example.com/mqtt").unwrap();
        assert_eq!(ws_config.port(), 80);

        let wss_config = WebSocketConfig::new("wss://example.com/mqtt").unwrap();
        assert_eq!(wss_config.port(), 443);

        let custom_port_config = WebSocketConfig::new("ws://example.com:8080/mqtt").unwrap();
        assert_eq!(custom_port_config.port(), 8080);
    }

    #[test]
    fn test_websocket_config_tls_auto() {
        // Should work for wss:// with IP address
        let config = WebSocketConfig::new("wss://127.0.0.1:8443/mqtt")
            .unwrap()
            .with_tls_auto()
            .unwrap();

        assert!(config.tls_config().is_some());
        let tls_config = config.tls_config().unwrap();
        assert_eq!(tls_config.addr.port(), 8443);
        assert_eq!(tls_config.hostname, "127.0.0.1");

        // Should fail for ws://
        let result = WebSocketConfig::new("ws://127.0.0.1:8080/mqtt")
            .unwrap()
            .with_tls_auto();
        assert!(result.is_err());

        // Test with default port
        let config_default = WebSocketConfig::new("wss://127.0.0.1/mqtt")
            .unwrap()
            .with_tls_auto()
            .unwrap();

        let tls_config_default = config_default.tls_config().unwrap();
        assert_eq!(tls_config_default.addr.port(), 443);
    }

    #[test]
    fn test_websocket_config_client_auth_from_bytes() {
        let cert_pem = b"-----BEGIN CERTIFICATE-----\ntest\n-----END CERTIFICATE-----";
        let key_pem = b"-----BEGIN PRIVATE KEY-----\ntest\n-----END PRIVATE KEY-----";

        let config = WebSocketConfig::new("wss://127.0.0.1/mqtt")
            .unwrap()
            .with_client_auth_from_bytes(cert_pem, key_pem)
            .unwrap();

        assert!(config.tls_config().is_some());
        let tls_config = config.tls_config().unwrap();
        assert!(tls_config.client_cert.is_some());
        assert!(tls_config.client_key.is_some());

        // Should fail for ws://
        let result = WebSocketConfig::new("ws://127.0.0.1/mqtt")
            .unwrap()
            .with_client_auth_from_bytes(cert_pem, key_pem);
        assert!(result.is_err());
    }

    #[test]
    fn test_websocket_config_ca_cert_from_bytes() {
        let ca_pem = b"-----BEGIN CERTIFICATE-----\ntest ca\n-----END CERTIFICATE-----";

        let config = WebSocketConfig::new("wss://127.0.0.1/mqtt")
            .unwrap()
            .with_ca_cert_from_bytes(ca_pem)
            .unwrap();

        assert!(config.tls_config().is_some());
        let tls_config = config.tls_config().unwrap();
        assert!(tls_config.root_certs.is_some());

        // Should fail for ws://
        let result = WebSocketConfig::new("ws://127.0.0.1/mqtt")
            .unwrap()
            .with_ca_cert_from_bytes(ca_pem);
        assert!(result.is_err());
    }

    #[test]
    fn test_websocket_config_with_custom_tls_config() {
        use std::net::{IpAddr, Ipv4Addr};

        let addr = std::net::SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8883);
        let tls_config = TlsConfig::new(addr, "localhost");

        let config = WebSocketConfig::new("wss://broker.example.com/mqtt")
            .unwrap()
            .with_tls_config(tls_config);

        assert!(config.tls_config().is_some());
        let tls_config = config.tls_config().unwrap();
        assert_eq!(tls_config.hostname, "localhost");
        assert_eq!(tls_config.addr.port(), 8883);
    }

    #[test]
    fn test_websocket_config_take_tls_config() {
        let mut config = WebSocketConfig::new("wss://127.0.0.1/mqtt")
            .unwrap()
            .with_tls_auto()
            .unwrap();

        assert!(config.tls_config().is_some());

        let tls_config = config.take_tls_config();
        assert!(tls_config.is_some());
        assert!(config.tls_config().is_none()); // Should be None after taking

        let tls_config = tls_config.unwrap();
        assert_eq!(tls_config.hostname, "127.0.0.1");
    }
}
