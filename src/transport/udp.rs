use crate::error::{MqttError, Result};
use crate::packet::{FixedHeader, Packet};
use crate::transport::{packet_io::encode_packet_to_buffer, PacketReader, PacketWriter, Transport};
use bytes::BytesMut;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, trace, warn};

const MAX_UDP_PACKET_SIZE: usize = 65507;
const DEFAULT_MTU: usize = 1472;
const MESSAGE_CACHE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RETRIES: u32 = 4;
const INITIAL_RETRY_TIMEOUT: Duration = Duration::from_millis(2500);
const MAX_RETRY_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug, Clone)]
pub struct UdpConfig {
    pub addr: SocketAddr,
    pub connect_timeout: Duration,
    pub mtu: usize,
    pub enable_fragmentation: bool,
    pub max_retries: u32,
}

impl UdpConfig {
    #[must_use]
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            connect_timeout: Duration::from_secs(30),
            mtu: DEFAULT_MTU,
            enable_fragmentation: true,
            max_retries: MAX_RETRIES,
        }
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

    #[must_use]
    pub fn with_fragmentation(mut self, enable: bool) -> Self {
        self.enable_fragmentation = enable;
        self
    }
}

#[derive(Debug, Clone)]
pub struct FragmentHeader {
    pub packet_id: u16,
    pub fragment_index: u16,
    pub total_fragments: u16,
}

impl FragmentHeader {
    pub const SIZE: usize = 6;

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];
        bytes[0..2].copy_from_slice(&self.packet_id.to_be_bytes());
        bytes[2..4].copy_from_slice(&self.fragment_index.to_be_bytes());
        bytes[4..6].copy_from_slice(&self.total_fragments.to_be_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        Some(Self {
            packet_id: u16::from_be_bytes([bytes[0], bytes[1]]),
            fragment_index: u16::from_be_bytes([bytes[2], bytes[3]]),
            total_fragments: u16::from_be_bytes([bytes[4], bytes[5]]),
        })
    }
}

#[derive(Debug)]
struct FragmentReassembly {
    fragments: HashMap<u16, Vec<u8>>,
    total_fragments: u16,
    received_fragments: u16,
    started_at: Instant,
}

type MessageCache = Arc<Mutex<HashMap<(SocketAddr, u16), (Instant, Vec<u8>)>>>;
type FragmentCache = Arc<Mutex<HashMap<u16, FragmentReassembly>>>;

#[derive(Debug)]
pub struct UdpTransport {
    config: UdpConfig,
    socket: Option<Arc<UdpSocket>>,
    message_cache: MessageCache,
    fragment_reassembly: FragmentCache,
    next_packet_id: Arc<Mutex<u16>>,
}

impl UdpTransport {
    #[must_use]
    pub fn new(config: UdpConfig) -> Self {
        Self {
            config,
            socket: None,
            message_cache: Arc::new(Mutex::new(HashMap::new())),
            fragment_reassembly: Arc::new(Mutex::new(HashMap::new())),
            next_packet_id: Arc::new(Mutex::new(0)),
        }
    }

    #[must_use]
    pub fn from_addr(addr: SocketAddr) -> Self {
        Self::new(UdpConfig::new(addr))
    }

    #[must_use]
    pub fn remote_addr(&self) -> SocketAddr {
        self.config.addr
    }

    async fn ensure_connected(&mut self) -> Result<Arc<UdpSocket>> {
        if let Some(socket) = &self.socket {
            return Ok(socket.clone());
        }

        let bind_addr = match self.config.addr {
            SocketAddr::V4(_) => "0.0.0.0:0",
            SocketAddr::V6(_) => "[::]:0",
        };

        let socket = timeout(self.config.connect_timeout, UdpSocket::bind(bind_addr))
            .await
            .map_err(|_| MqttError::Timeout)?
            .map_err(|e| MqttError::Io(e.to_string()))?;

        socket.connect(self.config.addr).await?;

        let socket = Arc::new(socket);
        self.socket = Some(socket.clone());

        debug!("UDP transport connected to {}", self.config.addr);
        Ok(socket)
    }

    async fn send_raw(&self, data: &[u8]) -> Result<()> {
        let socket = self.socket.as_ref().ok_or(MqttError::NotConnected)?;

        socket.send(data).await?;
        Ok(())
    }

    async fn receive_raw(&self) -> Result<Vec<u8>> {
        let socket = self.socket.as_ref().ok_or(MqttError::NotConnected)?;

        let mut buf = vec![0u8; MAX_UDP_PACKET_SIZE];
        let len = socket.recv(&mut buf).await?;
        buf.truncate(len);

        Ok(buf)
    }

