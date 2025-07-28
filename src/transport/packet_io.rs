//! Packet I/O utilities for transport layer
//!
//! CRITICAL: NO EVENT LOOPS
//! These are direct async methods for reading and writing MQTT packets.

use crate::encoding::encode_variable_int;
use crate::error::{MqttError, Result};
use crate::packet::{FixedHeader, MqttPacket, Packet, PacketType};
use crate::transport::Transport;
use bytes::{BufMut, BytesMut};
use std::future::Future;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

/// Extension trait for Transport to add packet I/O methods
pub trait PacketIo: Transport {
    /// Read a complete MQTT packet
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    fn read_packet(&mut self) -> impl Future<Output = Result<Packet>> + Send + '_ {
        async move {
            // Read fixed header bytes
            let mut header_buf = BytesMut::with_capacity(5);

            // Read first byte (packet type and flags)
            let mut byte = [0u8; 1];
            let n = self.read(&mut byte).await?;
            if n == 0 {
                return Err(MqttError::ConnectionError("Connection closed".to_string()));
            }
            header_buf.put_u8(byte[0]);

            // Read remaining length (variable length encoding)
            loop {
                let n = self.read(&mut byte).await?;
                if n == 0 {
                    return Err(MqttError::ConnectionError("Connection closed".to_string()));
                }
                header_buf.put_u8(byte[0]);

                if (byte[0] & 0x80) == 0 {
                    break;
                }

                if header_buf.len() > 4 {
                    return Err(MqttError::MalformedPacket(
                        "Invalid remaining length encoding".to_string(),
                    ));
                }
            }

            // Parse the complete fixed header
            let mut header_buf = header_buf.freeze();
            let fixed_header = FixedHeader::decode(&mut header_buf)?;

            if fixed_header.remaining_length > 10000 {
                tracing::debug!(
                    packet_type = ?fixed_header.packet_type,
                    remaining_length = fixed_header.remaining_length,
                    "Receiving large packet"
                );
            }

            // Read remaining bytes
            let mut payload = vec![0u8; fixed_header.remaining_length as usize];
            let mut bytes_read = 0;
            while bytes_read < payload.len() {
                let n = self.read(&mut payload[bytes_read..]).await?;
                if n == 0 {
                    return Err(MqttError::ConnectionError(
                        "Connection closed while reading packet".to_string(),
                    ));
                }
                bytes_read += n;
            }

            // Parse packet based on type
            let mut payload_buf = BytesMut::from(&payload[..]);

            match fixed_header.packet_type {
                PacketType::Connect => {
                    use crate::packet::connect::ConnectPacket;
                    let packet = ConnectPacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::Connect(Box::new(packet)))
                }
                PacketType::ConnAck => {
                    use crate::packet::connack::ConnAckPacket;
                    let packet = ConnAckPacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::ConnAck(packet))
                }
                PacketType::Publish => {
                    use crate::packet::publish::PublishPacket;
                    let packet = PublishPacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::Publish(packet))
                }
                PacketType::PubAck => {
                    use crate::packet::puback::PubAckPacket;
                    tracing::trace!(payload_len = payload.len(), "Decoding PUBACK packet");
                    let packet = PubAckPacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::PubAck(packet))
                }
                PacketType::PubRec => {
                    use crate::packet::pubrec::PubRecPacket;
                    let packet = PubRecPacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::PubRec(packet))
                }
                PacketType::PubRel => {
                    use crate::packet::pubrel::PubRelPacket;
                    let packet = PubRelPacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::PubRel(packet))
                }
                PacketType::PubComp => {
                    use crate::packet::pubcomp::PubCompPacket;
                    let packet = PubCompPacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::PubComp(packet))
                }
                PacketType::Subscribe => {
                    use crate::packet::subscribe::SubscribePacket;
                    let packet = SubscribePacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::Subscribe(packet))
                }
                PacketType::SubAck => {
                    use crate::packet::suback::SubAckPacket;
                    let packet = SubAckPacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::SubAck(packet))
                }
                PacketType::Unsubscribe => {
                    use crate::packet::unsubscribe::UnsubscribePacket;
                    let packet = UnsubscribePacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::Unsubscribe(packet))
                }
                PacketType::UnsubAck => {
                    use crate::packet::unsuback::UnsubAckPacket;
                    let packet = UnsubAckPacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::UnsubAck(packet))
                }
                PacketType::PingReq => Ok(Packet::PingReq),
                PacketType::PingResp => Ok(Packet::PingResp),
                PacketType::Disconnect => {
                    use crate::packet::disconnect::DisconnectPacket;
                    let packet = DisconnectPacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::Disconnect(packet))
                }
                PacketType::Auth => {
                    use crate::packet::auth::AuthPacket;
                    let packet = AuthPacket::decode_body(&mut payload_buf, &fixed_header)?;
                    Ok(Packet::Auth(packet))
                }
            }
        }
    }

    /// Write a complete MQTT packet
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    fn write_packet(&mut self, packet: Packet) -> impl Future<Output = Result<()>> + Send + '_ {
        async move {
            let mut buf = BytesMut::with_capacity(1024);

            // Encode packet
            match packet {
                Packet::Connect(p) => {
                    encode_packet(&mut buf, PacketType::Connect, 0, |buf| p.encode_body(buf))?;
                }
                Packet::ConnAck(p) => {
                    encode_packet(&mut buf, PacketType::ConnAck, 0, |buf| p.encode_body(buf))?;
                }
                Packet::Publish(p) => {
                    let flags = p.flags();
                    encode_packet(&mut buf, PacketType::Publish, flags, |buf| {
                        p.encode_body(buf)
                    })?;
                }
                Packet::PubAck(p) => {
                    encode_packet(&mut buf, PacketType::PubAck, 0, |buf| p.encode_body(buf))?;
                }
                Packet::PubRec(p) => {
                    encode_packet(&mut buf, PacketType::PubRec, 0, |buf| p.encode_body(buf))?;
                }
                Packet::PubRel(p) => {
                    encode_packet(&mut buf, PacketType::PubRel, 0x02, |buf| p.encode_body(buf))?;
                }
                Packet::PubComp(p) => {
                    encode_packet(&mut buf, PacketType::PubComp, 0, |buf| p.encode_body(buf))?;
                }
                Packet::Subscribe(p) => {
                    encode_packet(&mut buf, PacketType::Subscribe, 0x02, |buf| {
                        p.encode_body(buf)
                    })?;
                }
                Packet::SubAck(p) => {
                    encode_packet(&mut buf, PacketType::SubAck, 0, |buf| p.encode_body(buf))?;
                }
                Packet::Unsubscribe(p) => {
                    encode_packet(&mut buf, PacketType::Unsubscribe, 0x02, |buf| {
                        p.encode_body(buf)
                    })?;
                }
                Packet::UnsubAck(p) => {
                    encode_packet(&mut buf, PacketType::UnsubAck, 0, |buf| p.encode_body(buf))?;
                }
                Packet::PingReq => encode_packet(&mut buf, PacketType::PingReq, 0, |_| Ok(()))?,
                Packet::PingResp => encode_packet(&mut buf, PacketType::PingResp, 0, |_| Ok(()))?,
                Packet::Disconnect(p) => {
                    encode_packet(&mut buf, PacketType::Disconnect, 0, |buf| {
                        p.encode_body(buf)
                    })?;
                }
                Packet::Auth(p) => {
                    encode_packet(&mut buf, PacketType::Auth, 0, |buf| p.encode_body(buf))?;
                }
            }
            // Write to transport
            self.write(&buf).await?;
            Ok(())
        }
    }
}

