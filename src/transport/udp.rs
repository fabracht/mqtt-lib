use crate::error::{MqttError, Result};
use crate::packet::{FixedHeader, Packet};
use crate::packet_id::PacketIdGenerator;
use crate::transport::udp_fragmentation::{fragment_packet, FragmentReassembler};
use crate::transport::udp_reliability::UdpReliability;
use crate::transport::{packet_io::encode_packet_to_buffer, PacketReader, PacketWriter, Transport};
use bytes::BytesMut;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{interval, timeout};
use tracing::{debug, trace, warn};

// UDP transport constants
const MAX_UDP_PACKET_SIZE: usize = 65507;
const DEFAULT_MTU: usize = 1472; // Standard Ethernet MTU minus IP/UDP headers
const MESSAGE_CACHE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RETRIES: u32 = 4;
const INITIAL_RETRY_TIMEOUT: Duration = Duration::from_millis(2500);
const MAX_RETRY_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug, Clone)]
pub struct UdpConfig {
    pub addr: SocketAddr,
    pub connect_timeout: Duration,
    pub mtu: usize,
    pub max_retries: u32,
}

impl UdpConfig {
    #[must_use]
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            connect_timeout: Duration::from_secs(30),
            mtu: DEFAULT_MTU,
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
}

// FragmentHeader and FragmentReassembly are now imported from udp_fragmentation module

type MessageCache = Arc<Mutex<HashMap<(SocketAddr, u16), (Instant, Vec<u8>)>>>;

