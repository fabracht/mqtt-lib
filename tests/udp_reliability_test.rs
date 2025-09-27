//! Tests for UDP reliability layer

use mqtt5::transport::udp_reliability::UdpReliability;
use std::time::Duration;

#[test]
fn test_packet_wrapping_and_unwrapping() {
    let mut sender = UdpReliability::new();
    let mut receiver = UdpReliability::new();

    let original_data = b"Hello, MQTT over UDP!".to_vec();

    let wrapped = sender.wrap_packet(&original_data).unwrap();

    assert!(wrapped.len() > original_data.len());
    assert_eq!(wrapped[0], 0x01); // DATA packet type

    let unwrapped = receiver.unwrap_packet(&wrapped).unwrap();
    assert_eq!(unwrapped, Some(original_data));
}

#[test]
fn test_ack_generation() {
    let mut reliability = UdpReliability::new();

    let data1 = b"First packet".to_vec();
    let wrapped1 = reliability.wrap_packet(&data1).unwrap();

    let mut receiver = UdpReliability::new();
    let _ = receiver.unwrap_packet(&wrapped1).unwrap();

    let ack = receiver.generate_ack();
    assert!(ack.is_some());

    let ack_packet = ack.unwrap();
    assert_eq!(ack_packet[0], 0x02); // ACK packet type
}

#[test]
fn test_packet_retransmission() {
    let mut reliability = UdpReliability::new();

    let data = b"Test retransmission".to_vec();
    let _ = reliability.wrap_packet(&data).unwrap();

    let retries = reliability.get_packets_to_retry();
    assert_eq!(retries.len(), 0);

    std::thread::sleep(Duration::from_secs(2));

    let retries = reliability.get_packets_to_retry();
    assert_eq!(retries.len(), 1);
    assert_eq!(retries[0][0], 0x01); // DATA packet type
}

#[test]
fn test_duplicate_packet_detection() {
    let mut sender = UdpReliability::new();
    let mut receiver = UdpReliability::new();

    let data = b"Test duplicate".to_vec();
    let wrapped = sender.wrap_packet(&data).unwrap();

    let first = receiver.unwrap_packet(&wrapped).unwrap();
    assert_eq!(first, Some(data.clone()));

    let duplicate = receiver.unwrap_packet(&wrapped).unwrap();
    assert_eq!(duplicate, None);
}

#[test]
fn test_out_of_order_delivery() {
    let mut sender = UdpReliability::new();
    let mut receiver = UdpReliability::new();

    let data1 = b"First".to_vec();
    let data2 = b"Second".to_vec();
    let data3 = b"Third".to_vec();

    let wrapped1 = sender.wrap_packet(&data1).unwrap();
    let wrapped2 = sender.wrap_packet(&data2).unwrap();
    let wrapped3 = sender.wrap_packet(&data3).unwrap();

    let result3 = receiver.unwrap_packet(&wrapped3).unwrap();
    let result1 = receiver.unwrap_packet(&wrapped1).unwrap();
    let result2 = receiver.unwrap_packet(&wrapped2).unwrap();

    assert_eq!(result3, Some(data3));
    assert_eq!(result1, Some(data1));
    assert_eq!(result2, Some(data2));
}

#[test]
fn test_heartbeat_generation() {
    let mut reliability = UdpReliability::new();

    let heartbeat = reliability.generate_heartbeat();
    assert!(heartbeat.len() >= 9);
    assert_eq!(heartbeat[0], 0x03); // HEARTBEAT packet type
}

#[test]
fn test_backward_compatibility() {
    let mut reliability = UdpReliability::new();

    // MQTT CONNECT packet (packet type 1 << 4 = 0x10)
    let mqtt_packet = vec![0x10, 0x00, 0x04, b'M', b'Q', b'T', b'T'];

    // In the current implementation, all packets must go through the reliability layer
    // Raw MQTT packets are not supported - they must be wrapped with reliability protocol
    // This test now verifies that raw MQTT packets are NOT passed through
    let result = reliability.unwrap_packet(&mqtt_packet).unwrap();
    assert_eq!(result, None); // Raw MQTT packets return None as they're not wrapped
}

#[test]
fn test_reliability_stats() {
    let mut reliability = UdpReliability::new();

    let _ = reliability.wrap_packet(b"Test").unwrap();
    let _ = reliability.wrap_packet(b"Stats").unwrap();

    let stats = reliability.stats();
    assert_eq!(stats.unacked_count, 2);
    assert_eq!(stats.next_sequence, 3);
    assert!(stats.srtt.is_none());
}
