use crate::error::{MqttError, Result};
use crate::transport::Transport;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::{client::TlsStream, TlsConnector};

/// Certificate verifier that accepts all certificates (for testing only)
#[derive(Debug)]
struct NoVerification;

impl ServerCertVerifier for NoVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}

/// TLS transport configuration
#[derive(Debug)]
pub struct TlsConfig {
    /// Server address
    pub addr: SocketAddr,
    /// Server hostname for SNI
    pub hostname: String,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Client certificate chain (optional)
    pub client_cert: Option<Vec<CertificateDer<'static>>>,
    /// Client private key (optional)
    pub client_key: Option<PrivateKeyDer<'static>>,
    /// Custom root certificates (optional)
    pub root_certs: Option<Vec<CertificateDer<'static>>>,
    /// Whether to use system root certificates
    pub use_system_roots: bool,
    /// Whether to verify server certificate
    pub verify_server_cert: bool,
    /// ALPN protocols (optional)
    pub alpn_protocols: Option<Vec<Vec<u8>>>,
}

impl TlsConfig {
    /// Creates a new TLS configuration
    #[must_use]
    pub fn new(addr: SocketAddr, hostname: impl Into<String>) -> Self {
        Self {
            addr,
            hostname: hostname.into(),
            connect_timeout: Duration::from_secs(30),
            client_cert: None,
            client_key: None,
            root_certs: None,
            use_system_roots: true,
            verify_server_cert: true,
            alpn_protocols: None,
        }
    }

    /// Sets the connection timeout
    #[must_use]
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Sets client certificate and key for mutual TLS
    #[must_use]
    pub fn with_client_auth(
        mut self,
        cert: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
    ) -> Self {
        self.client_cert = Some(cert);
        self.client_key = Some(key);
        self
    }

    /// Sets custom root certificates
    #[must_use]
    pub fn with_root_certs(mut self, certs: Vec<CertificateDer<'static>>) -> Self {
        self.root_certs = Some(certs);
        self
    }

    /// Sets whether to use system root certificates
    #[must_use]
    pub fn with_system_roots(mut self, use_system: bool) -> Self {
        self.use_system_roots = use_system;
        self
    }

    /// Sets whether to verify server certificate
    ///
    /// # Safety
    ///
    /// Disabling certificate verification is insecure and should only
    /// be used for testing or in controlled environments
    #[must_use]
    pub fn with_verify_server_cert(mut self, verify: bool) -> Self {
        self.verify_server_cert = verify;
        self
    }

    /// Sets ALPN protocols
    #[must_use]
    pub fn with_alpn_protocols(mut self, protocols: &[&str]) -> Self {
        self.alpn_protocols = Some(protocols.iter().map(|p| p.as_bytes().to_vec()).collect());
        self
    }

    /// Sets ALPN protocols from raw bytes
    #[must_use]
    pub fn with_alpn_protocols_raw(mut self, protocols: Vec<Vec<u8>>) -> Self {
        self.alpn_protocols = Some(protocols);
        self
    }

    /// Loads client certificate from PEM file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub fn load_client_cert_pem(&mut self, cert_path: &str) -> Result<()> {
        let cert_pem = std::fs::read(cert_path)?;
        let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut &cert_pem[..])
            .filter_map(std::result::Result::ok)
            .collect();
        if certs.is_empty() {
            return Err(MqttError::ProtocolError(
                "No certificates found in file".to_string(),
            ));
        }
        self.client_cert = Some(certs);
        Ok(())
    }

    /// Loads client private key from PEM file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub fn load_client_key_pem(&mut self, key_path: &str) -> Result<()> {
        let key_pem = std::fs::read(key_path)?;
        let mut keys: Vec<PrivateKeyDer<'static>> =
            rustls_pemfile::pkcs8_private_keys(&mut &key_pem[..])
                .filter_map(std::result::Result::ok)
                .map(PrivateKeyDer::from)
                .collect();

        if keys.is_empty() {
            // Try RSA keys if PKCS8 didn't work
            keys = rustls_pemfile::rsa_private_keys(&mut &key_pem[..])
                .filter_map(std::result::Result::ok)
                .map(PrivateKeyDer::from)
                .collect();
        }

        if keys.is_empty() {
            return Err(MqttError::ProtocolError(
                "No private keys found in file".to_string(),
            ));
        }
        self.client_key = Some(keys.into_iter().next().ok_or_else(|| {
            MqttError::ProtocolError("Keys vector unexpectedly empty".to_string())
        })?);
        Ok(())
    }

    /// Loads CA certificate from PEM file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    pub fn load_ca_cert_pem(&mut self, ca_path: &str) -> Result<()> {
        let ca_pem = std::fs::read(ca_path)?;
        let ca_certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut &ca_pem[..])
            .filter_map(std::result::Result::ok)
            .collect();
        if ca_certs.is_empty() {
            return Err(MqttError::ProtocolError(
                "No CA certificates found in file".to_string(),
            ));
        }
        self.root_certs = Some(ca_certs);
        Ok(())
    }
}

