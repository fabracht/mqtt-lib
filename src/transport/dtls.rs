use crate::error::{MqttError, Result};
use crate::packet::{FixedHeader, Packet};
use crate::packet_id::PacketIdGenerator;
use crate::transport::{
    packet_io::encode_packet_to_buffer,
    udp_fragmentation::{fragment_packet, FragmentReassembler},
    PacketReader, PacketWriter, Transport,
};
use bytes::BytesMut;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, error};
use webrtc_dtls::cipher_suite::CipherSuiteId;
use webrtc_dtls::config::Config as WebRtcDtlsConfig;
use webrtc_dtls::conn::DTLSConn;
use webrtc_dtls::crypto::Certificate;
use webrtc_util::Conn;

const MAX_UDP_PACKET_SIZE: usize = 65507;
const DEFAULT_MTU: usize = 1472;
const MESSAGE_CACHE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct DtlsConfig {
    pub addr: SocketAddr,
    pub connect_timeout: Duration,
    pub mtu: usize,
    pub psk_identity: Option<Vec<u8>>,
    pub psk_key: Option<Vec<u8>>,
    pub cert_pem: Option<Vec<u8>>,
    pub key_pem: Option<Vec<u8>>,
    pub ca_cert_pem: Option<Vec<u8>>,
}

impl DtlsConfig {
    #[must_use]
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            connect_timeout: Duration::from_secs(30),
            mtu: DEFAULT_MTU,
            psk_identity: None,
            psk_key: None,
            cert_pem: None,
            key_pem: None,
            ca_cert_pem: None,
        }
    }

    #[must_use]
    pub fn with_psk(mut self, identity: Vec<u8>, key: Vec<u8>) -> Self {
        self.psk_identity = Some(identity);
        self.psk_key = Some(key);
        self
    }

    #[must_use]
    pub fn with_certificates(mut self, cert: Vec<u8>, key: Vec<u8>, ca: Option<Vec<u8>>) -> Self {
        self.cert_pem = Some(cert);
        self.key_pem = Some(key);
        self.ca_cert_pem = ca;
        self
    }

    #[must_use]
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    #[must_use]
    pub fn with_mtu(mut self, mtu: usize) -> Self {
        self.mtu = mtu.min(MAX_UDP_PACKET_SIZE);
        self
    }
}

type MessageCache = Arc<Mutex<HashMap<(SocketAddr, u16), (Instant, Vec<u8>)>>>;

pub struct DtlsTransport {
    config: DtlsConfig,
    conn: Option<Arc<dyn Conn + Send + Sync>>,
    message_cache: MessageCache,
    fragment_reassembler: Arc<Mutex<FragmentReassembler>>,
    packet_id_generator: PacketIdGenerator,
}

impl DtlsTransport {
    #[must_use]
    pub fn new(config: DtlsConfig) -> Self {
        Self {
            config,
            conn: None,
            message_cache: Arc::new(Mutex::new(HashMap::new())),
            fragment_reassembler: Arc::new(Mutex::new(FragmentReassembler::new())),
            packet_id_generator: PacketIdGenerator::new(),
        }
    }

    #[must_use]
    pub fn remote_addr(&self) -> SocketAddr {
        self.config.addr
    }

