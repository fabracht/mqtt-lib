use crate::error::Result;
use crate::packet::{FixedHeader, MqttPacket, PacketType};
use bytes::{Buf, BufMut};

/// MQTT PINGRESP packet
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PingRespPacket;

impl PingRespPacket {
    /// Creates a new PINGRESP packet
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for PingRespPacket {
    fn default() -> Self {
        Self::new()
    }
}

impl MqttPacket for PingRespPacket {
    fn packet_type(&self) -> PacketType {
        PacketType::PingResp
    }

    fn encode_body<B: BufMut>(&self, _buf: &mut B) -> Result<()> {
        // PINGRESP has no variable header or payload
        Ok(())
    }

    fn decode_body<B: Buf>(_buf: &mut B, _fixed_header: &FixedHeader) -> Result<Self> {
        // PINGRESP has no variable header or payload
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn test_pingresp_encode_decode() {
        let packet = PingRespPacket::new();

        let mut buf = BytesMut::new();
        packet.encode(&mut buf).unwrap();

        assert_eq!(buf.len(), 2); // Fixed header only
        assert_eq!(buf[0], 0xD0); // PINGRESP packet type
        assert_eq!(buf[1], 0x00); // Remaining length = 0

        let fixed_header = FixedHeader::decode(&mut buf).unwrap();
        assert_eq!(fixed_header.packet_type, PacketType::PingResp);
        assert_eq!(fixed_header.remaining_length, 0);

        let decoded = PingRespPacket::decode_body(&mut buf, &fixed_header).unwrap();
        assert_eq!(decoded, packet);
    }
}