/// UDP transport implementation with reliability and intelligent fragmentation
///
/// This transport provides MQTT-over-UDP with two layers:
/// 1. **Reliability layer** (always applied): Sequence numbers, ACKs, retransmission
/// 2. **Fragmentation layer** (intelligent): Only adds headers for packets > MTU
///
/// See `udp_fragmentation` and `udp_reliability` modules for detailed documentation.
pub struct UdpTransport {
    config: UdpConfig,
    socket: Option<Arc<UdpSocket>>,
    message_cache: MessageCache,
    fragment_reassembler: Arc<Mutex<FragmentReassembler>>,
    packet_id_generator: PacketIdGenerator,
    read_buffer: Arc<Mutex<Option<Vec<u8>>>>,
    read_offset: Arc<Mutex<usize>>,
    reliability: Arc<Mutex<UdpReliability>>,
    reliability_task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl UdpTransport {
    #[must_use]
    pub fn new(config: UdpConfig) -> Self {
        Self {
            config,
            socket: None,
            message_cache: Arc::new(Mutex::new(HashMap::new())),
            fragment_reassembler: Arc::new(Mutex::new(FragmentReassembler::new())),
            packet_id_generator: PacketIdGenerator::new(),
            read_buffer: Arc::new(Mutex::new(None)),
            read_offset: Arc::new(Mutex::new(0)),
            reliability: Arc::new(Mutex::new(UdpReliability::new())),
            reliability_task: Arc::new(Mutex::new(None)),
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

        let local_addr = socket
            .local_addr()
            .unwrap_or_else(|_| "unknown".parse().unwrap());
        debug!(
            "UDP transport connected from {} to {}",
            local_addr, self.config.addr
        );
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
        debug!(
            "UDP client waiting to receive from connected address {}",
            self.config.addr
        );
        let len = socket.recv(&mut buf).await?;
        buf.truncate(len);
        debug!("UDP client received {} bytes", len);

        Ok(buf)
    }

    fn fragment_packet(&self, packet_bytes: &[u8]) -> Vec<Vec<u8>> {
        // Get next packet ID for fragmentation
        let packet_id = self.packet_id_generator.next();

        // Use the shared fragmentation function which intelligently decides:
        // - Small packets (≤ MTU-6): returned as-is without headers
        // - Large packets (> MTU-6): fragmented with 6-byte headers
        fragment_packet(packet_bytes, self.config.mtu, packet_id)
    }

    async fn reassemble_fragment(&self, data: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut reassembler = self.fragment_reassembler.lock().await;
        reassembler.reassemble_fragment(data)
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
        // Check if we have buffered data from a previous datagram
        let mut read_buffer = self.read_buffer.lock().await;
        let mut read_offset = self.read_offset.lock().await;

        if let Some(ref buffer) = *read_buffer {
            let remaining = buffer.len() - *read_offset;
            if remaining > 0 {
                // Read from the existing buffer
                let to_read = remaining.min(buf.len());
                buf[..to_read].copy_from_slice(&buffer[*read_offset..*read_offset + to_read]);
                *read_offset += to_read;

                // Clear the buffer if we've read it all
                if *read_offset >= buffer.len() {
                    *read_buffer = None;
                    *read_offset = 0;
                }

                return Ok(to_read);
            }
        }

        // No buffered data, receive a new datagram
        loop {
            let data = self.receive_raw().await?;

            // Always process through reliability layer
            let mut reliability = self.reliability.lock().await;
            let Some(processed_data) = reliability.unwrap_packet(&data)? else {
                // This was a reliability control packet (ACK, heartbeat, etc.), continue waiting
                continue;
            };

            if let Some(complete_packet) = self.reassemble_fragment(&processed_data).await? {
                // For UDP, we need to buffer the complete packet for byte-by-byte reading
                let to_read = complete_packet.len().min(buf.len());
                buf[..to_read].copy_from_slice(&complete_packet[..to_read]);

                // Store the rest for future reads if needed
                if complete_packet.len() > buf.len() {
                    *read_buffer = Some(complete_packet);
                    *read_offset = buf.len();
                } else {
                    *read_buffer = None;
                    *read_offset = 0;
                }

                return Ok(to_read);
            }
        }
    }

    async fn write(&mut self, buf: &[u8]) -> Result<()> {
        let fragments = self.fragment_packet(buf);

        for fragment in fragments {
            // Always wrap with reliability layer
            let mut reliability = self.reliability.lock().await;
            let data_to_send = reliability.wrap_packet(&fragment)?;

            self.send_raw(&data_to_send).await?;
        }

        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        // Abort the reliability background task
        let mut task_lock = self.reliability_task.lock().await;
        if let Some(handle) = task_lock.take() {
            handle.abort();
            debug!("UDP reliability task aborted");
        }

        self.socket = None;
        debug!("UDP transport closed");
        Ok(())
    }
}

impl UdpTransport {
    pub fn into_split(self) -> Result<(UdpReadHalf, UdpWriteHalf)> {
        let socket = self.socket.ok_or(MqttError::NotConnected)?;

        let socket_for_task = socket.clone();
        let reliability_for_task = self.reliability.clone();

        let handle = tokio::spawn(async move {
            const MAX_CONSECUTIVE_ERRORS: u32 = 10;
            let mut ticker = interval(Duration::from_millis(100));
            let mut consecutive_errors = 0;

            loop {
                ticker.tick().await;

                let mut rel = reliability_for_task.lock().await;

                if let Some(ack_packet) = rel.generate_ack() {
                    match socket_for_task.send(&ack_packet).await {
                        Ok(_) => {
                            consecutive_errors = 0;
                        }
                        Err(e) => {
                            consecutive_errors += 1;
                            if consecutive_errors == 1 {
                                warn!("Failed to send ACK packet: {}", e);
                            } else if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                debug!(
                                    "Too many consecutive errors ({}), stopping reliability task",
                                    consecutive_errors
                                );
                                break;
                            }
                        }
                    }
                }

                let retry_packets = rel.get_packets_to_retry();
                if retry_packets.is_empty() {
                    consecutive_errors = consecutive_errors.saturating_sub(1);
                }

                for retry_packet in retry_packets {
                    match socket_for_task.send(&retry_packet).await {
                        Ok(_) => {
                            consecutive_errors = 0;
                        }
                        Err(e) => {
                            consecutive_errors += 1;
                            if consecutive_errors == 1 {
                                warn!("Failed to send retry packet: {}", e);
                            } else if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                debug!(
                                    "Too many consecutive errors ({}), stopping reliability task",
                                    consecutive_errors
                                );
                                break;
                            }
                        }
                    }
                }

                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    break;
                }
            }

            debug!("UDP reliability task (from into_split) terminated");
        });

