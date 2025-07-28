use crate::error::{MqttError, Result};
use crate::packet::{FixedHeader, MqttPacket, PacketType};
use crate::protocol::v5::properties::Properties;
use crate::types::ReasonCode;
use bytes::{Buf, BufMut};

/// MQTT PUBACK packet (`QoS` 1 publish acknowledgment)
#[derive(Debug, Clone)]
pub struct PubAckPacket {
    /// Packet identifier
    pub packet_id: u16,
    /// Reason code
    pub reason_code: ReasonCode,
    /// PUBACK properties (v5.0 only)
    pub properties: Properties,
}

impl PubAckPacket {
    /// Creates a new PUBACK packet
    #[must_use]
    pub fn new(packet_id: u16) -> Self {
        Self {
            packet_id,
            reason_code: ReasonCode::Success,
            properties: Properties::default(),
        }
    }

    /// Creates a new PUBACK packet with a reason code
    #[must_use]
    pub fn new_with_reason(packet_id: u16, reason_code: ReasonCode) -> Self {
        Self {
            packet_id,
            reason_code,
            properties: Properties::default(),
        }
    }

    /// Sets the reason string
    #[must_use]
    pub fn with_reason_string(mut self, reason: String) -> Self {
        self.properties.set_reason_string(reason);
        self
    }

    /// Adds a user property
    #[must_use]
    pub fn with_user_property(mut self, key: String, value: String) -> Self {
        self.properties.add_user_property(key, value);
        self
    }

    /// Validates the reason code for PUBACK
    fn is_valid_puback_reason_code(code: ReasonCode) -> bool {
        matches!(
            code,
            ReasonCode::Success
                | ReasonCode::NoMatchingSubscribers
                | ReasonCode::UnspecifiedError
                | ReasonCode::ImplementationSpecificError
                | ReasonCode::NotAuthorized
                | ReasonCode::TopicNameInvalid
                | ReasonCode::PacketIdentifierInUse
                | ReasonCode::QuotaExceeded
                | ReasonCode::PayloadFormatInvalid
        )
    }
}

impl MqttPacket for PubAckPacket {
    fn packet_type(&self) -> PacketType {
        PacketType::PubAck
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        // Variable header
        buf.put_u16(self.packet_id);

        // For v5.0, always encode reason code and properties
        // For v3.1.1, we would skip these if reason_code is Success and no properties
        if self.reason_code != ReasonCode::Success || !self.properties.is_empty() {
            buf.put_u8(u8::from(self.reason_code));
            self.properties.encode(buf)?;
        }

        Ok(())
    }

    fn decode_body<B: Buf>(buf: &mut B, fixed_header: &FixedHeader) -> Result<Self> {
        tracing::trace!(
            fixed_header_remaining = fixed_header.remaining_length,
            buf_remaining = buf.remaining(),
            "PUBACK decode started"
        );

        // Debug: print first few bytes if available
        if buf.remaining() >= 4 {
            let bytes = buf.chunk();
            tracing::trace!(
                first_bytes = ?&bytes[..4.min(bytes.len())],
                "PUBACK packet bytes"
            );
        }

        // Packet identifier
        if buf.remaining() < 2 {
            return Err(MqttError::MalformedPacket(
                "PUBACK missing packet identifier".to_string(),
            ));
        }
        let packet_id = buf.get_u16();

        // MQTT v5.0: Reason code and properties are optional
        // If remaining length is 2 (just packet ID), reason code defaults to Success (0x00)
        let (reason_code, properties) = if buf.has_remaining() {
            // Read reason code
            let reason_byte = buf.get_u8();
            let code = ReasonCode::from_u8(reason_byte).ok_or_else(|| {
                MqttError::MalformedPacket(format!(
                    "Invalid PUBACK reason code: {reason_byte} (0x{reason_byte:02X})"
                ))
            })?;

            if !Self::is_valid_puback_reason_code(code) {
                return Err(MqttError::MalformedPacket(format!(
                    "Invalid PUBACK reason code: {code:?}"
                )));
            }

            // Properties (if present)
            let props = if buf.has_remaining() {
                Properties::decode(buf)?
            } else {
                Properties::default()
            };

            (code, props)
        } else {
            // No reason code or properties - Success with empty properties
            (ReasonCode::Success, Properties::default())
        };

        Ok(Self {
            packet_id,
            reason_code,
            properties,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::v5::properties::PropertyId;
    use bytes::BytesMut;

    #[test]
    fn test_puback_basic() {
        let packet = PubAckPacket::new(123);

        assert_eq!(packet.packet_id, 123);
        assert_eq!(packet.reason_code, ReasonCode::Success);
        assert!(packet.properties.is_empty());
    }

    #[test]
    fn test_puback_with_reason() {
        let packet = PubAckPacket::new_with_reason(456, ReasonCode::NoMatchingSubscribers)
            .with_reason_string("No subscribers for topic".to_string());

        assert_eq!(packet.packet_id, 456);
        assert_eq!(packet.reason_code, ReasonCode::NoMatchingSubscribers);
        assert!(packet.properties.contains(PropertyId::ReasonString));
    }

    #[test]
    fn test_puback_encode_decode_minimal() {
        let packet = PubAckPacket::new(789);

        let mut buf = BytesMut::new();
        packet.encode(&mut buf).unwrap();

        let fixed_header = FixedHeader::decode(&mut buf).unwrap();
        assert_eq!(fixed_header.packet_type, PacketType::PubAck);

        let decoded = PubAckPacket::decode_body(&mut buf, &fixed_header).unwrap();
        assert_eq!(decoded.packet_id, 789);
        assert_eq!(decoded.reason_code, ReasonCode::Success);
    }

    #[test]
    fn test_puback_encode_decode_with_reason() {
        let packet = PubAckPacket::new_with_reason(999, ReasonCode::QuotaExceeded)
            .with_user_property("quota".to_string(), "exceeded".to_string());

        let mut buf = BytesMut::new();
        packet.encode(&mut buf).unwrap();

        let fixed_header = FixedHeader::decode(&mut buf).unwrap();
        let decoded = PubAckPacket::decode_body(&mut buf, &fixed_header).unwrap();

        assert_eq!(decoded.packet_id, 999);
        assert_eq!(decoded.reason_code, ReasonCode::QuotaExceeded);
        assert!(decoded.properties.contains(PropertyId::UserProperty));
    }

    #[test]
    fn test_puback_v311_style() {
        // v3.1.1 style - only packet ID
        let mut buf = BytesMut::new();
        buf.put_u16(1234);

        let fixed_header = FixedHeader::new(PacketType::PubAck, 0, 2);
        let decoded = PubAckPacket::decode_body(&mut buf, &fixed_header).unwrap();

        assert_eq!(decoded.packet_id, 1234);
        assert_eq!(decoded.reason_code, ReasonCode::Success);
        assert!(decoded.properties.is_empty());
    }

    #[test]
    fn test_puback_invalid_reason_code() {
        let mut buf = BytesMut::new();
        buf.put_u16(123);
        buf.put_u8(0xFF); // Invalid reason code

        let fixed_header = FixedHeader::new(PacketType::PubAck, 0, 3);
        let result = PubAckPacket::decode_body(&mut buf, &fixed_header);
        assert!(result.is_err());
    }

    #[test]
    fn test_puback_missing_packet_id() {
        let mut buf = BytesMut::new();
        buf.put_u8(0); // Only one byte, not enough for packet ID

        let fixed_header = FixedHeader::new(PacketType::PubAck, 0, 1);
        let result = PubAckPacket::decode_body(&mut buf, &fixed_header);
        assert!(result.is_err());
    }
}
