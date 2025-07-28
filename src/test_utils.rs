//! Test utilities for the MQTT library
//!
//! CRITICAL: NO EVENT LOOPS
//! These are utilities for testing the async MQTT implementation.

use crate::packet::connack::*;
use crate::packet::connect::*;
use crate::packet::publish::*;
use crate::packet::suback::*;
use crate::packet::subscribe::*;
use crate::packet::*;
use crate::protocol::v5::properties::*;
use crate::protocol::v5::reason_codes::*;
use crate::QoS;
use bytes::{BufMut, BytesMut};
use std::time::Duration;
use tokio::time::timeout;

/// Helper trait for building test packets
pub trait TestPacketBuilder: Sized {
    fn test_default() -> Self;
}

impl TestPacketBuilder for ConnectPacket {
    fn test_default() -> Self {
        Self {
            protocol_version: 5,
            clean_start: true,
            keep_alive: 60,
            properties: Properties::new(),
            client_id: "test-client".to_string(),
            will: None,
            username: None,
            password: None,
            will_properties: Properties::new(),
        }
    }
}

impl TestPacketBuilder for ConnAckPacket {
    fn test_default() -> Self {
        Self {
            protocol_version: 5,
            session_present: false,
            reason_code: ReasonCode::Success,
            properties: Properties::new(),
        }
    }
}

impl TestPacketBuilder for PublishPacket {
    fn test_default() -> Self {
        Self {
            topic_name: "test/topic".to_string(),
            packet_id: None,
            payload: b"test payload".to_vec(),
            qos: QoS::AtMostOnce,
            retain: false,
            dup: false,
            properties: Properties::new(),
        }
    }
}

impl TestPacketBuilder for SubscribePacket {
    fn test_default() -> Self {
        Self {
            packet_id: 1,
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
        }
    }
}

impl TestPacketBuilder for SubAckPacket {
    fn test_default() -> Self {
        Self {
            packet_id: 1,
            properties: Properties::new(),
            reason_codes: vec![SubAckReasonCode::GrantedQoS1],
        }
    }
}

/// Creates a test CONNECT packet
pub fn create_test_connect() -> Packet {
    Packet::Connect(Box::new(ConnectPacket::test_default()))
}

/// Creates a test CONNACK packet
pub fn create_test_connack(session_present: bool, reason_code: ReasonCode) -> Packet {
    Packet::ConnAck(ConnAckPacket {
        protocol_version: 5,
        session_present,
        reason_code,
        properties: Properties::new(),
    })
}

/// Creates a test PUBLISH packet
pub fn create_test_publish(topic: &str, payload: &[u8], qos: QoS) -> Packet {
    let packet_id = match qos {
        QoS::AtMostOnce => None,
        _ => Some(1),
    };

    Packet::Publish(PublishPacket {
        topic_name: topic.to_string(),
        packet_id,
        payload: payload.to_vec(),
        qos,
        retain: false,
        dup: false,
        properties: Properties::new(),
    })
}

/// Creates a test SUBSCRIBE packet
pub fn create_test_subscribe(topics: Vec<(&str, QoS)>) -> Packet {
    Packet::Subscribe(SubscribePacket {
        packet_id: 1,
        properties: Properties::new(),
        filters: topics
            .into_iter()
            .map(|(filter, qos)| TopicFilter {
                filter: filter.to_string(),
                options: SubscriptionOptions {
                    qos,
                    no_local: false,
                    retain_as_published: false,
                    retain_handling: crate::packet::subscribe::RetainHandling::SendAtSubscribe,
                },
            })
            .collect(),
    })
}

