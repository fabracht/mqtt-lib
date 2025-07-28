use crate::error::Result;
use crate::packet::{FixedHeader, MqttPacket, PacketType};
use bytes::{Buf, BufMut};

/// MQTT PINGREQ packet
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PingReqPacket;

impl PingReqPacket {
    /// Creates a new PINGREQ packet
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for PingReqPacket {
    fn default() -> Self {
        Self::new()
    }
}

impl MqttPacket for PingReqPacket {
    fn packet_type(&self) -> PacketType {
        PacketType::PingReq
    }

    fn encode_body<B: BufMut>(&self, _buf: &mut B) -> Result<()> {
        // PINGREQ has no variable header or payload
        Ok(())
    }

    fn decode_body<B: Buf>(_buf: &mut B, _fixed_header: &FixedHeader) -> Result<Self> {
        // PINGREQ has no variable header or payload
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn test_pingreq_encode_decode() {
        let packet = PingReqPacket::new();

        let mut buf = BytesMut::new();
        packet.encode(&mut buf).unwrap();

        assert_eq!(buf.len(), 2); // Fixed header only
        assert_eq!(buf[0], 0xC0); // PINGREQ packet type
        assert_eq!(buf[1], 0x00); // Remaining length = 0

        let fixed_header = FixedHeader::decode(&mut buf).unwrap();
        assert_eq!(fixed_header.packet_type, PacketType::PingReq);
        assert_eq!(fixed_header.remaining_length, 0);

        let decoded = PingReqPacket::decode_body(&mut buf, &fixed_header).unwrap();
        assert_eq!(decoded, packet);
    }
}