        let reader = UdpReadHalf::new(
            socket.clone(),
            self.config.mtu,
            self.reliability.clone(),
            Some(handle),
        );
        let writer = UdpWriteHalf::new(
            socket,
            self.config.mtu,
            self.reliability,
            self.packet_id_generator,
        );

        Ok((reader, writer))
    }
}

pub struct UdpReadHalf {
    socket: Arc<UdpSocket>,
    fragment_reassembler: Arc<Mutex<FragmentReassembler>>,
    mtu: usize,
    reliability: Arc<Mutex<UdpReliability>>,
    reliability_task: Option<JoinHandle<()>>,
}

impl UdpReadHalf {
    pub fn new(
        socket: Arc<UdpSocket>,
        mtu: usize,
        reliability: Arc<Mutex<UdpReliability>>,
        reliability_task: Option<JoinHandle<()>>,
    ) -> Self {
        Self {
            socket,
            fragment_reassembler: Arc::new(Mutex::new(FragmentReassembler::new())),
            mtu,
            reliability,
            reliability_task,
        }
    }

    async fn reassemble_fragment(&self, unwrapped_data: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut reassembler = self.fragment_reassembler.lock().await;
        reassembler.reassemble_fragment(unwrapped_data)
    }
}

impl Drop for UdpReadHalf {
    fn drop(&mut self) {
        if let Some(handle) = self.reliability_task.take() {
            handle.abort();
            debug!("Aborted reliability task from UdpReadHalf drop");
        }
    }
}

impl PacketReader for UdpReadHalf {
    async fn read_packet(&mut self) -> Result<Packet> {
        loop {
            let mut buf = vec![0u8; MAX_UDP_PACKET_SIZE];
            let len = self.socket.recv(&mut buf).await?;
            buf.truncate(len);

            // First unwrap reliability layer
            let mut reliability = self.reliability.lock().await;
            let Some(unwrapped_data) = reliability.unwrap_packet(&buf)? else {
                // This was a control packet (ACK, heartbeat, etc.), continue waiting
                continue;
            };
            drop(reliability);

            // Then handle fragmentation
            if let Some(complete_packet) = self.reassemble_fragment(&unwrapped_data).await? {
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
    packet_id_generator: PacketIdGenerator,
    mtu: usize,
    reliability: Arc<Mutex<UdpReliability>>,
}

impl UdpWriteHalf {
    pub fn new(
        socket: Arc<UdpSocket>,
        mtu: usize,
        reliability: Arc<Mutex<UdpReliability>>,
        packet_id_generator: PacketIdGenerator,
    ) -> Self {
        Self {
            socket,
            packet_id_generator,
            mtu,
            reliability,
        }
    }

    fn fragment_packet(&self, packet_bytes: &[u8]) -> Vec<Vec<u8>> {
        // Get next packet ID for fragmentation
        let packet_id = self.packet_id_generator.next();

        // Use the shared fragmentation function which intelligently decides:
        // - Small packets (≤ MTU-6): returned as-is without headers
        // - Large packets (> MTU-6): fragmented with 6-byte headers
        fragment_packet(packet_bytes, self.mtu, packet_id)
    }
}

impl PacketWriter for UdpWriteHalf {
    async fn write_packet(&mut self, packet: Packet) -> Result<()> {
        let mut buf = BytesMut::with_capacity(1024);
        encode_packet_to_buffer(&packet, &mut buf)?;
        let fragments = self.fragment_packet(&buf);

        for fragment in fragments {
            // Wrap with reliability layer
            let mut reliability = self.reliability.lock().await;
            let wrapped = reliability.wrap_packet(&fragment)?;
            drop(reliability);

            self.socket.send(&wrapped).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::udp_fragmentation::FragmentHeader;

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
            .with_mtu(2048);

        assert_eq!(config.addr, addr);
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.mtu, 2048);
    }
}
