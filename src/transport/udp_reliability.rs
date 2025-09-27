//! UDP Reliability Layer
//!
//! Implements QUIC-inspired reliability mechanisms for MQTT over UDP.
//!
//! ## Packet Flow Architecture
//!
//! The UDP transport uses a layered approach for reliability:
//!
//! ### Sending (Client/Broker → Network):
//! 1. **MQTT Layer**: Generates MQTT protocol packets (CONNECT, PUBLISH, etc.)
//! 2. **Fragmentation Layer**: Splits large packets into MTU-sized fragments
//! 3. **Reliability Layer**: Wraps each fragment with sequence numbers and reliability headers
//! 4. **UDP Socket**: Sends the reliable packets over the network
//!
//! ### Receiving (Network → Client/Broker):
//! 1. **UDP Socket**: Receives packets from the network
//! 2. **Reliability Layer**: Unwraps reliability headers, handles ACKs, tracks sequences
//! 3. **Fragmentation Layer**: Reassembles fragments into complete MQTT packets
//! 4. **MQTT Layer**: Processes the complete MQTT protocol packets
//!
//! ### Key Features:
//! - **Sequence Numbers**: Each packet gets a unique sequence number for ordering and deduplication
//! - **Acknowledgments (ACKs)**: Receivers send ACKs to confirm packet receipt
//! - **Retransmission**: Unacknowledged packets are retransmitted with exponential backoff
//! - **RTT Calculation**: Round-trip time is tracked for adaptive timeout calculation
//! - **Duplicate Detection**: Sequence numbers prevent duplicate packet processing
//!
//! ### Packet Types:
//! - `0x01`: DATA - Contains actual MQTT payload data
//! - `0x02`: ACK - Acknowledges received packets with ranges
//! - `0x03`: HEARTBEAT - Keeps connection alive
//! - `0x04`: HEARTBEAT_ACK - Acknowledges heartbeat
//!
//! All UDP communication in this library uses this reliability layer - there is no
//! "unreliable" UDP mode. This ensures consistent behavior and prevents packet loss
//! issues common with raw UDP.

use crate::error::{MqttError, Result};
use bytes::{Buf, BufMut, BytesMut};
use std::collections::{hash_map::Entry, HashMap, VecDeque};
use std::time::{Duration, Instant};
use tracing::{debug, trace, warn};

const MAX_RETRIES: u8 = 5;
const INITIAL_RTO: Duration = Duration::from_millis(1000);
const MAX_RTO: Duration = Duration::from_secs(60);
const ACK_DELAY: Duration = Duration::from_millis(25);

#[derive(Debug, Clone)]
pub struct ReliablePacket {
    pub sequence: u64,
    pub data: Vec<u8>,
    pub sent_time: Instant,
    pub retry_count: u8,
    pub acked: bool,
}

#[derive(Debug)]
pub struct AckPacket {
    pub largest_acked: u64,
    pub ack_delay: Duration,
    pub ack_ranges: Vec<AckRange>,
}

#[derive(Debug)]
pub struct AckRange {
    pub gap: u64,
    pub ack_range_length: u64,
}

#[derive(Debug)]
pub struct UdpReliability {
    next_sequence: u64,
    next_expected: u64,
    unacked_packets: HashMap<u64, ReliablePacket>,
    received_packets: HashMap<u64, Instant>,
    ack_queue: VecDeque<u64>,
    srtt: Option<Duration>,
    rttvar: Duration,
    rto: Duration,
    max_ack_delay: Duration,
}

impl Default for UdpReliability {
    fn default() -> Self {
        Self::new()
    }
}

impl UdpReliability {
    pub fn new() -> Self {
        Self {
            next_sequence: 1,
            next_expected: 1,
            unacked_packets: HashMap::new(),
            received_packets: HashMap::new(),
            ack_queue: VecDeque::new(),
            srtt: None,
            rttvar: Duration::from_millis(250),
            rto: INITIAL_RTO,
            max_ack_delay: Duration::from_millis(25),
        }
    }

    pub fn wrap_packet(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);

        let packet = ReliablePacket {
            sequence,
            data: data.to_vec(),
            sent_time: Instant::now(),
            retry_count: 0,
            acked: false,
        };

        self.unacked_packets.insert(sequence, packet);

        let mut buf = BytesMut::with_capacity(9 + data.len());
        buf.put_u8(0x01); // Packet type: DATA
        buf.put_u64(sequence);
        buf.extend_from_slice(data);

