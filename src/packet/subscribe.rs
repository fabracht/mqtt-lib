use crate::encoding::{decode_string, encode_string};
use crate::error::{MqttError, Result};
use crate::packet::{FixedHeader, MqttPacket, PacketType};
use crate::protocol::v5::properties::Properties;
use crate::QoS;
use bytes::{Buf, BufMut};

/// Subscription options (v5.0)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscriptionOptions {
    /// Maximum `QoS` level the client will accept
    pub qos: QoS,
    /// No Local option - if true, Application Messages MUST NOT be forwarded to this connection
    pub no_local: bool,
    /// Retain As Published - if true, keep the RETAIN flag as published
    pub retain_as_published: bool,
    /// Retain Handling option
    pub retain_handling: RetainHandling,
}

/// Retain handling options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RetainHandling {
    /// Send retained messages at subscribe time
    SendAtSubscribe = 0,
    /// Send retained messages at subscribe time only if subscription doesn't exist
    SendAtSubscribeIfNew = 1,
    /// Don't send retained messages at subscribe time
    DoNotSend = 2,
}

impl Default for SubscriptionOptions {
    fn default() -> Self {
        Self {
            qos: QoS::AtMostOnce,
            no_local: false,
            retain_as_published: false,
            retain_handling: RetainHandling::SendAtSubscribe,
        }
    }
}

impl SubscriptionOptions {
    /// Creates subscription options with the specified `QoS`
    #[must_use]
    pub fn new(qos: QoS) -> Self {
        Self {
            qos,
            ..Default::default()
        }
    }

    /// Sets the `QoS` level
    #[must_use]
    pub fn with_qos(mut self, qos: QoS) -> Self {
        self.qos = qos;
        self
    }

    /// Encodes subscription options as a byte (v5.0)
    #[must_use]
    pub fn encode(&self) -> u8 {
        let mut byte = self.qos as u8;

        if self.no_local {
            byte |= 0x04;
        }

        if self.retain_as_published {
            byte |= 0x08;
        }

        byte |= (self.retain_handling as u8) << 4;

        byte
    }

    /// Decodes subscription options from a byte (v5.0)
    #[must_use]
    pub fn decode(byte: u8) -> Result<Self> {
        let qos_val = byte & 0x03;
        let qos = match qos_val {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            2 => QoS::ExactlyOnce,
            _ => {
                return Err(MqttError::MalformedPacket(format!(
                    "Invalid QoS value in subscription options: {}",
                    qos_val
                )))
            }
        };

        let no_local = (byte & 0x04) != 0;
        let retain_as_published = (byte & 0x08) != 0;

        let retain_handling_val = (byte >> 4) & 0x03;
        let retain_handling = match retain_handling_val {
            0 => RetainHandling::SendAtSubscribe,
            1 => RetainHandling::SendAtSubscribeIfNew,
            2 => RetainHandling::DoNotSend,
            _ => {
                return Err(MqttError::MalformedPacket(format!(
                    "Invalid retain handling value: {}",
                    retain_handling_val
                )))
            }
        };

        // Check reserved bits
        if (byte & 0xC0) != 0 {
            return Err(MqttError::MalformedPacket(
                "Reserved bits in subscription options must be 0".to_string(),
            ));
        }

        Ok(Self {
            qos,
            no_local,
            retain_as_published,
            retain_handling,
        })
    }
}

/// Topic filter with subscription options
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicFilter {
    /// Topic filter string (may contain wildcards)
    pub filter: String,
    /// Subscription options
    pub options: SubscriptionOptions,
}

impl TopicFilter {
    /// Creates a new topic filter with the specified `QoS`
    #[must_use]
    pub fn new(filter: impl Into<String>, qos: QoS) -> Self {
        Self {
            filter: filter.into(),
            options: SubscriptionOptions::new(qos),
        }
    }

    /// Creates a new topic filter with custom options
    #[must_use]
    pub fn with_options(filter: impl Into<String>, options: SubscriptionOptions) -> Self {
        Self {
            filter: filter.into(),
            options,
        }
    }
}

/// MQTT SUBSCRIBE packet
#[derive(Debug, Clone)]
pub struct SubscribePacket {
    /// Packet identifier
    pub packet_id: u16,
    /// Topic filters to subscribe to
    pub filters: Vec<TopicFilter>,
    /// SUBSCRIBE properties (v5.0 only)
    pub properties: Properties,
}

impl SubscribePacket {
    /// Creates a new SUBSCRIBE packet
    #[must_use]
    pub fn new(packet_id: u16) -> Self {
        Self {
            packet_id,
            filters: Vec::new(),
            properties: Properties::new(),
        }
    }

    /// Adds a topic filter
    #[must_use]
    pub fn add_filter(mut self, filter: impl Into<String>, qos: QoS) -> Self {
        self.filters.push(TopicFilter::new(filter, qos));
        self
    }

    /// Adds a topic filter with options
    #[must_use]
    pub fn add_filter_with_options(mut self, filter: TopicFilter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Sets the subscription identifier
    #[must_use]
    pub fn with_subscription_identifier(mut self, id: u32) -> Self {
        self.properties.set_subscription_identifier(id);
        self
    }

    /// Adds a user property
    #[must_use]
    pub fn with_user_property(mut self, key: String, value: String) -> Self {
        self.properties.add_user_property(key, value);
        self
    }
}

impl MqttPacket for SubscribePacket {
    fn packet_type(&self) -> PacketType {
        PacketType::Subscribe
    }