    fn create_dtls_config(&self) -> Result<WebRtcDtlsConfig> {
        if let (Some(identity), Some(key)) = (&self.config.psk_identity, &self.config.psk_key) {
            let key_clone = key.clone();
            let psk_callback = Arc::new(
                move |_hint: &[u8]| -> std::result::Result<Vec<u8>, webrtc_dtls::Error> {
                    Ok(key_clone.clone())
                },
            );

            Ok(WebRtcDtlsConfig {
                psk: Some(psk_callback),
                psk_identity_hint: Some(identity.clone()),
                cipher_suites: vec![
                    CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8,
                    CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256,
                ],
                ..Default::default()
            })
        } else if let (Some(cert), Some(key)) = (&self.config.cert_pem, &self.config.key_pem) {
            let cert_str = String::from_utf8_lossy(cert);
            let key_str = String::from_utf8_lossy(key);
            let combined_pem = format!("{}{}", key_str, cert_str);

            let certificate = Certificate::from_pem(&combined_pem)
                .map_err(|e| MqttError::ConnectionError(format!("Invalid certificate: {e}")))?;

            let config = WebRtcDtlsConfig {
                certificates: vec![certificate],
                cipher_suites: vec![
                    CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256,
                    CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256,
                    CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm,
                    CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm_8,
                ],
                insecure_skip_verify: self.config.ca_cert_pem.is_none(),
                ..Default::default()
            };

            if self.config.ca_cert_pem.is_some() {
                debug!("CA certificate provided for DTLS verification");
            }

            Ok(config)
        } else {
            Err(MqttError::ConnectionError(
                "DTLS transport requires either PSK or certificate configuration".to_string(),
            ))
        }
    }

    async fn ensure_connected(&mut self) -> Result<Arc<dyn Conn + Send + Sync>> {
        if let Some(conn) = &self.conn {
            return Ok(conn.clone());
        }

        let bind_addr = match self.config.addr {
            SocketAddr::V4(_) => "0.0.0.0:0",
            SocketAddr::V6(_) => "[::]:0",
        };

        let udp_socket = timeout(self.config.connect_timeout, UdpSocket::bind(bind_addr))
            .await
            .map_err(|_| MqttError::Timeout)?
            .map_err(|e| MqttError::Io(e.to_string()))?;

        udp_socket
            .connect(self.config.addr)
            .await
            .map_err(|e| MqttError::Io(e.to_string()))?;

        let udp_conn: Arc<dyn Conn + Send + Sync> = Arc::new(udp_socket);
        let dtls_config = self.create_dtls_config()?;

        let dtls_conn = timeout(
            self.config.connect_timeout,
            DTLSConn::new(udp_conn, dtls_config, true, None),
        )
        .await
        .map_err(|_| MqttError::Timeout)?
        .map_err(|e| {
            MqttError::ConnectionError(format!("Failed to create DTLS connection: {e}"))
        })?;

        let conn: Arc<dyn Conn + Send + Sync> = Arc::new(dtls_conn);
        self.conn = Some(conn.clone());

        debug!("DTLS transport connected to {}", self.config.addr);
        Ok(conn)
    }

    async fn send_raw(&self, data: &[u8]) -> Result<()> {
        let conn = self.conn.as_ref().ok_or(MqttError::NotConnected)?;

        conn.send(data)
            .await
            .map_err(|e| MqttError::Io(e.to_string()))?;
        Ok(())
    }

    async fn receive_raw(&self) -> Result<Vec<u8>> {
        let conn = self.conn.as_ref().ok_or(MqttError::NotConnected)?;

        let mut buf = vec![0u8; MAX_UDP_PACKET_SIZE];
        let len = conn
            .recv(&mut buf)
            .await
            .map_err(|e| MqttError::Io(e.to_string()))?;
        buf.truncate(len);

        Ok(buf)
    }

    fn fragment_packet(&self, packet_bytes: &[u8]) -> Vec<Vec<u8>> {
        let packet_id = self.packet_id_generator.next();

        fragment_packet(packet_bytes, self.config.mtu, packet_id)
    }

    async fn reassemble_fragment(&self, data: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut reassembler = self.fragment_reassembler.lock().await;
        reassembler.reassemble_fragment(data)
    }
}

impl Transport for DtlsTransport {
    async fn connect(&mut self) -> Result<()> {
        self.ensure_connected().await?;
        Ok(())
    }

    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            let data = self.receive_raw().await?;

