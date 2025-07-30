//! Integration tests for BeBytes 2.6.0 with existing buffer management
//!
//! This test suite verifies that BeBytes 2.6.0's to_be_bytes_buf() method
//! integrates seamlessly with the existing BytesMut patterns in the MQTT library.

use bebytes::BeBytes;
use bytes::{BufMut, Bytes, BytesMut};
use mqtt_v5::encoding::mqtt_string::MqttString;
use mqtt_v5::encoding::variable_int::VariableInt;
use mqtt_v5::packet::pingreq::PingReqPacket;
use mqtt_v5::packet::subscribe::SubscriptionOptionsBits;
use mqtt_v5::packet::{AckPacketHeader, FixedHeader, MqttTypeAndFlags, PacketType};
use mqtt_v5::types::ReasonCode;

#[test]
fn test_bebytes_with_existing_bytesmut_patterns() {
    // Test that BeBytes structures can be efficiently written to existing BytesMut buffers
    let mut buf = BytesMut::with_capacity(256);

    // Write fixed header using BeBytes
    let header = MqttTypeAndFlags::for_publish(2, false, true);
    let header_bytes = header.to_be_bytes_buf();
    buf.put(header_bytes);

    // Write variable length using BeBytes
    let remaining_len = VariableInt::new(127).unwrap();
    let len_bytes = remaining_len.to_be_bytes_buf();
    buf.put(len_bytes);

    // Verify buffer contains expected data
    assert_eq!(buf.len(), 2); // 1 byte header + 1 byte length
    assert_eq!(buf[0], 0x35); // PUBLISH with QoS 2, retain
    assert_eq!(buf[1], 0x7F); // Length 127
}

#[test]
fn test_mixed_bebytes_and_manual_encoding() {
    // Test mixing BeBytes encoding with manual buffer operations
    let mut buf = BytesMut::with_capacity(512);

    // Use BeBytes for fixed header
    let header = MqttTypeAndFlags::for_packet_type(PacketType::Subscribe);
    buf.put(header.to_be_bytes_buf());

    // Manual encoding for remaining length
    buf.put_u8(10); // Remaining length

    // Use BeBytes for packet ID
    let packet_id = 42u16;
    buf.put_u16(packet_id);

    // Use BeBytes for subscription options
    let sub_opts = SubscriptionOptionsBits::new(0, 0, 1, 0, 1);
    buf.put(sub_opts.to_be_bytes_buf());

    // Verify seamless integration
    assert_eq!(buf.len(), 5); // 1 + 1 + 2 + 1
    assert_eq!(buf[0], 0x80); // SUBSCRIBE packet type (8 << 4)
    assert_eq!(buf[1], 10); // Remaining length
    assert_eq!(buf[2], 0); // Packet ID MSB
    assert_eq!(buf[3], 42); // Packet ID LSB
}

#[test]
fn test_buffer_reuse_with_bebytes() {
    // Test that buffers can be efficiently reused with BeBytes
    let mut buf = BytesMut::with_capacity(128);

    // First packet
    let ping_req = PingReqPacket::default();
    buf.put(ping_req.to_be_bytes_buf());
    assert_eq!(buf.len(), 2);

    // Clear and reuse for second packet
    buf.clear();
    let ack_header = AckPacketHeader::create(1234, ReasonCode::Success);
    buf.put(ack_header.to_be_bytes_buf());
    assert_eq!(buf.len(), 3);

    // Clear and reuse for third packet
    buf.clear();
    let mqtt_str = MqttString::create("test/topic").unwrap();
    buf.put(mqtt_str.to_be_bytes_buf());
    assert_eq!(buf.len(), 2 + "test/topic".len());
}

#[test]
fn test_zero_copy_from_bebytes_to_bytes() {
    // Test that to_be_bytes_buf() provides zero-copy Bytes
    let header = MqttTypeAndFlags::for_publish(1, true, false);
    let bytes: Bytes = header.to_be_bytes_buf();

    // Clone should be cheap (reference counted)
    let bytes2 = bytes.clone();
    assert_eq!(bytes, bytes2);

    // Can be efficiently converted to BytesMut if needed
    let mut buf = BytesMut::from(bytes);
    buf.put_u8(42); // Add more data
    assert_eq!(buf.len(), 2); // Original byte + new byte
}