/// Encodes a packet to bytes for testing
pub fn encode_packet(packet: Packet) -> Result<Vec<u8>, crate::error::MqttError> {
    use crate::encoding::encode_variable_int;

    let mut buf = BytesMut::with_capacity(1024);

    match packet {
        Packet::Connect(p) => {
            // Fixed header
            buf.extend_from_slice(&[0x10]); // CONNECT type
            let mut body = BytesMut::new();
            p.encode(&mut body)?;
            encode_variable_int(&mut buf, body.len().min(u32::MAX as usize) as u32)?;
            buf.extend_from_slice(&body);
        }
        Packet::ConnAck(p) => {
            buf.extend_from_slice(&[0x20]); // CONNACK type
            let mut body = BytesMut::new();
            p.encode(&mut body)?;
            encode_variable_int(&mut buf, body.len().min(u32::MAX as usize) as u32)?;
            buf.extend_from_slice(&body);
        }
        Packet::Publish(p) => {
            let flags = p.flags();
            buf.put_u8((u8::from(PacketType::Publish)) << 4 | flags);
            let mut body = BytesMut::new();
            p.encode(&mut body)?;
            encode_variable_int(&mut buf, body.len().min(u32::MAX as usize) as u32)?;
            buf.extend_from_slice(&body);
        }
        Packet::Subscribe(p) => {
            buf.extend_from_slice(&[0x82]); // SUBSCRIBE type with required flags
            let mut body = BytesMut::new();
            p.encode(&mut body)?;
            encode_variable_int(&mut buf, body.len().min(u32::MAX as usize) as u32)?;
            buf.extend_from_slice(&body);
        }
        Packet::SubAck(p) => {
            buf.extend_from_slice(&[0x90]); // SUBACK type
            let mut body = BytesMut::new();
            p.encode(&mut body)?;
            encode_variable_int(&mut buf, body.len().min(u32::MAX as usize) as u32)?;
            buf.extend_from_slice(&body);
        }
        Packet::PingReq => {
            buf.extend_from_slice(&[0xC0, 0x00]); // PINGREQ
        }
        Packet::PingResp => {
            buf.extend_from_slice(&[0xD0, 0x00]); // PINGRESP
        }
        _ => {
            return Err(crate::error::MqttError::ProtocolError(
                "Unsupported packet type in test".to_string(),
            ))
        }
    }

    Ok(buf.to_vec())
}

/// Asserts that two packets are equal
pub fn assert_packets_equal(actual: &Packet, expected: &Packet) {
    use Packet::*;

    match (actual, expected) {
        (Connect(a), Connect(e)) => {
            assert_eq!(a.protocol_version, e.protocol_version);
            assert_eq!(a.clean_start, e.clean_start);
            assert_eq!(a.keep_alive, e.keep_alive);
            assert_eq!(a.client_id, e.client_id);
        }
        (ConnAck(a), ConnAck(e)) => {
            assert_eq!(a.session_present, e.session_present);
            assert_eq!(a.reason_code, e.reason_code);
        }
        (Publish(a), Publish(e)) => {
            assert_eq!(a.topic_name, e.topic_name);
            assert_eq!(a.payload, e.payload);
            assert_eq!(a.qos, e.qos);
            assert_eq!(a.packet_id, e.packet_id);
        }
        (PingReq, PingReq) | (PingResp, PingResp) => {}
        _ => panic!("Packet types don't match: {:?} vs {:?}", actual, expected),
    }
}

/// Runs an async test with a timeout
pub async fn run_with_timeout<F, T>(duration: Duration, future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    match timeout(duration, future).await {
        Ok(result) => result,
        Err(_) => panic!("Test timed out after {:?}", duration),
    }
}

/// Helper macro for async tests with timeout
#[macro_export]
macro_rules! test_timeout {
    ($duration:expr, $body:expr) => {
        $crate::test_utils::run_with_timeout($duration, $body)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_builder_connect() {
        let packet = ConnectPacket::test_default();
        assert_eq!(packet.client_id, "test-client");
        assert_eq!(packet.protocol_version, 5);
        assert!(packet.clean_start);
    }

    #[test]
    fn test_create_publish() {
        let packet = create_test_publish("test/topic", b"hello", QoS::AtLeastOnce);
        match packet {
            Packet::Publish(p) => {
                assert_eq!(p.topic_name, "test/topic");
                assert_eq!(p.payload, b"hello");
                assert_eq!(p.qos, QoS::AtLeastOnce);
                assert_eq!(p.packet_id, Some(1));
            }
            _ => panic!("Wrong packet type"),
        }
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let original = create_test_connect();
        let encoded = encode_packet(original.clone()).unwrap();
        assert!(!encoded.is_empty());

        // Verify fixed header
        assert_eq!(encoded[0] >> 4, u8::from(PacketType::Connect));
    }

    #[tokio::test]
    async fn test_timeout_helper() {
        // Should complete
        let result = run_with_timeout(Duration::from_millis(100), async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            42
        })
        .await;
        assert_eq!(result, 42);

        // Should timeout
        let result = std::panic::catch_unwind(|| {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                run_with_timeout(Duration::from_millis(10), async {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    42
                })
                .await
            })
        });
        assert!(result.is_err());
    }
}