            if let Some(complete_packet) = self.reassemble_fragment(&data).await? {
                let len = complete_packet.len().min(buf.len());
                buf[..len].copy_from_slice(&complete_packet[..len]);
                return Ok(len);
            }
        }
    }

    async fn write(&mut self, buf: &[u8]) -> Result<()> {
        let fragments = self.fragment_packet(buf);

        for fragment in fragments {
            self.send_raw(&fragment).await?;
        }

        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(conn) = self.conn.take() {
            if let Err(e) = conn.close().await {
                error!("Error closing DTLS connection: {}", e);
            }
        }
        debug!("DTLS transport closed");
        Ok(())
    }
}

impl DtlsTransport {
    pub fn into_split(self) -> Result<(DtlsReadHalf, DtlsWriteHalf)> {
        let conn = self.conn.ok_or(MqttError::NotConnected)?;

        let reader = DtlsReadHalf::new(conn.clone(), self.config.mtu);
        let writer = DtlsWriteHalf::new(conn, self.config.mtu, self.packet_id_generator);

        Ok((reader, writer))
    }
}

pub struct DtlsReadHalf {
    conn: Arc<dyn Conn + Send + Sync>,
    fragment_reassembler: Arc<Mutex<FragmentReassembler>>,
    mtu: usize,
}

impl DtlsReadHalf {
    pub fn new(conn: Arc<dyn Conn + Send + Sync>, mtu: usize) -> Self {
        Self {
            conn,
            fragment_reassembler: Arc::new(Mutex::new(FragmentReassembler::new())),
            mtu,
        }
    }

    async fn reassemble_fragment(&self, data: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut reassembler = self.fragment_reassembler.lock().await;
        reassembler.reassemble_fragment(data)
    }
}

impl PacketReader for DtlsReadHalf {
    async fn read_packet(&mut self) -> Result<Packet> {
        let mut buf = vec![0u8; MAX_UDP_PACKET_SIZE];
        loop {
            let len = self
                .conn
                .recv(&mut buf)
                .await
                .map_err(|e| MqttError::Io(e.to_string()))?;
            buf.truncate(len);

            if let Some(complete_packet) = self.reassemble_fragment(&buf).await? {
                // Parse packet from bytes
                let mut bytes = BytesMut::from(&complete_packet[..]);
                let fixed_header = FixedHeader::decode(&mut bytes)?;
                let packet =
                    Packet::decode_from_body(fixed_header.packet_type, &fixed_header, &mut bytes)?;
                return Ok(packet);
            }
        }
    }
}

pub struct DtlsWriteHalf {
    conn: Arc<dyn Conn + Send + Sync>,
    packet_id_generator: PacketIdGenerator,
    mtu: usize,
}

impl DtlsWriteHalf {
    pub fn new(
        conn: Arc<dyn Conn + Send + Sync>,
        mtu: usize,
        packet_id_generator: PacketIdGenerator,
    ) -> Self {
        Self {
            conn,
            packet_id_generator,
            mtu,
        }
    }

    fn fragment_packet(&self, packet_bytes: &[u8]) -> Vec<Vec<u8>> {
        let packet_id = self.packet_id_generator.next();

        fragment_packet(packet_bytes, self.mtu, packet_id)
    }
}

impl PacketWriter for DtlsWriteHalf {
    async fn write_packet(&mut self, packet: Packet) -> Result<()> {
        let mut buf = BytesMut::with_capacity(1024);
        encode_packet_to_buffer(&packet, &mut buf)?;
        let fragments = self.fragment_packet(&buf);

        for fragment in fragments {
            self.conn
                .send(&fragment)
                .await
                .map_err(|e| MqttError::Io(e.to_string()))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dtls_config() {
        let addr = "127.0.0.1:8883".parse().unwrap();
        let config = DtlsConfig::new(addr)
            .with_psk(b"identity".to_vec(), b"secret_key".to_vec())
            .with_connect_timeout(Duration::from_secs(10))
            .with_mtu(2048);

        assert_eq!(config.addr, addr);
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.mtu, 2048);
        assert!(config.psk_identity.is_some());
        assert!(config.psk_key.is_some());
    }
}