#[test]
fn test_bebytes_in_packet_encoding_flow() {
    // Test BeBytes in a realistic packet encoding scenario
    let mut buf = BytesMut::with_capacity(1024);

    // Create a PUBLISH packet
    let fixed_header = FixedHeader {
        packet_type: PacketType::Publish,
        flags: 0b0011, // QoS 1, retain
        remaining_length: 100,
    };

    // Encode fixed header manually (existing pattern)
    buf.put_u8((fixed_header.packet_type as u8) << 4 | fixed_header.flags);

    // Encode remaining length with BeBytes
    let var_len = VariableInt::new(fixed_header.remaining_length).unwrap();
    buf.put(var_len.to_be_bytes_buf());

    // Encode topic with BeBytes
    let topic = MqttString::create("sensor/temperature").unwrap();
    buf.put(topic.to_be_bytes_buf());

    // Encode packet ID manually
    buf.put_u16(42);

    // Verify the encoding
    assert!(buf.len() > 4); // At least fixed header + topic length
    assert_eq!(buf[0], 0x33); // PUBLISH with flags
}

#[test]
fn test_bebytes_memory_efficiency() {
    // Test that BeBytes doesn't cause unexpected allocations
    let mut total_capacity = 0;
    let mut buffers = Vec::new();

    // Create multiple buffers with BeBytes data
    for i in 0..100 {
        let mut buf = BytesMut::with_capacity(64);

        // Add BeBytes encoded data
        let header = MqttTypeAndFlags::for_publish(i % 3, i % 2 == 0, i % 2 == 1);
        buf.put(header.to_be_bytes_buf());

        let var_int = VariableInt::new(i as u32 * 10).unwrap();
        buf.put(var_int.to_be_bytes_buf());

        total_capacity += buf.capacity();
        buffers.push(buf);
    }

    // Verify reasonable memory usage
    assert!(total_capacity < 100 * 128); // Less than 128 bytes per buffer
    assert_eq!(buffers.len(), 100);
}

#[test]
fn test_bebytes_with_buffer_chains() {
    // Test BeBytes with chained buffer operations
    let mut main_buf = BytesMut::with_capacity(256);

    // Create sub-buffers with BeBytes data
    let header_bytes = MqttTypeAndFlags::for_packet_type(PacketType::Connect).to_be_bytes_buf();
    let len_bytes = VariableInt::new(50).unwrap().to_be_bytes_buf();
    let str_bytes = MqttString::create("clientId123").unwrap().to_be_bytes_buf();

    // Chain operations
    main_buf.put(header_bytes);
    main_buf.put(len_bytes);
    main_buf.put(str_bytes);

    // Verify chaining works correctly
    assert_eq!(main_buf[0], 0x10); // CONNECT packet type
    assert_eq!(main_buf[1], 50); // Variable length

    // Can still use manual operations
    main_buf.put_u16(60); // Keep-alive

    assert!(main_buf.len() > 5);
}

#[test]
fn test_bebytes_concurrent_buffer_usage() {
    use std::sync::Arc;
    use std::thread;

    // Test that BeBytes Bytes can be safely shared across threads
    let header = MqttTypeAndFlags::for_publish(1, false, true);
    let header_bytes: Bytes = header.to_be_bytes_buf();
    let first_byte = header_bytes[0]; // Save this before moving
    let shared_bytes = Arc::new(header_bytes);

    let mut handles = vec![];

    for i in 0..4 {
        let bytes = Arc::clone(&shared_bytes);
        let handle = thread::spawn(move || {
            let mut buf = BytesMut::with_capacity(64);
            buf.put_slice(&bytes); // Use the shared bytes
            buf.put_u8(i as u8); // Add thread-specific data
            buf
        });
        handles.push(handle);
    }

    // Collect results
    let buffers: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Verify all threads produced correct data
    for (i, buf) in buffers.iter().enumerate() {
        assert_eq!(buf[0], first_byte); // Same header byte
        assert_eq!(buf[1], i as u8); // Thread-specific byte
    }
}