/// TLS transport implementation
#[derive(Debug)]
pub struct TlsTransport {
    config: TlsConfig,
    stream: Option<TlsStream<TcpStream>>,
}

impl TlsTransport {
    /// Creates a new TLS transport
    #[must_use]
    pub fn new(config: TlsConfig) -> Self {
        Self {
            config,
            stream: None,
        }
    }

    /// Checks if the transport is connected
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Builds the rustls client configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    fn build_tls_config(&mut self) -> Result<ClientConfig> {
        let mut root_store = RootCertStore::empty();

        // Add system roots if requested
        if self.config.use_system_roots {
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.to_vec());
        }

        // Add custom root certificates
        if let Some(ref root_certs) = self.config.root_certs {
            for cert in root_certs {
                root_store.add(cert.clone()).map_err(|e| {
                    MqttError::ProtocolError(format!("Failed to add root cert: {e}"))
                })?;
            }
        }

        let config_builder = if self.config.verify_server_cert {
            ClientConfig::builder().with_root_certificates(root_store)
        } else {
            // Disable certificate verification for testing with self-signed certs
            ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerification))
        };

        // Configure client authentication if provided
        let mut config = if let (Some(cert), Some(key)) = (
            self.config.client_cert.take(),
            self.config.client_key.take(),
        ) {
            config_builder
                .with_client_auth_cert(cert, key)
                .map_err(|e| {
                    MqttError::ProtocolError(format!("Failed to configure client auth: {e}"))
                })?
        } else {
            config_builder.with_no_client_auth()
        };

        // Configure ALPN protocols if provided
        if let Some(ref protocols) = self.config.alpn_protocols {
            config.alpn_protocols.clone_from(protocols);
        }

        Ok(config)
    }
}