/// Helper function to encode a packet with fixed header
fn encode_packet<F>(
    buf: &mut BytesMut,
    packet_type: PacketType,
    flags: u8,
    encode_body: F,
) -> Result<()>
where
    F: FnOnce(&mut BytesMut) -> Result<()>,
{
    // Encode body first to get remaining length
    let mut body_buf = BytesMut::new();
    encode_body(&mut body_buf)?;

    // Write fixed header
    let byte1 = (u8::from(packet_type) << 4) | (flags & 0x0F);
    buf.put_u8(byte1);
    encode_variable_int(buf, body_buf.len().min(u32::MAX as usize) as u32)?;

    // Write body
    buf.put(body_buf);

    Ok(())
}

// Implement PacketIo for all types that implement Transport
impl<T: Transport> PacketIo for T {}

/// Packet reader trait for split read halves
pub trait PacketReader {
    /// Read a complete MQTT packet
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    fn read_packet(&mut self) -> impl Future<Output = Result<Packet>> + Send + '_;
}

/// Packet writer trait for split write halves  
pub trait PacketWriter {
    /// Write a complete MQTT packet
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails
    fn write_packet(&mut self, packet: Packet) -> impl Future<Output = Result<()>> + Send + '_;
}

/// Implementation for TCP read half
impl PacketReader for OwnedReadHalf {
    async fn read_packet(&mut self) -> Result<Packet> {
        // Read fixed header bytes
        let mut header_buf = BytesMut::with_capacity(5);

        // Read first byte (packet type and flags)
        let mut byte = [0u8; 1];
        let n = self.read(&mut byte).await?;
        if n == 0 {
            return Err(MqttError::ConnectionError("Connection closed".to_string()));
        }
        header_buf.put_u8(byte[0]);

        // Read remaining length (variable length encoding)
        loop {
            let n = self.read(&mut byte).await?;
            if n == 0 {
                return Err(MqttError::ConnectionError("Connection closed".to_string()));
            }
            header_buf.put_u8(byte[0]);

            if (byte[0] & 0x80) == 0 {
                break;
            }

            if header_buf.len() > 4 {
                return Err(MqttError::MalformedPacket(
                    "Invalid remaining length encoding".to_string(),
                ));
            }
        }

        // Parse the complete fixed header
        let mut header_buf = header_buf.freeze();
        let fixed_header = FixedHeader::decode(&mut header_buf)?;

        // Read remaining bytes
        let mut payload = vec![0u8; fixed_header.remaining_length as usize];
        let mut bytes_read = 0;
        while bytes_read < payload.len() {
            let n = self.read(&mut payload[bytes_read..]).await?;
            if n == 0 {
                return Err(MqttError::ConnectionError(
                    "Connection closed while reading packet".to_string(),
                ));
            }
            bytes_read += n;
        }

        // Parse packet based on type
        let mut payload_buf = BytesMut::from(&payload[..]);

        match fixed_header.packet_type {
            PacketType::Connect => {
                use crate::packet::connect::ConnectPacket;
                let packet = ConnectPacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::Connect(Box::new(packet)))
            }
            PacketType::ConnAck => {
                use crate::packet::connack::ConnAckPacket;
                let packet = ConnAckPacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::ConnAck(packet))
            }
            PacketType::Publish => {
                use crate::packet::publish::PublishPacket;
                let packet = PublishPacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::Publish(packet))
            }
            PacketType::PubAck => {
                use crate::packet::puback::PubAckPacket;
                let packet = PubAckPacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::PubAck(packet))
            }
            PacketType::PubRec => {
                use crate::packet::pubrec::PubRecPacket;
                let packet = PubRecPacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::PubRec(packet))
            }
            PacketType::PubRel => {
                use crate::packet::pubrel::PubRelPacket;
                let packet = PubRelPacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::PubRel(packet))
            }
            PacketType::PubComp => {
                use crate::packet::pubcomp::PubCompPacket;
                let packet = PubCompPacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::PubComp(packet))
            }
            PacketType::Subscribe => {
                use crate::packet::subscribe::SubscribePacket;
                let packet = SubscribePacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::Subscribe(packet))
            }
            PacketType::SubAck => {
                use crate::packet::suback::SubAckPacket;
                let packet = SubAckPacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::SubAck(packet))
            }
            PacketType::Unsubscribe => {
                use crate::packet::unsubscribe::UnsubscribePacket;
                let packet = UnsubscribePacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::Unsubscribe(packet))
            }
            PacketType::UnsubAck => {
                use crate::packet::unsuback::UnsubAckPacket;
                let packet = UnsubAckPacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::UnsubAck(packet))
            }
            PacketType::PingReq => Ok(Packet::PingReq),
            PacketType::PingResp => Ok(Packet::PingResp),
            PacketType::Disconnect => {
                use crate::packet::disconnect::DisconnectPacket;
                let packet = DisconnectPacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::Disconnect(packet))
            }
            PacketType::Auth => {
                use crate::packet::auth::AuthPacket;
                let packet = AuthPacket::decode_body(&mut payload_buf, &fixed_header)?;
                Ok(Packet::Auth(packet))
            }
        }
    }
}