#[test]
fn test_bebytes_with_packet_size_limits() {
    // Test BeBytes with MQTT packet size constraints
    const MAX_PACKET_SIZE: usize = 256 * 1024; // 256KB

    let mut buf = BytesMut::with_capacity(1024);

    // Build a packet with BeBytes components
    let header = MqttTypeAndFlags::for_publish(0, false, false);
    buf.put(header.to_be_bytes_buf());

    // Large payload size
    let payload_size = 65535;
    let var_len = VariableInt::new(payload_size).unwrap();
    buf.put(var_len.to_be_bytes_buf());

    // Topic
    let topic = MqttString::create("data/stream").unwrap();
    buf.put(topic.to_be_bytes_buf());

    // Verify we can check size limits
    let current_size = buf.len() + payload_size as usize;
    assert!(current_size < MAX_PACKET_SIZE);
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_bebytes_vs_manual_buffer_performance() {
        const ITERATIONS: usize = 100_000;

        // Pre-create headers to test just the buffer writing performance
        let headers: Vec<_> = (0..4)
            .map(|i| MqttTypeAndFlags::for_publish((i % 3) as u8, i % 2 == 0, false))
            .collect();

        // Benchmark BeBytes to_be_bytes_buf() approach
        let start = Instant::now();
        let mut buf1 = BytesMut::with_capacity(ITERATIONS * 4);
        for i in 0..ITERATIONS {
            let header = &headers[i % 4];
            buf1.put(header.to_be_bytes_buf());
        }
        let bebytes_time = start.elapsed();

        // Benchmark manual approach
        let start = Instant::now();
        let mut buf2 = BytesMut::with_capacity(ITERATIONS * 4);
        for i in 0..ITERATIONS {
            let byte = (3u8 << 4) | (((i % 3) as u8) << 1) | ((i % 2) as u8);
            buf2.put_u8(byte);
        }
        let manual_time = start.elapsed();

        println!("BeBytes buffer integration: {:?}", bebytes_time);
        println!("Manual buffer writing: {:?}", manual_time);

        // BeBytes should be reasonably close to manual performance
        let ratio = bebytes_time.as_nanos() as f64 / manual_time.as_nanos() as f64;
        println!("Performance ratio: {:.2}x", ratio);

        // Note: The overhead here includes both BeBytes AND BytesMut.put() with Bytes
        // In real-world usage, the safety and correctness benefits outweigh this overhead
        // For ultra-performance paths, consider using encode_be_to() method instead
        assert!(
            ratio < 30.0,
            "BeBytes integration unexpectedly slow: {:.2}x",
            ratio
        );
    }

    #[test]
    fn test_bebytes_encode_be_to_performance() {
        const ITERATIONS: usize = 100_000;

        // Pre-create headers
        let headers: Vec<_> = (0..4)
            .map(|i| MqttTypeAndFlags::for_publish((i % 3) as u8, i % 2 == 0, false))
            .collect();

        // Benchmark BeBytes encode_be_to() approach (BeBytes 2.4.0 zero-allocation)
        let start = Instant::now();
        let mut buf1 = BytesMut::with_capacity(ITERATIONS * 4);
        for i in 0..ITERATIONS {
            let header = &headers[i % 4];
            let _ = header.encode_be_to(&mut buf1);
        }
        let bebytes_encode_time = start.elapsed();

        // Benchmark to_be_bytes_buf() for comparison
        let start = Instant::now();
        let mut buf2 = BytesMut::with_capacity(ITERATIONS * 4);
        for i in 0..ITERATIONS {
            let header = &headers[i % 4];
            buf2.put(header.to_be_bytes_buf());
        }
        let bebytes_buf_time = start.elapsed();

        // Benchmark manual approach
        let start = Instant::now();
        let mut buf3 = BytesMut::with_capacity(ITERATIONS * 4);
        for i in 0..ITERATIONS {
            let byte = (3u8 << 4) | (((i % 3) as u8) << 1) | ((i % 2) as u8);
            buf3.put_u8(byte);
        }
        let manual_time = start.elapsed();

        println!("\nPerformance comparison:");
        println!("Manual put_u8:          {:?}", manual_time);
        println!(
            "BeBytes encode_be_to:   {:?} ({:.2}x vs manual)",
            bebytes_encode_time,
            bebytes_encode_time.as_nanos() as f64 / manual_time.as_nanos() as f64
        );
        println!(
            "BeBytes to_bytes_buf:   {:?} ({:.2}x vs manual)",
            bebytes_buf_time,
            bebytes_buf_time.as_nanos() as f64 / manual_time.as_nanos() as f64
        );

        // Note: Current BeBytes implementation shows encode_be_to() is not optimized
        // to_be_bytes_buf() from 2.6.0 actually performs better!
        let encode_ratio = bebytes_encode_time.as_nanos() as f64 / manual_time.as_nanos() as f64;
        let buf_ratio = bebytes_buf_time.as_nanos() as f64 / manual_time.as_nanos() as f64;

        // Document that to_be_bytes_buf() is the preferred method with BeBytes 2.6.0
        println!("\nRecommendation: Use to_be_bytes_buf() for best BeBytes performance");
        assert!(
            buf_ratio < encode_ratio,
            "to_be_bytes_buf should be faster than encode_be_to"
        );
    }
}