        Ok(buf.to_vec())
    }

    pub fn unwrap_packet(&mut self, data: &[u8]) -> Result<Option<Vec<u8>>> {
        if data.is_empty() {
            return Ok(None);
        }

        let mut buf = BytesMut::from(data);
        let packet_type = buf.get_u8();

        match packet_type {
            0x01 => {
                // DATA packet
                if buf.remaining() < 8 {
                    return Err(MqttError::MalformedPacket("Invalid reliable packet".into()));
                }
                let sequence = buf.get_u64();
                let payload = buf.to_vec();

                // Check if we've seen this packet before
                match self.received_packets.entry(sequence) {
                    Entry::Occupied(_) => {
                        // Duplicate packet, ignore payload but still ACK it
                        trace!("Duplicate packet received: seq={}", sequence);
                        self.ack_queue.push_back(sequence);
                        Ok(None)
                    }
                    Entry::Vacant(e) => {
                        // Track received packet
                        e.insert(Instant::now());
                        self.ack_queue.push_back(sequence);

                        // Update next expected if this fills a gap
                        if sequence >= self.next_expected {
                            self.next_expected = sequence + 1;
                        }
                        Ok(Some(payload))
                    }
                }
            }
            0x02 => {
                // ACK packet
                self.process_ack(&mut buf)?;
                Ok(None)
            }
            0x03 => {
                // HEARTBEAT packet
                if buf.remaining() >= 8 {
                    let sequence = buf.get_u64();
                    self.send_heartbeat_ack(sequence);
                }
                Ok(None)
            }
            0x04 => {
                // HEARTBEAT_ACK packet
                // Process heartbeat acknowledgment
                Ok(None)
            }
            _ => {
                warn!("Unknown reliable packet type: {}", packet_type);
                Ok(None)
            }
        }
    }

    fn process_ack(&mut self, buf: &mut BytesMut) -> Result<()> {
        if buf.remaining() < 16 {
            warn!(
                "ACK packet too small: {} bytes remaining (need 16)",
                buf.remaining()
            );
            warn!("Buffer contents: {:02x?}", &buf[..buf.remaining().min(20)]);
            return Err(MqttError::MalformedPacket("Invalid ACK packet".into()));
        }

        let largest_acked = buf.get_u64();
        let ack_delay_us = buf.get_u64();
        let ack_delay = Duration::from_micros(ack_delay_us);

        // Mark packet as acknowledged
        if let Some(packet) = self.unacked_packets.get_mut(&largest_acked) {
            packet.acked = true;
            let rtt = packet.sent_time.elapsed().saturating_sub(ack_delay);
            self.update_rtt(rtt);
        }

        // Process ACK ranges if present
        while buf.remaining() >= 16 {
            let gap = buf.get_u64();
            let range_length = buf.get_u64();

            let mut seq = largest_acked.saturating_sub(gap + 1);
            for _ in 0..=range_length {
                if let Some(packet) = self.unacked_packets.get_mut(&seq) {
                    packet.acked = true;
                }
                seq = seq.saturating_sub(1);
            }
        }

        // Remove acknowledged packets
        self.unacked_packets.retain(|_, p| !p.acked);

        Ok(())
    }

    fn update_rtt(&mut self, latest_rtt: Duration) {
        match self.srtt {
            None => {
                self.srtt = Some(latest_rtt);
                self.rttvar = latest_rtt / 2;
            }
            Some(srtt) => {
                let rttvar_sample = srtt.abs_diff(latest_rtt);

                self.rttvar = (self.rttvar * 3 + rttvar_sample) / 4;
                self.srtt = Some((srtt * 7 + latest_rtt) / 8);
            }
        }

        // Update RTO based on RFC 6298
        let srtt = self.srtt.unwrap_or(INITIAL_RTO);
        self.rto = srtt + self.rttvar * 4;
        self.rto = self.rto.max(Duration::from_millis(200)).min(MAX_RTO);

        debug!(
            "RTT updated: SRTT={:?}, RTTVAR={:?}, RTO={:?}",
            srtt, self.rttvar, self.rto
        );
    }

    pub fn generate_ack(&mut self) -> Option<Vec<u8>> {
        if self.ack_queue.is_empty() {
            return None;
        }

        let mut sequences: Vec<_> = self.ack_queue.drain(..).collect();
        sequences.sort_unstable();
        sequences.dedup();

        if sequences.is_empty() {
            return None;
        }

        let largest_acked = *sequences.last()?;
        let ack_delay = self
            .received_packets
            .get(&largest_acked)
            .map_or(Duration::ZERO, Instant::elapsed);

        trace!(
            "Generating ACK: largest_acked={}, sequences={:?}",
            largest_acked,
            sequences
        );

        let mut buf = BytesMut::with_capacity(256);
        buf.put_u8(0x02); // ACK packet type
        buf.put_u64(largest_acked);
        buf.put_u64(ack_delay.as_micros().try_into().unwrap_or(u64::MAX));

        // Build ACK ranges
        let mut ranges = Vec::new();
        let mut current_start = largest_acked;
        let mut current_end = largest_acked;

        for &seq in sequences.iter().rev().skip(1) {
            if seq + 1 == current_start {
                current_start = seq;
            } else {
                let gap = current_end - seq - 1;
                let length = current_end - current_start;
                ranges.push((gap, length));
                current_start = seq;
                current_end = seq;
            }
        }

        // Add final range if needed
        if current_start != largest_acked || current_end != largest_acked {
            let length = current_end - current_start;
            ranges.push((0, length));
        }

        // Write ranges to buffer
        for (gap, length) in ranges {
            buf.put_u64(gap);
            buf.put_u64(length);
        }

        let result = buf.to_vec();

        // Ensure minimum ACK packet size (1 byte type + 8 bytes largest + 8 bytes delay = 17)
        if result.len() < 17 {
            warn!("Generated ACK packet too small: {} bytes", result.len());
            return None;
        }

        Some(result)
    }

    pub fn get_packets_to_retry(&mut self) -> Vec<Vec<u8>> {
        let now = Instant::now();
        let mut packets_to_retry = Vec::new();

        for packet in self.unacked_packets.values_mut() {
            if packet.retry_count >= MAX_RETRIES {
                warn!("Packet {} exceeded max retries", packet.sequence);
                continue;
            }

            let timeout = self.rto * 2u32.pow(packet.retry_count as u32);
            if now.duration_since(packet.sent_time) > timeout {
                packet.retry_count += 1;
                packet.sent_time = now;

                let mut buf = BytesMut::with_capacity(9 + packet.data.len());
                buf.put_u8(0x01); // DATA packet
                buf.put_u64(packet.sequence);
                buf.extend_from_slice(&packet.data);

                packets_to_retry.push(buf.to_vec());
                debug!(
                    "Retrying packet {} (attempt {})",
                    packet.sequence, packet.retry_count
                );
            }
        }

        packets_to_retry
    }

    pub fn generate_heartbeat(&mut self) -> Vec<u8> {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);

        let mut buf = BytesMut::with_capacity(9);
        buf.put_u8(0x03); // HEARTBEAT packet
        buf.put_u64(sequence);
        buf.to_vec()
    }

    fn send_heartbeat_ack(&self, sequence: u64) -> Vec<u8> {
        let mut buf = BytesMut::with_capacity(9);
        buf.put_u8(0x04); // HEARTBEAT_ACK packet
        buf.put_u64(sequence);
        buf.to_vec()
    }

    pub fn is_connection_alive(&self, timeout: Duration) -> bool {
        // Check if we've received any packets recently
        if let Some(latest) = self.received_packets.values().max() {
            latest.elapsed() < timeout
        } else {
            false
        }
    }

    pub fn stats(&self) -> ReliabilityStats {
        ReliabilityStats {
            unacked_count: self.unacked_packets.len(),
            srtt: self.srtt,
            rto: self.rto,
            next_sequence: self.next_sequence,
        }
    }
}

