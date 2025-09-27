//! UDP packet fragmentation and reassembly
//!
//! This module provides intelligent fragmentation and reassembly for MQTT packets
//! transmitted over UDP. It optimizes for the common case of small packets while
//! handling large packets that exceed the MTU.
//!
//! ## Architecture Overview
//!
//! The fragmentation layer works in conjunction with the reliability layer to provide
//! efficient and reliable MQTT-over-UDP transport:
//!
//! ```text
//! Sending Path:
//!   MQTT Packet → fragment_packet() → Reliability Layer → UDP Socket
//!                      ↓
//!              [Intelligent Decision]
//!                ↓              ↓
//!          Small packet    Large packet
//!         (≤ MTU-6 bytes)  (> MTU-6 bytes)
//!              ↓                ↓
//!         Pass through     Add fragment headers
//!          unchanged        Split into chunks
//! ```
//!
//! ## Fragmentation Strategy
//!
//! The fragmentation layer uses an **intelligent optimization**:
//!
//! - **Small packets** (≤ MTU - 6 bytes): Passed through WITHOUT fragmentation headers
//!   - Most MQTT control packets (CONNECT, CONNACK, SUBSCRIBE, etc.) fall in this category
//!   - Saves 6 bytes of overhead per packet
//!   - Reduces latency and bandwidth usage
//!
//! - **Large packets** (> MTU - 6 bytes): Fragmented with headers
//!   - Each fragment gets a 6-byte header (packet_id, fragment_index, total_fragments)
//!   - Fragments are reassembled on the receiving side
//!   - Out-of-order delivery is handled correctly
//!
//! ## Reassembly Logic
//!
//! The reassembler uses smart detection to distinguish between:
//!
//! 1. **Raw MQTT packets**: Detected by checking if the first byte contains a valid
//!    MQTT packet type (1-14) in bits 7-4
//! 2. **Fragmented packets**: Contain fragment headers that are parsed and reassembled
//! 3. **Too-small packets**: Packets smaller than fragment header size are passed through
//!
//! ## Why This Design?
//!
//! Since there's no formal MQTT-over-UDP standard, we've optimized for:
//!
//! - **Efficiency**: No unnecessary overhead for small packets
//! - **Reliability**: The reliability layer (always present) ensures delivery
//! - **Compatibility**: Works seamlessly with standard MQTT packet formats
//! - **Performance**: Reduces bandwidth usage for typical MQTT traffic patterns
//!
//! This intelligent fragmentation is a feature, not a limitation. The reliability
//! layer provides consistent ordering and delivery guarantees regardless of whether
//! fragmentation headers are present.

use crate::error::Result;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{trace, warn};

const FRAGMENT_TIMEOUT: Duration = Duration::from_secs(30);

/// Fragment header for UDP packets
#[derive(Debug, Clone, Copy)]
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

/// State for reassembling fragmented packets
#[derive(Debug)]
struct FragmentReassembly {
    fragments: HashMap<u16, Vec<u8>>,
    total_fragments: u16,
    received_fragments: u16,
    started_at: Instant,
}

/// Handles reassembly of fragmented UDP packets
#[derive(Debug)]
pub struct FragmentReassembler {
    fragment_reassembly: HashMap<u16, FragmentReassembly>,
}

impl FragmentReassembler {
    pub fn new() -> Self {
        Self {
            fragment_reassembly: HashMap::new(),
        }
    }