impl Transport for TlsTransport {
    async fn connect(&mut self) -> Result<()> {
        if self.stream.is_some() {
            return Err(MqttError::AlreadyConnected);
        }

        // Build TLS configuration
        let tls_config = Arc::new(self.build_tls_config()?);
        let connector = TlsConnector::from(tls_config);

        // Connect TCP first
        let tcp_stream = timeout(
            self.config.connect_timeout,
            TcpStream::connect(self.config.addr),
        )
        .await
        .map_err(|_| MqttError::Timeout)??;

        // Configure TCP options
        tcp_stream.set_nodelay(true)?;

        // Perform TLS handshake
        let domain = ServerName::try_from(self.config.hostname.clone())
            .map_err(|_| MqttError::ProtocolError("Invalid server hostname".to_string()))?;

        let tls_stream = timeout(
            self.config.connect_timeout,
            connector.connect(domain, tcp_stream),
        )
        .await
        .map_err(|_| MqttError::Timeout)?
        .map_err(|e| MqttError::ConnectionError(format!("TLS handshake failed: {e}")))?;

        self.stream = Some(tls_stream);
        Ok(())
    }

    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        match &mut self.stream {
            Some(stream) => {
                let n = stream.read(buf).await?;
                if n == 0 {
                    return Err(MqttError::ConnectionError(
                        "Connection closed by remote".to_string(),
                    ));
                }
                Ok(n)
            }
            None => Err(MqttError::NotConnected),
        }
    }

    async fn write(&mut self, buf: &[u8]) -> Result<()> {
        match &mut self.stream {
            Some(stream) => {
                stream.write_all(buf).await?;
                stream.flush().await?;
                Ok(())
            }
            None => Err(MqttError::NotConnected),
        }
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(mut stream) = self.stream.take() {
            stream.shutdown().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_tls_config() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8883);
        let config = TlsConfig::new(addr, "localhost")
            .with_connect_timeout(Duration::from_secs(10))
            .with_system_roots(false)
            .with_verify_server_cert(false);

        assert_eq!(config.addr, addr);
        assert_eq!(config.hostname, "localhost");
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert!(!config.use_system_roots);
        assert!(!config.verify_server_cert);
        assert!(config.alpn_protocols.is_none());
    }

    #[test]
    fn test_tls_config_with_alpn() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8883);
        let config =
            TlsConfig::new(addr, "localhost").with_alpn_protocols(&["mqtt", "x-amzn-mqtt-ca"]);

        assert!(config.alpn_protocols.is_some());
        let protocols = config.alpn_protocols.unwrap();
        assert_eq!(protocols.len(), 2);
        assert_eq!(protocols[0], b"mqtt");
        assert_eq!(protocols[1], b"x-amzn-mqtt-ca");
    }

    #[test]
    fn test_tls_transport_creation() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8883);
        let transport = TlsTransport::new(TlsConfig::new(addr, "localhost"));

        assert!(!transport.is_connected());
        assert_eq!(transport.config.addr, addr);
    }

    #[tokio::test]
    async fn test_tls_connect_not_connected() {
        let mut transport = TlsTransport::new(TlsConfig::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8883),
            "localhost",
        ));

        // Try to read when not connected
        let mut buf = [0u8; 10];
        let result = transport.read(&mut buf).await;
        assert!(result.is_err());

        // Try to write when not connected
        let result = transport.write(b"test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tls_connect_real_broker() {
        use crate::packet::connect::ConnectPacket;
        use crate::packet::MqttPacket;
        use crate::protocol::v5::properties::Properties;

        let mut config = TlsConfig::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8884),
            "localhost",
        );

        // Load test certificates (skip if they don't exist)
        if config.load_ca_cert_pem("test_certs/ca.pem").is_err() {
            // Test certificates not available, skip the test
            return;
        }
        config.verify_server_cert = false; // For self-signed test certs

        let mut transport = TlsTransport::new(config);

        // Connect
        let result = transport.connect().await;
        assert!(
            result.is_ok(),
            "Failed to connect via TLS: {:?}",
            result.err()
        );
        assert!(transport.is_connected());

        // Write MQTT CONNECT packet using proper packet construction

        let connect = ConnectPacket {
            client_id: "tls_test".to_string(),
            keep_alive: 60,
            clean_start: true,
            will: None,
            username: None,
            password: None,
            properties: Properties::new(),
            protocol_version: 5,
            will_properties: Properties::new(),
        };

        let mut connect_bytes = Vec::new();
        let result = connect.encode(&mut connect_bytes);
        assert!(
            result.is_ok(),
            "Failed to encode CONNECT packet: {:?}",
            result.err()
        );

        let result = transport.write(&connect_bytes).await;
        assert!(result.is_ok());

        // Read response (should get CONNACK)
        let mut buf = [0u8; 256];
        let result = transport.read(&mut buf).await;
        assert!(result.is_ok());
        let n = result.unwrap();
        assert!(n > 0);
        assert_eq!(buf[0] >> 4, 2); // CONNACK packet type

        // Close connection
        let result = transport.close().await;
        assert!(result.is_ok());
        assert!(!transport.is_connected());
    }

    #[test]
    fn test_tls_config_load_files() {
        let mut config = TlsConfig::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8883),
            "localhost",
        );

        // These should fail on non-existent files
        assert!(config.load_client_cert_pem("non_existent.pem").is_err());
        assert!(config.load_client_key_pem("non_existent.pem").is_err());
        assert!(config.load_ca_cert_pem("non_existent.pem").is_err());
    }
}
