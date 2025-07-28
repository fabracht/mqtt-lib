use crate::error::{MqttError, Result};
use bytes::{Buf, BufMut};

/// Maximum value that can be encoded as a variable byte integer (268,435,455)
pub const VARIABLE_BYTE_INT_MAX: u32 = 268_435_455;

/// Encodes a u32 value as a variable byte integer according to MQTT specification
///
/// # Rules:
/// - Values 0-127 use 1 byte
/// - Values 128-16,383 use 2 bytes
/// - Values 16,384-2,097,151 use 3 bytes
/// - Values 2,097,152-268,435,455 use 4 bytes
///
/// # Errors
///
/// Returns ``MqttError`::ProtocolError` if the value exceeds the maximum
pub fn encode_variable_int<B: BufMut>(buf: &mut B, value: u32) -> Result<()> {
    if value > VARIABLE_BYTE_INT_MAX {
        return Err(MqttError::ProtocolError(format!(
            "Variable byte integer value {value} exceeds maximum {VARIABLE_BYTE_INT_MAX}"
        )));
    }

    let mut val = value;
    loop {
        let mut byte = (val % 128) as u8;
        val /= 128;
        if val > 0 {
            byte |= 0x80; // Set continuation bit
        }
        buf.put_u8(byte);
        if val == 0 {
            break;
        }
    }

    Ok(())
}

/// Decodes a variable byte integer from the buffer
///
/// # Errors
///
/// Returns an error if:
/// - The buffer doesn't contain enough bytes
/// - The encoded value exceeds the maximum
/// - More than 4 bytes are used (protocol violation)
pub fn decode_variable_int<B: Buf>(buf: &mut B) -> Result<u32> {
    let mut value = 0u32;
    let mut multiplier = 1u32;
    let mut byte_count = 0;

    loop {
        if !buf.has_remaining() {
            return Err(MqttError::MalformedPacket(
                "Insufficient bytes for variable byte integer".to_string(),
            ));
        }

        byte_count += 1;
        if byte_count > 4 {
            return Err(MqttError::MalformedPacket(
                "Variable byte integer exceeds 4 bytes".to_string(),
            ));
        }

        let byte = buf.get_u8();
        value += u32::from(byte & 0x7F) * multiplier;

        if (byte & 0x80) == 0 {
            break;
        }

        multiplier *= 128;
        if multiplier > 128 * 128 * 128 {
            return Err(MqttError::MalformedPacket(
                "Variable byte integer overflow".to_string(),
            ));
        }
    }

    if value > VARIABLE_BYTE_INT_MAX {
        return Err(MqttError::MalformedPacket(format!(
            "Variable byte integer value {value} exceeds maximum"
        )));
    }

    Ok(value)
}

/// Calculates the number of bytes needed to encode a value as variable byte integer
#[must_use]
pub fn variable_int_len(value: u32) -> usize {
    match value {
        0..=127 => 1,
        128..=16_383 => 2,
        16_384..=2_097_151 => 3,
        2_097_152..=VARIABLE_BYTE_INT_MAX => 4,
        _ => 5, // Invalid, but we return 5 to indicate error
    }
}

/// Alias for variable_int_len for consistency
#[must_use]
pub fn encoded_variable_int_len(value: u32) -> usize {
    variable_int_len(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn test_encode_decode_single_byte() {
        let mut buf = BytesMut::new();

        // Test values that fit in 1 byte (0-127)
        for value in [0, 1, 127] {
            buf.clear();
            encode_variable_int(&mut buf, value).unwrap();
            assert_eq!(buf.len(), 1);

            let decoded = decode_variable_int(&mut buf).unwrap();
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn test_encode_decode_two_bytes() {
        let mut buf = BytesMut::new();

        // Test values that need 2 bytes (128-16383)
        for value in [128, 129, 16_383] {
            buf.clear();
            encode_variable_int(&mut buf, value).unwrap();
            assert_eq!(buf.len(), 2);

            let decoded = decode_variable_int(&mut buf).unwrap();
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn test_encode_decode_three_bytes() {
        let mut buf = BytesMut::new();

        // Test values that need 3 bytes (16384-2097151)
        for value in [16_384, 65_535, 2_097_151] {
            buf.clear();
            encode_variable_int(&mut buf, value).unwrap();
            assert_eq!(buf.len(), 3);

            let decoded = decode_variable_int(&mut buf).unwrap();
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn test_encode_decode_four_bytes() {
        let mut buf = BytesMut::new();

        // Test values that need 4 bytes (2097152-268435455)
        for value in [2_097_152, 268_435_455] {
            buf.clear();
            encode_variable_int(&mut buf, value).unwrap();
            assert_eq!(buf.len(), 4);

            let decoded = decode_variable_int(&mut buf).unwrap();
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn test_encode_value_too_large() {
        let mut buf = BytesMut::new();
        let result = encode_variable_int(&mut buf, VARIABLE_BYTE_INT_MAX + 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_insufficient_bytes() {
        let mut buf = BytesMut::new();
        buf.put_u8(0x80); // Continuation bit set but no following byte

        let result = decode_variable_int(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_too_many_bytes() {
        let mut buf = BytesMut::new();
        // 5 bytes with continuation bits
        buf.put_u8(0x80);
        buf.put_u8(0x80);
        buf.put_u8(0x80);
        buf.put_u8(0x80);
        buf.put_u8(0x01);

        let result = decode_variable_int(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_variable_int_len() {
        assert_eq!(variable_int_len(0), 1);
        assert_eq!(variable_int_len(127), 1);
        assert_eq!(variable_int_len(128), 2);
        assert_eq!(variable_int_len(16_383), 2);
        assert_eq!(variable_int_len(16_384), 3);
        assert_eq!(variable_int_len(2_097_151), 3);
        assert_eq!(variable_int_len(2_097_152), 4);
        assert_eq!(variable_int_len(VARIABLE_BYTE_INT_MAX), 4);
        assert_eq!(variable_int_len(VARIABLE_BYTE_INT_MAX + 1), 5); // Invalid
    }

    #[test]
    fn test_mqtt_spec_examples() {
        let mut buf = BytesMut::new();

        // Example from MQTT spec: 64 decimal = 0x40
        buf.clear();
        encode_variable_int(&mut buf, 64).unwrap();
        assert_eq!(buf[0], 0x40);

        // Example: 321 decimal = 0xC1 0x02
        buf.clear();
        encode_variable_int(&mut buf, 321).unwrap();
        assert_eq!(buf[0], 0xC1);
        assert_eq!(buf[1], 0x02);
    }
}
