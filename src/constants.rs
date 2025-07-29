//! MQTT Protocol Constants
//!
//! This module defines constants for MQTT packet types and flags to avoid magic numbers
//! throughout the codebase.

use crate::PacketType;

/// Fixed header byte 1 values (packet type << 4 | flags)
pub mod fixed_header {
    /// CONNECT packet fixed header (0x10)
    pub const CONNECT: u8 = (super::PacketType::Connect as u8) << 4;

    /// CONNACK packet fixed header (0x20)
    pub const CONNACK: u8 = (super::PacketType::ConnAck as u8) << 4;

    /// PUBLISH packet fixed header base (0x30) - flags vary
    pub const PUBLISH_BASE: u8 = (super::PacketType::Publish as u8) << 4;

    /// PUBACK packet fixed header (0x40)
    pub const PUBACK: u8 = (super::PacketType::PubAck as u8) << 4;

    /// PUBREC packet fixed header (0x50)
    pub const PUBREC: u8 = (super::PacketType::PubRec as u8) << 4;

    /// PUBREL packet fixed header (0x62) - has required flags
    pub const PUBREL: u8 = (super::PacketType::PubRel as u8) << 4 | 0x02;

    /// PUBCOMP packet fixed header (0x70)
    pub const PUBCOMP: u8 = (super::PacketType::PubComp as u8) << 4;

    /// SUBSCRIBE packet fixed header (0x82) - has required flags
    pub const SUBSCRIBE: u8 = (super::PacketType::Subscribe as u8) << 4 | 0x02;

    /// SUBACK packet fixed header (0x90)
    pub const SUBACK: u8 = (super::PacketType::SubAck as u8) << 4;

    /// UNSUBSCRIBE packet fixed header (0xA2) - has required flags
    pub const UNSUBSCRIBE: u8 = (super::PacketType::Unsubscribe as u8) << 4 | 0x02;

    /// UNSUBACK packet fixed header (0xB0)
    pub const UNSUBACK: u8 = (super::PacketType::UnsubAck as u8) << 4;

    /// PINGREQ packet fixed header (0xC0)
    pub const PINGREQ: u8 = (super::PacketType::PingReq as u8) << 4;

    /// PINGRESP packet fixed header (0xD0)
    pub const PINGRESP: u8 = (super::PacketType::PingResp as u8) << 4;

    /// DISCONNECT packet fixed header (0xE0)
    pub const DISCONNECT: u8 = (super::PacketType::Disconnect as u8) << 4;

    /// AUTH packet fixed header (0xF0)
    pub const AUTH: u8 = (super::PacketType::Auth as u8) << 4;
}

/// Masks for extracting fields from fixed header
pub mod masks {
    /// Mask for extracting packet type from fixed header byte 1 (0xF0)
    pub const PACKET_TYPE: u8 = 0xF0;

    /// Mask for extracting flags from fixed header byte 1 (0x0F)
    pub const FLAGS: u8 = 0x0F;

    /// Mask for checking continuation bit in variable byte integer (0x80)
    pub const CONTINUATION_BIT: u8 = 0x80;

    /// Mask for extracting value from variable byte integer (0x7F)
    pub const VARIABLE_BYTE_VALUE: u8 = 0x7F;
}

/// Common packet payloads
pub mod packets {
    /// PINGREQ packet as bytes
    pub const PINGREQ_BYTES: [u8; 2] = [super::fixed_header::PINGREQ, 0x00];

    /// PINGRESP packet as bytes
    pub const PINGRESP_BYTES: [u8; 2] = [super::fixed_header::PINGRESP, 0x00];
}

/// Subscription option masks
pub mod subscription {
    /// Mask for `QoS` bits (bits 0-1)
    pub const QOS_MASK: u8 = 0x03;

    /// Mask for No Local flag (bit 2)
    pub const NO_LOCAL_MASK: u8 = 0x04;

    /// Mask for Retain As Published flag (bit 3)
    pub const RETAIN_AS_PUBLISHED_MASK: u8 = 0x08;

    /// Mask for Retain Handling (bits 4-5)
    pub const RETAIN_HANDLING_MASK: u8 = 0x30;

    /// Shift for Retain Handling
    pub const RETAIN_HANDLING_SHIFT: u8 = 4;

    /// Mask for reserved bits (bits 6-7)
    pub const RESERVED_BITS_MASK: u8 = 0xC0;
}

/// CONNECT flags masks
pub mod connect_flags {
    /// Mask for clearing Will `QoS` bits (bits 3-4)
    pub const WILL_QOS_CLEAR_MASK: u8 = !0x18;
    /// Mask for extracting Will `QoS` (bits 3-4 shifted)
    pub const WILL_QOS_MASK: u8 = 0x03;
    /// Shift for Will `QoS`
    pub const WILL_QOS_SHIFT: u8 = 3;
}

/// PUBLISH flags masks
pub mod publish_flags {
    /// Mask for clearing `QoS` bits (bits 1-2)
    pub const QOS_CLEAR_MASK: u8 = !0x06;
    /// Mask for extracting `QoS` (bits 1-2 shifted)
    pub const QOS_MASK: u8 = 0x03;
    /// Shift for `QoS`
    pub const QOS_SHIFT: u8 = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_header_values() {
        assert_eq!(fixed_header::CONNECT, 0x10);
        assert_eq!(fixed_header::CONNACK, 0x20);
        assert_eq!(fixed_header::PUBLISH_BASE, 0x30);
        assert_eq!(fixed_header::PINGREQ, 0xC0);
        assert_eq!(fixed_header::PINGRESP, 0xD0);
    }

    #[test]
    fn test_packets() {
        assert_eq!(packets::PINGREQ_BYTES, [0xC0, 0x00]);
        assert_eq!(packets::PINGRESP_BYTES, [0xD0, 0x00]);
    }
}
