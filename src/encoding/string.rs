use crate::error::{MqttError, Result};
use bytes::{Buf, BufMut};

/// Encodes a UTF-8 string with a 2-byte length prefix
///
/// # MQTT String Format:
/// - 2 bytes: string length (big-endian)
/// - N bytes: UTF-8 encoded string data
///
/// # Errors
///
/// Returns an error if:
/// - The string contains null characters
/// - The string length exceeds maximum string length
pub fn encode_string<B: BufMut>(buf: &mut B, string: &str) -> Result<()> {
    // Check for null characters
    if string.contains('\0') {
        return Err(MqttError::MalformedPacket(
            "String contains null character".to_string(),
        ));
    }

    let bytes = string.as_bytes();
    if bytes.len() > crate::constants::limits::MAX_STRING_LENGTH as usize {
        return Err(MqttError::MalformedPacket(format!(
            "String length {} exceeds maximum {}",
            bytes.len(),
            crate::constants::limits::MAX_STRING_LENGTH
        )));
    }

    // Write length as 2-byte big-endian
    // Safe cast: length was already validated to be <= u16::MAX
    #[allow(clippy::cast_possible_truncation)]
    buf.put_u16(bytes.len() as u16);
    // Write string data
    buf.put_slice(bytes);

    Ok(())
}

/// Decodes a UTF-8 string with a 2-byte length prefix
///
/// # Errors
///
/// Returns an error if:
/// - Insufficient bytes in buffer
/// - String is not valid UTF-8
/// - String contains null characters
pub fn decode_string<B: Buf>(buf: &mut B) -> Result<String> {
    if buf.remaining() < 2 {
        return Err(MqttError::MalformedPacket(
            "Insufficient bytes for string length".to_string(),
        ));
    }

    let len = buf.get_u16() as usize;

    if buf.remaining() < len {
        return Err(MqttError::MalformedPacket(format!(
            "Insufficient bytes for string data: expected {}, got {}",
            len,
            buf.remaining()
        )));
    }

    let mut bytes = vec![0u8; len];
    buf.copy_to_slice(&mut bytes);

    let string = String::from_utf8(bytes)
        .map_err(|e| MqttError::MalformedPacket(format!("Invalid UTF-8: {e}")))?;

    // Check for null characters
    if string.contains('\0') {
        return Err(MqttError::MalformedPacket(
            "String contains null character".to_string(),
        ));
    }

    Ok(string)
}

/// Calculates the encoded length of a string (2 bytes for length + string bytes)
#[must_use]
pub fn string_len(string: &str) -> usize {
    2 + string.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn test_encode_decode_string() {
        let mut buf = BytesMut::new();

        // Test various strings
        let long_string = "a".repeat(100);
        let test_strings = vec!["", "hello", "MQTT", "Hello, 世界!", &long_string];

        for test_str in &test_strings {
            buf.clear();
            encode_string(&mut buf, test_str).unwrap();

            let decoded = decode_string(&mut buf).unwrap();
            assert_eq!(&decoded, test_str);
        }
    }

    #[test]
    fn test_encode_string_with_null() {
        let mut buf = BytesMut::new();
        let result = encode_string(&mut buf, "hello\0world");
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_string_with_null() {
        let mut buf = BytesMut::new();
        // Manually create a string with null
        buf.put_u16(11);
        buf.put_slice(b"hello\0world");

        let result = decode_string(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_encode_string_too_long() {
        let mut buf = BytesMut::new();
        let long_string = "a".repeat(crate::constants::limits::MAX_BINARY_LENGTH as usize);
        let result = encode_string(&mut buf, &long_string);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_insufficient_length_bytes() {
        let mut buf = BytesMut::new();
        buf.put_u8(0); // Only 1 byte instead of 2

        let result = decode_string(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_insufficient_string_bytes() {
        let mut buf = BytesMut::new();
        buf.put_u16(10); // Claims 10 bytes
        buf.put_slice(b"hello"); // Only 5 bytes

        let result = decode_string(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_invalid_utf8() {
        let mut buf = BytesMut::new();
        buf.put_u16(3);
        buf.put_slice(&[0xFF, 0xFE, 0xFD]); // Invalid UTF-8

        let result = decode_string(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_string_len() {
        assert_eq!(string_len(""), 2);
        assert_eq!(string_len("MQTT"), 6); // 2 + 4
        assert_eq!(string_len("Hello, 世界!"), 2 + "Hello, 世界!".len());
    }

    #[test]
    fn test_empty_string() {
        let mut buf = BytesMut::new();
        encode_string(&mut buf, "").unwrap();

        assert_eq!(buf.len(), 2);
        assert_eq!(buf[0], 0);
        assert_eq!(buf[1], 0);

        let decoded = decode_string(&mut buf).unwrap();
        assert_eq!(decoded, "");
    }
}