#[derive(Debug)]
pub struct ReliabilityStats {
    pub unacked_count: usize,
    pub srtt: Option<Duration>,
    pub rto: Duration,
    pub next_sequence: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ack_generation_and_parsing() {
        let mut sender = UdpReliability::new();
        let mut receiver = UdpReliability::new();

        // Sender sends a DATA packet
        let data = b"test data";
        let wrapped = sender.wrap_packet(data).unwrap();
        println!("Wrapped packet: {:02x?}", wrapped);

        // Receiver processes the DATA packet
        let unwrapped = receiver.unwrap_packet(&wrapped).unwrap();
        assert!(unwrapped.is_some());
        assert_eq!(unwrapped.unwrap(), data);

        // Receiver generates ACK
        let ack_packet = receiver.generate_ack();
        assert!(
            ack_packet.is_some(),
            "Should generate ACK after receiving data"
        );
        let ack_packet = ack_packet.unwrap();
        println!(
            "ACK packet: {:02x?} ({} bytes)",
            ack_packet,
            ack_packet.len()
        );

        // Verify ACK packet has minimum size
        assert!(
            ack_packet.len() >= 17,
            "ACK packet too small: {} bytes",
            ack_packet.len()
        );

        // Sender processes the ACK
        let ack_result = sender.unwrap_packet(&ack_packet).unwrap();
        assert!(
            ack_result.is_none(),
            "ACK should return None (control packet)"
        );

        // Verify packet was acknowledged
        assert_eq!(
            sender.unacked_packets.len(),
            0,
            "Packet should be acknowledged"
        );
    }
}