    fn flags(&self) -> u8 {
        0x02 // SUBSCRIBE must have flags = 0x02
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        // Variable header
        buf.put_u16(self.packet_id);

        // Properties (v5.0)
        self.properties.encode(buf)?;

        // Payload - topic filters
        if self.filters.is_empty() {
            return Err(MqttError::MalformedPacket(
                "SUBSCRIBE packet must contain at least one topic filter".to_string(),
            ));
        }

        for filter in &self.filters {
            encode_string(buf, &filter.filter)?;
            buf.put_u8(filter.options.encode());
        }

        Ok(())
    }

    fn decode_body<B: Buf>(buf: &mut B, fixed_header: &FixedHeader) -> Result<Self> {
        // Validate flags
        if fixed_header.flags != 0x02 {
            return Err(MqttError::MalformedPacket(format!(
                "Invalid SUBSCRIBE flags: expected 0x02, got 0x{:02X}",
                fixed_header.flags
            )));
        }

        // Packet identifier
        if buf.remaining() < 2 {
            return Err(MqttError::MalformedPacket(
                "SUBSCRIBE missing packet identifier".to_string(),
            ));
        }
        let packet_id = buf.get_u16();

        // Properties (v5.0)
        let properties = Properties::decode(buf)?;

        // Payload - topic filters
        let mut filters = Vec::new();

        if !buf.has_remaining() {
            return Err(MqttError::MalformedPacket(
                "SUBSCRIBE packet must contain at least one topic filter".to_string(),
            ));
        }

        while buf.has_remaining() {
            let filter_str = decode_string(buf)?;

            if !buf.has_remaining() {
                return Err(MqttError::MalformedPacket(
                    "Missing subscription options for topic filter".to_string(),
                ));
            }

            let options_byte = buf.get_u8();
            let options = SubscriptionOptions::decode(options_byte)?;

            filters.push(TopicFilter {
                filter: filter_str,
                options,
            });
        }

        Ok(Self {
            packet_id,
            filters,
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
    fn test_subscription_options_encode_decode() {
        let options = SubscriptionOptions {
            qos: QoS::AtLeastOnce,
            no_local: true,
            retain_as_published: true,
            retain_handling: RetainHandling::SendAtSubscribeIfNew,
        };

        let encoded = options.encode();
        assert_eq!(encoded, 0x1D); // QoS 1 + No Local + RAP + RH 1

        let decoded = SubscriptionOptions::decode(encoded).unwrap();
        assert_eq!(decoded, options);
    }

    #[test]
    fn test_subscribe_basic() {
        let packet = SubscribePacket::new(123)
            .add_filter("temperature/+", QoS::AtLeastOnce)
            .add_filter("humidity/#", QoS::ExactlyOnce);

        assert_eq!(packet.packet_id, 123);
        assert_eq!(packet.filters.len(), 2);
        assert_eq!(packet.filters[0].filter, "temperature/+");
        assert_eq!(packet.filters[0].options.qos, QoS::AtLeastOnce);
        assert_eq!(packet.filters[1].filter, "humidity/#");
        assert_eq!(packet.filters[1].options.qos, QoS::ExactlyOnce);
    }

    #[test]
    fn test_subscribe_with_options() {
        let options = SubscriptionOptions {
            qos: QoS::AtLeastOnce,
            no_local: true,
            retain_as_published: false,
            retain_handling: RetainHandling::DoNotSend,
        };

        let packet = SubscribePacket::new(456)
            .add_filter_with_options(TopicFilter::with_options("test/topic", options));

        assert_eq!(packet.filters[0].options.no_local, true);
        assert_eq!(
            packet.filters[0].options.retain_handling,
            RetainHandling::DoNotSend
        );
    }

    #[test]
    fn test_subscribe_encode_decode() {
        let packet = SubscribePacket::new(789)
            .add_filter("sensor/temp", QoS::AtMostOnce)
            .add_filter("sensor/humidity", QoS::AtLeastOnce)
            .with_subscription_identifier(42);

        let mut buf = BytesMut::new();
        packet.encode(&mut buf).unwrap();

        let fixed_header = FixedHeader::decode(&mut buf).unwrap();
        assert_eq!(fixed_header.packet_type, PacketType::Subscribe);
        assert_eq!(fixed_header.flags, 0x02);

        let decoded = SubscribePacket::decode_body(&mut buf, &fixed_header).unwrap();
        assert_eq!(decoded.packet_id, 789);
        assert_eq!(decoded.filters.len(), 2);
        assert_eq!(decoded.filters[0].filter, "sensor/temp");
        assert_eq!(decoded.filters[0].options.qos, QoS::AtMostOnce);
        assert_eq!(decoded.filters[1].filter, "sensor/humidity");
        assert_eq!(decoded.filters[1].options.qos, QoS::AtLeastOnce);

        let sub_id = decoded.properties.get(PropertyId::SubscriptionIdentifier);
        assert!(sub_id.is_some());
    }

    #[test]
    fn test_subscribe_invalid_flags() {
        let mut buf = BytesMut::new();
        buf.put_u16(123);

        let fixed_header = FixedHeader::new(PacketType::Subscribe, 0x00, 2); // Wrong flags
        let result = SubscribePacket::decode_body(&mut buf, &fixed_header);
        assert!(result.is_err());
    }

    #[test]
    fn test_subscribe_empty_filters() {
        let packet = SubscribePacket::new(123);

        let mut buf = BytesMut::new();
        let result = packet.encode(&mut buf);
        assert!(result.is_err());
    }
}