    async fn fragment_packet(&self, packet_bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
        if !self.config.enable_fragmentation {
            if packet_bytes.len() > self.config.mtu {
                return Err(MqttError::PacketTooLarge {
                    size: packet_bytes.len(),
                    max: self.config.mtu,
                });
            }
            return Ok(vec![packet_bytes.to_vec()]);
        }

        let max_payload_size = self.config.mtu - FragmentHeader::SIZE;
        if packet_bytes.len() <= max_payload_size {
            return Ok(vec![packet_bytes.to_vec()]);
        }

        let total_fragments =
            u16::try_from(packet_bytes.len().div_ceil(max_payload_size)).unwrap_or(u16::MAX);
        let mut packet_id = self.next_packet_id.lock().await;
        let current_packet_id = *packet_id;
        *packet_id = packet_id.wrapping_add(1);
        drop(packet_id);

        let mut fragments = Vec::new();
        for i in 0..total_fragments {
            let start = (i as usize) * max_payload_size;
            let end = ((i as usize + 1) * max_payload_size).min(packet_bytes.len());

            let header = FragmentHeader {
                packet_id: current_packet_id,
                fragment_index: i,
                total_fragments,
            };

            let mut fragment = Vec::with_capacity(FragmentHeader::SIZE + (end - start));
            fragment.extend_from_slice(&header.to_bytes());
            fragment.extend_from_slice(&packet_bytes[start..end]);

            fragments.push(fragment);
        }

        trace!("Fragmented packet into {} fragments", fragments.len());
        Ok(fragments)
    }

    async fn reassemble_fragment(&self, data: &[u8]) -> Result<Option<Vec<u8>>> {
        if data.len() < FragmentHeader::SIZE {
            return Ok(Some(data.to_vec()));
        }

        let Some(header) = FragmentHeader::from_bytes(data) else {
            return Ok(Some(data.to_vec()));
        };

        if header.total_fragments == 1 {
            return Ok(Some(data[FragmentHeader::SIZE..].to_vec()));
        }

        let payload = &data[FragmentHeader::SIZE..];
        let mut reassembly_map = self.fragment_reassembly.lock().await;

        reassembly_map.retain(|_, v| v.started_at.elapsed() < Duration::from_secs(30));

        let reassembly =
            reassembly_map
                .entry(header.packet_id)
                .or_insert_with(|| FragmentReassembly {
                    fragments: HashMap::new(),
                    total_fragments: header.total_fragments,
                    received_fragments: 0,
                    started_at: Instant::now(),
                });

        if reassembly.fragments.contains_key(&header.fragment_index) {
            trace!(
                "Duplicate fragment {} for packet {}",
                header.fragment_index,
                header.packet_id
            );
            return Ok(None);
        }

        reassembly
            .fragments
            .insert(header.fragment_index, payload.to_vec());
        reassembly.received_fragments += 1;

        if reassembly.received_fragments == reassembly.total_fragments {
            let mut complete_packet = Vec::new();
            for i in 0..reassembly.total_fragments {
                if let Some(fragment) = reassembly.fragments.get(&i) {
                    complete_packet.extend_from_slice(fragment);
                } else {
                    warn!("Missing fragment {} for packet {}", i, header.packet_id);
                    return Ok(None);
                }
            }

            reassembly_map.remove(&header.packet_id);
            trace!(
                "Reassembled complete packet {} ({} bytes)",
                header.packet_id,
                complete_packet.len()
            );
            Ok(Some(complete_packet))
        } else {
            trace!(
                "Received fragment {}/{} for packet {}",
                reassembly.received_fragments,
                reassembly.total_fragments,
                header.packet_id
            );
            Ok(None)
        }
    }

    async fn check_duplicate(
        &self,
        addr: SocketAddr,
        packet_id: u16,
        data: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        let mut cache = self.message_cache.lock().await;
        let now = Instant::now();

        cache.retain(|_, (timestamp, _)| now.duration_since(*timestamp) < MESSAGE_CACHE_TIMEOUT);

        let key = (addr, packet_id);
        if let Some((_, cached_data)) = cache.get(&key) {
            trace!("Duplicate packet detected for {:?}", key);
            return Ok(Some(cached_data.clone()));
        }

        cache.insert(key, (now, data.to_vec()));
        Ok(None)
    }
}

impl Transport for UdpTransport {
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
        let fragments = self.fragment_packet(buf).await?;

        for fragment in fragments {
            self.send_raw(&fragment).await?;
        }

        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        self.socket = None;
        debug!("UDP transport closed");
        Ok(())
    }
}

impl UdpTransport {
    pub fn into_split(self) -> Result<(UdpReadHalf, UdpWriteHalf)> {
        let socket = self.socket.ok_or(MqttError::NotConnected)?;

        let reader = UdpReadHalf::new(socket.clone(), self.config.mtu);
        let writer = UdpWriteHalf::new(socket, self.config.mtu);

        Ok((reader, writer))
    }
}

pub struct UdpReadHalf {
    socket: Arc<UdpSocket>,
    fragment_reassembly: Arc<Mutex<HashMap<u16, FragmentReassembly>>>,
    mtu: usize,
}