/// Implementation for TCP write half
impl PacketWriter for OwnedWriteHalf {
    async fn write_packet(&mut self, packet: Packet) -> Result<()> {
        let mut buf = BytesMut::with_capacity(1024);

        // Encode packet
        match packet {
            Packet::Connect(p) => {
                encode_packet(&mut buf, PacketType::Connect, 0, |buf| p.encode_body(buf))?;
            }
            Packet::ConnAck(p) => {
                encode_packet(&mut buf, PacketType::ConnAck, 0, |buf| p.encode_body(buf))?;
            }
            Packet::Publish(p) => {
                let flags = p.flags();
                encode_packet(&mut buf, PacketType::Publish, flags, |buf| {
                    p.encode_body(buf)
                })?;
            }
            Packet::PubAck(p) => {
                encode_packet(&mut buf, PacketType::PubAck, 0, |buf| p.encode_body(buf))?;
            }
            Packet::PubRec(p) => {
                encode_packet(&mut buf, PacketType::PubRec, 0, |buf| p.encode_body(buf))?;
            }
            Packet::PubRel(p) => {
                encode_packet(&mut buf, PacketType::PubRel, 0x02, |buf| p.encode_body(buf))?;
            }
            Packet::PubComp(p) => {
                encode_packet(&mut buf, PacketType::PubComp, 0, |buf| p.encode_body(buf))?;
            }
            Packet::Subscribe(p) => {
                encode_packet(&mut buf, PacketType::Subscribe, 0x02, |buf| {
                    p.encode_body(buf)
                })?;
            }
            Packet::SubAck(p) => {
                encode_packet(&mut buf, PacketType::SubAck, 0, |buf| p.encode_body(buf))?;
            }
            Packet::Unsubscribe(p) => {
                encode_packet(&mut buf, PacketType::Unsubscribe, 0x02, |buf| {
                    p.encode_body(buf)
                })?;
            }
            Packet::UnsubAck(p) => {
                encode_packet(&mut buf, PacketType::UnsubAck, 0, |buf| p.encode_body(buf))?;
            }
            Packet::PingReq => encode_packet(&mut buf, PacketType::PingReq, 0, |_| Ok(()))?,
            Packet::PingResp => encode_packet(&mut buf, PacketType::PingResp, 0, |_| Ok(()))?,
            Packet::Disconnect(p) => {
                encode_packet(&mut buf, PacketType::Disconnect, 0, |buf| {
                    p.encode_body(buf)
                })?;
            }
            Packet::Auth(p) => {
                encode_packet(&mut buf, PacketType::Auth, 0, |buf| p.encode_body(buf))?;
            }
        }
        // Write to transport
        self.write_all(&buf).await?;
        self.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::connack::ConnAckPacket;
    use crate::packet::publish::PublishPacket;
    use crate::packet::subscribe::{SubscribePacket, SubscriptionOptions, TopicFilter};
    use crate::protocol::v5::properties::Properties;
    use crate::protocol::v5::reason_codes::ReasonCode;
    use crate::transport::mock::MockTransport;
    use crate::QoS;

    #[tokio::test]
    async fn test_read_packet_pingresp() {
        let mut transport = MockTransport::new();
        transport.connect().await.unwrap();

        // Inject a PINGRESP packet (0xD0 0x00)
        transport.add_incoming_data(&[0xD0, 0x00]).await;

        let packet = transport.read_packet().await.unwrap();
        assert!(matches!(packet, Packet::PingResp));
    }

    #[tokio::test]
    async fn test_read_packet_pingreq() {
        let mut transport = MockTransport::new();
        transport.connect().await.unwrap();

        // Inject a PINGREQ packet (0xC0 0x00)
        transport.add_incoming_data(&[0xC0, 0x00]).await;

        let packet = transport.read_packet().await.unwrap();
        assert!(matches!(packet, Packet::PingReq));
    }

    #[tokio::test]
    async fn test_read_packet_connack() {
        let mut transport = MockTransport::new();
        transport.connect().await.unwrap();

        // Create a simple CONNACK: fixed header + session present + reason code + properties
        let data = vec![
            0x20, // CONNACK type
            0x03, // Remaining length
            0x00, // Session not present
            0x00, // Success reason code
            0x00, // No properties
        ];
        transport.add_incoming_data(&data).await;

        let packet = transport.read_packet().await.unwrap();
        match packet {
            Packet::ConnAck(connack) => {
                assert!(!connack.session_present);
                assert_eq!(connack.reason_code, ReasonCode::Success);
            }
            _ => panic!("Expected CONNACK packet"),
        }
    }

    #[tokio::test]
    async fn test_read_packet_publish() {
        let mut transport = MockTransport::new();
        transport.connect().await.unwrap();

        // Create a PUBLISH packet with QoS 0
        let topic = "test/topic";
        let payload = b"Hello MQTT";

        let mut data = vec![
            0x30,                                                             // PUBLISH with QoS 0
            u8::try_from(2 + topic.len() + 1 + payload.len()).unwrap_or(255), // Remaining length (topic_len + topic + props + payload)
        ];

        // Topic length and name
        data.extend_from_slice(&[
            ((topic.len() >> 8) & 0xFF) as u8,
            (topic.len() & 0xFF) as u8,
        ]);
        data.extend_from_slice(topic.as_bytes());

        // Properties length (0 for no properties)
        data.push(0x00);

        // Payload
        data.extend_from_slice(payload);

        transport.add_incoming_data(&data).await;

        let packet = transport.read_packet().await.unwrap();
        match packet {
            Packet::Publish(publish) => {
                assert_eq!(publish.topic_name, "test/topic");
                assert_eq!(publish.payload, b"Hello MQTT");
                assert_eq!(publish.qos, QoS::AtMostOnce);
                assert_eq!(publish.packet_id, None);
            }
            _ => panic!("Expected PUBLISH packet"),
        }
    }

    #[tokio::test]
    async fn test_read_packet_invalid_remaining_length() {
        let mut transport = MockTransport::new();
        transport.connect().await.unwrap();

        // Create packet with invalid remaining length (5 bytes with continuation bit)
        let data = vec![
            0x30, // PUBLISH
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // Invalid - too many bytes
        ];
        transport.add_incoming_data(&data).await;

        let result = transport.read_packet().await;
        assert!(result.is_err());
        if let Err(e) = result {
            match e {
                MqttError::MalformedPacket(_) => {}
                _ => panic!("Expected MalformedPacket error, got: {e:?}"),
            }
        }
    }

    #[tokio::test]
    async fn test_read_packet_connection_closed() {
        let mut transport = MockTransport::new();

        // Don't add any data - read should return 0
        let result = transport.read_packet().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_packet_pingreq() {
        let mut transport = MockTransport::new();
        transport.connect().await.unwrap();

        transport.write_packet(Packet::PingReq).await.unwrap();

        let written = transport.get_written_data().await;
        assert_eq!(written, vec![0xC0, 0x00]); // PINGREQ packet
    }

    #[tokio::test]
    async fn test_write_packet_publish() {
        let mut transport = MockTransport::new();
        transport.connect().await.unwrap();

        let publish = PublishPacket {
            topic_name: "test".to_string(),
            payload: vec![1, 2, 3],
            qos: QoS::AtLeastOnce,
            retain: false,
            dup: false,
            packet_id: Some(123),
            properties: Properties::new(),
        };

        transport
            .write_packet(Packet::Publish(publish))
            .await
            .unwrap();

        let written = transport.get_written_data().await;

        // Verify fixed header
        assert_eq!(written[0] >> 4, u8::from(PacketType::Publish));
        assert_eq!(written[0] & 0x0F, 0x02); // QoS 1 flag

        // Should contain topic, packet ID, and payload
        assert!(written.len() > 2 + 4 + 2 + 3); // header + topic + packet_id + payload
    }

    #[tokio::test]
    async fn test_write_packet_subscribe() {
        let mut transport = MockTransport::new();
        transport.connect().await.unwrap();

        let subscribe = SubscribePacket {
            packet_id: 456,
            properties: Properties::new(),
            filters: vec![TopicFilter {
                filter: "test/+".to_string(),
                options: SubscriptionOptions {
                    qos: QoS::AtLeastOnce,
                    no_local: false,
                    retain_as_published: false,
                    retain_handling: crate::packet::subscribe::RetainHandling::SendAtSubscribe,
                },
            }],
        };

        transport
            .write_packet(Packet::Subscribe(subscribe))
            .await
            .unwrap();

        let written = transport.get_written_data().await;

        // Verify fixed header
        assert_eq!(written[0], 0x82); // SUBSCRIBE with required flags
        assert!(written.len() > 2); // Has content
    }

    #[tokio::test]
    async fn test_roundtrip_packets() {
        // Test that we can write and read back various packet types
        let test_packets = vec![
            Packet::PingReq,
            Packet::PingResp,
            Packet::ConnAck(ConnAckPacket {
                session_present: true,
                reason_code: ReasonCode::Success,
                properties: Properties::new(),
                protocol_version: 5,
            }),
        ];

        for packet in test_packets {
            let mut write_transport = MockTransport::new();
            write_transport.connect().await.unwrap();
            write_transport.write_packet(packet.clone()).await.unwrap();

            let data = write_transport.get_written_data().await;

            let mut read_transport = MockTransport::new();
            read_transport.connect().await.unwrap();
            read_transport.add_incoming_data(&data).await;

            let read_packet = read_transport.read_packet().await.unwrap();

            // Basic type check
            match (&packet, &read_packet) {
                (Packet::PingReq, Packet::PingReq) => {}
                (Packet::PingResp, Packet::PingResp) => {}
                (Packet::ConnAck(a), Packet::ConnAck(b)) => {
                    assert_eq!(a.session_present, b.session_present);
                    assert_eq!(a.reason_code, b.reason_code);
                }
                _ => panic!("Packet type mismatch"),
            }
        }
    }

    #[tokio::test]
    async fn test_encode_packet_helper() {
        let mut buf = BytesMut::new();

        // Test encoding a simple packet
        encode_packet(&mut buf, PacketType::PingReq, 0, |_| Ok(())).unwrap();

        assert_eq!(buf.len(), 2);
        assert_eq!(buf[0], 0xC0); // PINGREQ type
        assert_eq!(buf[1], 0x00); // Zero length
    }

    #[tokio::test]
    async fn test_variable_length_encoding() {
        let mut transport = MockTransport::new();
        transport.connect().await.unwrap();

        // Create a publish with large payload to test variable length encoding
        let mut large_payload = vec![0u8; 200];
        for i in 0..200 {
            large_payload[i] = (i % 256) as u8;
        }

        let publish = PublishPacket {
            topic_name: "test".to_string(),
            payload: large_payload,
            qos: QoS::AtMostOnce,
            retain: false,
            dup: false,
            packet_id: None,
            properties: Properties::new(),
        };

        transport
            .write_packet(Packet::Publish(publish))
            .await
            .unwrap();

        let written = transport.get_written_data().await;

        // Verify the remaining length uses 2 bytes (since payload > 127)
        assert!(written[1] & 0x80 != 0); // Continuation bit set
        assert!(written.len() > 200); // Contains the large payload
    }
}
