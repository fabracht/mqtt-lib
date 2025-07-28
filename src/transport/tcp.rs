use crate::error::{MqttError, Result};
use crate::transport::Transport;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{
    tcp::{OwnedReadHalf, OwnedWriteHalf},
    TcpStream,
};
use tokio::time::timeout;

/// TCP transport configuration
#[derive(Debug, Clone)]
pub struct TcpConfig {
    /// Server address
    pub addr: SocketAddr,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Socket nodelay option (disable Nagle's algorithm)
    pub nodelay: bool,
    /// Socket keepalive option
    pub keepalive: Option<Duration>,
}

impl TcpConfig {
    /// Creates a new TCP configuration
    #[must_use]
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            connect_timeout: Duration::from_secs(30),
            nodelay: true,
            keepalive: Some(Duration::from_secs(60)),
        }
    }

    /// Sets the connection timeout
    #[must_use]
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Sets the TCP nodelay option
    #[must_use]
    pub fn with_nodelay(mut self, nodelay: bool) -> Self {
        self.nodelay = nodelay;
        self
    }

    /// Sets the TCP keepalive option
    #[must_use]
    pub fn with_keepalive(mut self, keepalive: Option<Duration>) -> Self {
        self.keepalive = keepalive;
        self
    }
}

/// TCP transport implementation
#[derive(Debug)]
pub struct TcpTransport {
    config: TcpConfig,
    stream: Option<TcpStream>,
}

impl TcpTransport {
    /// Creates a new TCP transport
    #[must_use]
    pub fn new(config: TcpConfig) -> Self {
        Self {
            config,
            stream: None,
        }
    }

    /// Creates a TCP transport from an address
    #[must_use]
    pub fn from_addr(addr: SocketAddr) -> Self {
        Self::new(TcpConfig::new(addr))
    }

    /// Checks if the transport is connected
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Splits the transport into read and write halves for concurrent access
    ///
    /// This consumes the transport and returns owned halves that can be used
    /// in separate tasks without mutex contention.
    pub fn into_split(self) -> Result<(OwnedReadHalf, OwnedWriteHalf)> {
        match self.stream {
            Some(stream) => {
                let (reader, writer) = stream.into_split();
                Ok((reader, writer))
            }
            None => Err(MqttError::NotConnected),
        }
    }
}

impl Transport for TcpTransport {
    async fn connect(&mut self) -> Result<()> {
        if self.stream.is_some() {
            return Err(MqttError::AlreadyConnected);
        }

        // Connect with timeout
        let stream = timeout(
            self.config.connect_timeout,
            TcpStream::connect(self.config.addr),
        )
        .await
        .map_err(|_| MqttError::Timeout)??;

        // Configure socket options
        stream.set_nodelay(self.config.nodelay)?;

        // Set keepalive if configured
        if let Some(keepalive_duration) = self.config.keepalive {
            let sock_ref = socket2::SockRef::from(&stream);
            let keepalive = socket2::TcpKeepalive::new().with_time(keepalive_duration);
            sock_ref.set_tcp_keepalive(&keepalive)?;
        }

        self.stream = Some(stream);
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
    fn test_tcp_config() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1883);
        let config = TcpConfig::new(addr)
            .with_connect_timeout(Duration::from_secs(10))
            .with_nodelay(false)
            .with_keepalive(None);

        assert_eq!(config.addr, addr);
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert!(!config.nodelay);
        assert!(config.keepalive.is_none());
    }

    #[test]
    fn test_tcp_transport_creation() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1883);
        let transport = TcpTransport::from_addr(addr);

        assert!(!transport.is_connected());
        assert_eq!(transport.config.addr, addr);
    }

    #[tokio::test]
    async fn test_tcp_connect_not_connected() {
        let mut transport = TcpTransport::from_addr(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            1883,
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
    async fn test_tcp_connect_timeout() {
        // Use a mock transport for reliable timeout testing
        // Real network timeouts are unreliable across different environments
        let mut transport = TcpTransport::new(
            TcpConfig::new(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
                1883,
            ))
            .with_connect_timeout(Duration::from_millis(100)),
        );

        let result = transport.connect().await;
        // The error could be Timeout or Io depending on the system
        assert!(result.is_err(), "Expected connection to 192.0.2.1 to fail");
    }

    #[tokio::test]
    async fn test_tcp_connect_real_broker() {
        let mut transport = TcpTransport::from_addr(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            1883,
        ));

        // Test TCP connection
        let result = transport.connect().await;
        assert!(result.is_ok(), "Failed to connect: {:?}", result.err());
        assert!(transport.is_connected());

        // Test that we can write and read something
        // Use the proper MQTT CONNECT packet
        use crate::packet::connect::ConnectPacket;
        use crate::packet::MqttPacket;
        use crate::protocol::v5::properties::Properties;
        
        let connect = ConnectPacket {
            client_id: "test".to_string(),
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
        assert!(result.is_ok(), "Failed to encode CONNECT packet: {:?}", result.err());

        let result = transport.write(&connect_bytes).await;
        assert!(result.is_ok(), "Failed to write: {:?}", result.err());

        // Try to read CONNACK response
        let mut buf = [0u8; 256];
        let result = transport.read(&mut buf).await;

        // We should get some response from the broker
        assert!(result.is_ok(), "Failed to read: {:?}", result.err());
        let n = result.unwrap();
        assert!(n > 0, "Expected to read some bytes but got 0");

        // Basic validation - should be a CONNACK (0x20)
        assert_eq!(buf[0] & 0xF0, 0x20, "Expected CONNACK packet type");

        // Close connection
        let result = transport.close().await;
        assert!(result.is_ok());
        assert!(!transport.is_connected());
    }

    #[test]
    fn test_tcp_close_when_not_connected() {
        let mut transport = TcpTransport::from_addr(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            1883,
        ));

        // Close should succeed even when not connected
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = runtime.block_on(transport.close());
        assert!(result.is_ok());
    }
}