impl UdpReadHalf {
    pub fn new(socket: Arc<UdpSocket>, mtu: usize) -> Self {
        Self {
            socket,
            fragment_reassembly: Arc::new(Mutex::new(HashMap::new())),
            mtu,
        }
    }

    async fn reassemble_fragment(&self, data: &[u8]) -> Result<Option<Vec<u8>>> {
        if data.len() < FragmentHeader::SIZE {
            return Ok(Some(data.to_vec()));
        }

        let Some(header) = FragmentHeader::from_bytes(data) else {
            return Ok(Some(data.to_vec()));
        };

        if header.total_fragments == 1 {
            return Ok(Some(data[FragmentHeader::SIZE..].to_vec()));
        }

        let payload = &data[FragmentHeader::SIZE..];
        let mut reassembly_map = self.fragment_reassembly.lock().await;

        let reassembly =
            reassembly_map
                .entry(header.packet_id)
                .or_insert_with(|| FragmentReassembly {
                    fragments: HashMap::new(),
                    total_fragments: header.total_fragments,
                    received_fragments: 0,
                    started_at: Instant::now(),
                });

        reassembly
            .fragments
            .insert(header.fragment_index, payload.to_vec());
        reassembly.received_fragments += 1;

        if reassembly.received_fragments == reassembly.total_fragments {
            let mut complete_packet = Vec::new();
            for i in 0..reassembly.total_fragments {
                if let Some(fragment) = reassembly.fragments.get(&i) {
                    complete_packet.extend_from_slice(fragment);
                }
            }
            reassembly_map.remove(&header.packet_id);
            Ok(Some(complete_packet))
        } else {
            Ok(None)
        }
    }
}

impl PacketReader for UdpReadHalf {
    async fn read_packet(&mut self) -> Result<Packet> {
        let mut buf = vec![0u8; MAX_UDP_PACKET_SIZE];
        loop {
            let len = self.socket.recv(&mut buf).await?;
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

pub struct UdpWriteHalf {
    socket: Arc<UdpSocket>,
    next_packet_id: Arc<Mutex<u16>>,
    mtu: usize,
}

impl UdpWriteHalf {
    pub fn new(socket: Arc<UdpSocket>, mtu: usize) -> Self {
        Self {
            socket,
            next_packet_id: Arc::new(Mutex::new(0)),
            mtu,
        }
    }

    async fn fragment_packet(&self, packet_bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
        let max_payload_size = self.mtu - FragmentHeader::SIZE;
        if packet_bytes.len() <= max_payload_size {
            return Ok(vec![packet_bytes.to_vec()]);
        }

        let total_fragments =
            u16::try_from(packet_bytes.len().div_ceil(max_payload_size)).unwrap_or(u16::MAX);
        let mut packet_id = self.next_packet_id.lock().await;
        let current_packet_id = *packet_id;
        *packet_id = packet_id.wrapping_add(1);
        drop(packet_id);

        let mut fragments = Vec::new();
        for i in 0..total_fragments {
            let start = (i as usize) * max_payload_size;
            let end = ((i as usize + 1) * max_payload_size).min(packet_bytes.len());

            let header = FragmentHeader {
                packet_id: current_packet_id,
                fragment_index: i,
                total_fragments,
            };

            let mut fragment = Vec::with_capacity(FragmentHeader::SIZE + (end - start));
            fragment.extend_from_slice(&header.to_bytes());
            fragment.extend_from_slice(&packet_bytes[start..end]);

            fragments.push(fragment);
        }

        Ok(fragments)
    }
}

impl PacketWriter for UdpWriteHalf {
    async fn write_packet(&mut self, packet: Packet) -> Result<()> {
        let mut buf = BytesMut::with_capacity(1024);
        encode_packet_to_buffer(&packet, &mut buf)?;
        let fragments = self.fragment_packet(&buf).await?;

        for fragment in fragments {
            self.socket.send(&fragment).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fragment_header_roundtrip() {
        let header = FragmentHeader {
            packet_id: 12345,
            fragment_index: 3,
            total_fragments: 10,
        };

        let bytes = header.to_bytes();
        let decoded = FragmentHeader::from_bytes(&bytes).unwrap();

        assert_eq!(header.packet_id, decoded.packet_id);
        assert_eq!(header.fragment_index, decoded.fragment_index);
        assert_eq!(header.total_fragments, decoded.total_fragments);
    }

    #[tokio::test]
    async fn test_udp_config() {
        let addr = "127.0.0.1:1883".parse().unwrap();
        let config = UdpConfig::new(addr)
            .with_connect_timeout(Duration::from_secs(10))
            .with_mtu(2048)
            .with_fragmentation(false);

        assert_eq!(config.addr, addr);
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.mtu, 2048);
        assert!(!config.enable_fragmentation);
    }
}