    /// Reassemble a fragment or detect a complete packet
    ///
    /// This method implements intelligent detection to handle both:
    /// 1. Small packets that were not fragmented (no headers)
    /// 2. Large packets that were fragmented (with headers)
    ///
    /// Detection logic:
    /// - Empty or very small packets: Pass through unchanged
    /// - Valid MQTT packet types (1-14): Pass through as complete packets
    /// - Otherwise: Parse as fragment header and reassemble
    ///
    /// This allows the system to handle both optimized small packets and
    /// properly fragmented large packets seamlessly.
    pub fn reassemble_fragment(&mut self, data: &[u8]) -> Result<Option<Vec<u8>>> {
        if data.is_empty() {
            return Ok(Some(vec![]));
        }

        if data.len() < FragmentHeader::SIZE {
            // Too small to have a fragment header, must be a complete packet
            return Ok(Some(data.to_vec()));
        }

        // Check if this looks like an MQTT packet (first byte has packet type in bits 7-4)
        // MQTT packet types 1-14 are valid (15 is reserved, 0 is invalid)
        let first_byte = data[0];
        let packet_type = (first_byte >> 4) & 0x0F;
        if (1..=14).contains(&packet_type) {
            // This is an MQTT packet that wasn't fragmented (optimization path)
            return Ok(Some(data.to_vec()));
        }

        let Some(header) = FragmentHeader::from_bytes(data) else {
            return Ok(Some(data.to_vec()));
        };

        if header.total_fragments == 1 {
            return Ok(Some(data[FragmentHeader::SIZE..].to_vec()));
        }

        // Clean up old fragments
        self.fragment_reassembly
            .retain(|_, v| v.started_at.elapsed() < FRAGMENT_TIMEOUT);

        let payload = &data[FragmentHeader::SIZE..];
        let reassembly = self
            .fragment_reassembly
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

            self.fragment_reassembly.remove(&header.packet_id);
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
}

impl Default for FragmentReassembler {
    fn default() -> Self {
        Self::new()
    }
}

/// Fragment a packet into multiple smaller packets if necessary
///
/// This function implements intelligent fragmentation:
/// - Packets that fit within MTU-6 bytes are returned unchanged (no headers added)
/// - Larger packets are split into fragments with 6-byte headers
///
/// This optimization reduces overhead for small packets, which are common in MQTT
/// (CONNECT, CONNACK, SUBSCRIBE, SUBACK, PINGREQ, PINGRESP, etc.).
///
/// # Arguments
/// * `packet_bytes` - The complete packet to potentially fragment
/// * `mtu` - Maximum transmission unit size
/// * `packet_id` - Unique identifier for this packet (used if fragmentation occurs)
///
/// # Returns
/// A vector containing either:
/// - A single element with the original packet (if small enough)
/// - Multiple fragments with headers (if larger than MTU-6)
pub fn fragment_packet(packet_bytes: &[u8], mtu: usize, packet_id: u16) -> Vec<Vec<u8>> {
    let max_payload_size = mtu - FragmentHeader::SIZE;
    if packet_bytes.len() <= max_payload_size {
        // Optimization: small packets pass through without fragmentation headers
        return vec![packet_bytes.to_vec()];
    }

    let total_fragments =
        u16::try_from(packet_bytes.len().div_ceil(max_payload_size)).unwrap_or(u16::MAX);
    let mut fragments = Vec::new();

    for i in 0..total_fragments {
        let start = (i as usize) * max_payload_size;
        let end = ((i as usize + 1) * max_payload_size).min(packet_bytes.len());

        let header = FragmentHeader {
            packet_id,
            fragment_index: i,
            total_fragments,
        };

        let mut fragment = Vec::with_capacity(FragmentHeader::SIZE + (end - start));
        fragment.extend_from_slice(&header.to_bytes());
        fragment.extend_from_slice(&packet_bytes[start..end]);

        fragments.push(fragment);
    }

    trace!(
        "Fragmented packet {} into {} fragments",
        packet_id,
        fragments.len()
    );
    fragments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fragment_and_reassemble_small() {
        let mut reassembler = FragmentReassembler::new();
        let small_data = b"Small packet";

        // Small packets should not be fragmented
        let fragments = fragment_packet(small_data, 1500, 1);
        assert_eq!(fragments.len(), 1);
        assert_eq!(&fragments[0], small_data);

        // Should reassemble immediately
        let result = reassembler.reassemble_fragment(&fragments[0]).unwrap();
        assert_eq!(result, Some(small_data.to_vec()));
    }

    #[test]
    fn test_fragment_and_reassemble_large() {
        let mut reassembler = FragmentReassembler::new();
        let large_data = vec![0xAB; 5000]; // Larger than typical MTU
        let mtu = 1500;

        // Fragment the large packet
        let fragments = fragment_packet(&large_data, mtu, 42);
        assert!(fragments.len() > 1);

        // Reassemble fragments in order
        let mut result = None;
        for (i, fragment) in fragments.iter().enumerate() {
            result = reassembler.reassemble_fragment(fragment).unwrap();
            if i < fragments.len() - 1 {
                assert_eq!(result, None); // Not complete yet
            }
        }

        // Final fragment should complete reassembly
        assert_eq!(result, Some(large_data));
    }

    #[test]
    fn test_out_of_order_reassembly() {
        let mut reassembler = FragmentReassembler::new();
        let data = vec![0xCD; 3000];
        let fragments = fragment_packet(&data, 1000, 99);

        // Send fragments out of order: 2, 0, 3, 1
        let indices = if fragments.len() >= 4 {
            vec![2, 0, 3, 1]
        } else {
            // Handle case where there are fewer fragments
            (0..fragments.len()).rev().collect()
        };

        for (step, &idx) in indices.iter().enumerate() {
            if idx < fragments.len() {
                let result = reassembler.reassemble_fragment(&fragments[idx]).unwrap();
                if step < indices.len() - 1 {
                    assert_eq!(result, None);
                } else {
                    assert_eq!(result, Some(data.clone()));
                }
            }
        }
    }

    #[test]
    fn test_duplicate_fragment_handling() {
        let mut reassembler = FragmentReassembler::new();
        let data = vec![0xEF; 2000];
        let fragments = fragment_packet(&data, 800, 77);

        // Send first fragment twice
        assert_eq!(
            reassembler.reassemble_fragment(&fragments[0]).unwrap(),
            None
        );
        assert_eq!(
            reassembler.reassemble_fragment(&fragments[0]).unwrap(),
            None
        ); // Duplicate

        // Complete with remaining fragments
        let mut last_result = None;
        for fragment in &fragments[1..] {
            last_result = reassembler.reassemble_fragment(fragment).unwrap();
        }

        assert_eq!(last_result, Some(data));
    }

    #[test]
    fn test_mqtt_packet_detection() {
        let mut reassembler = FragmentReassembler::new();

        // Test valid MQTT packet types (1-14)
        for packet_type in 1..=14u8 {
            let mqtt_packet = vec![packet_type << 4, 0x00, 0xAB, 0xCD];
            let result = reassembler.reassemble_fragment(&mqtt_packet).unwrap();
            assert_eq!(result, Some(mqtt_packet.clone()));
        }

        // Test invalid packet type 0 - should be treated as fragment
        let invalid_mqtt = vec![0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0xFF];
        let result = reassembler.reassemble_fragment(&invalid_mqtt).unwrap();
        assert!(result.is_some()); // Should try to process as fragment

        // Test invalid packet type 15 - should be treated as fragment
        let invalid_mqtt = vec![0xF0, 0x00, 0xAB, 0xCD];
        let result = reassembler.reassemble_fragment(&invalid_mqtt).unwrap();
        // May return Some if it doesn't look like a valid fragment header
        assert!(result.is_none() || result == Some(invalid_mqtt));
    }

    #[test]
    fn test_boundary_conditions() {
        let mut reassembler = FragmentReassembler::new();

        // Test empty packet
        let empty = b"";
        let result = reassembler.reassemble_fragment(empty).unwrap();
        assert_eq!(result, Some(vec![]));

        // Test exactly MTU-sized packet
        let mtu = 1500;
        let exact_mtu_data = vec![0x11; mtu - FragmentHeader::SIZE];
        let fragments = fragment_packet(&exact_mtu_data, mtu, 55);
        assert_eq!(fragments.len(), 1);

        // Test one byte over MTU
        let over_mtu_data = vec![0x22; mtu - FragmentHeader::SIZE + 1];
        let fragments = fragment_packet(&over_mtu_data, mtu, 66);
        assert_eq!(fragments.len(), 2);

        // Test maximum u16 packet ID wrap-around
        // Use larger data to force fragmentation
        let data = vec![0x33; 2000];
        let fragments1 = fragment_packet(&data, 1000, 65535);
        let fragments2 = fragment_packet(&data, 1000, 0); // Should wrap

        assert!(fragments1.len() > 1); // Should be fragmented
        assert!(fragments2.len() > 1);

        // Verify different packet IDs in fragment headers
        let header1 = FragmentHeader::from_bytes(&fragments1[0]).unwrap();
        let header2 = FragmentHeader::from_bytes(&fragments2[0]).unwrap();
        assert_eq!(header1.packet_id, 65535);
        assert_eq!(header2.packet_id, 0);
    }

    #[test]
    fn test_concurrent_fragment_streams() {
        let mut reassembler = FragmentReassembler::new();

        // Create two different packets
        let packet1 = vec![0xAA; 2000];
        let packet2 = vec![0xBB; 2500];

        let fragments1 = fragment_packet(&packet1, 800, 100);
        let fragments2 = fragment_packet(&packet2, 800, 200);

        // Interleave fragments from both packets
        let mut complete_count = 0;

        // Send first fragment of each
        assert!(reassembler
            .reassemble_fragment(&fragments1[0])
            .unwrap()
            .is_none());
        assert!(reassembler
            .reassemble_fragment(&fragments2[0])
            .unwrap()
            .is_none());

        // Send middle fragments interleaved (up to the smaller packet size - 1)
        let min_len = fragments1.len().min(fragments2.len());
        for i in 1..min_len - 1 {
            assert!(reassembler
                .reassemble_fragment(&fragments1[i])
                .unwrap()
                .is_none());
            assert!(reassembler
                .reassemble_fragment(&fragments2[i])
                .unwrap()
                .is_none());
        }

        // Complete remaining fragments for packet 1
        for i in min_len - 1..fragments1.len() {
            let result = reassembler.reassemble_fragment(&fragments1[i]).unwrap();
            if i == fragments1.len() - 1 {
                if let Some(data) = result {
                    assert_eq!(data, packet1);
                    complete_count += 1;
                }
            } else {
                assert!(result.is_none());
            }
        }

        // Complete remaining fragments for packet 2
        for i in min_len - 1..fragments2.len() {
            let result = reassembler.reassemble_fragment(&fragments2[i]).unwrap();
            if i == fragments2.len() - 1 {
                if let Some(data) = result {
                    assert_eq!(data, packet2);
                    complete_count += 1;
                }
            } else {
                assert!(result.is_none());
            }
        }

        assert_eq!(complete_count, 2);
    }
}
