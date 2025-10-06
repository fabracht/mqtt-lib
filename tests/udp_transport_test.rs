#![cfg(feature = "udp")]

use mqtt5::transport::{Transport, UdpConfig, UdpTransport};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;

#[tokio::test]
async fn test_udp_transport_creation() {
    let addr: SocketAddr = "127.0.0.1:1883".parse().unwrap();
    let config = UdpConfig::new(addr)
        .with_connect_timeout(Duration::from_secs(5))
        .with_mtu(2048);

    let _transport = UdpTransport::new(config);
    // Transport should be created successfully (socket is private)
}

#[tokio::test]
async fn test_udp_transport_connect() {
    // Start a UDP echo server
    let server_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let server = UdpSocket::bind(server_addr).await.unwrap();
    let actual_addr = server.local_addr().unwrap();

    // Spawn echo server
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65507];
        loop {
            if let Ok((len, src)) = server.recv_from(&mut buf).await {
                let _ = server.send_to(&buf[..len], src).await;
            }
        }
    });

    // Create and connect client transport
    let config = UdpConfig::new(actual_addr).with_connect_timeout(Duration::from_secs(5));
    let mut transport = UdpTransport::new(config);

    // Connect should succeed
    let result = transport.connect().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_udp_fragmentation() {
    use mqtt5::transport::udp_fragmentation::FragmentHeader;

    // Test fragment header serialization
    let header = FragmentHeader {
        packet_id: 12345,
        fragment_index: 3,
        total_fragments: 10,
    };

    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), FragmentHeader::SIZE);

    let decoded = FragmentHeader::from_bytes(&bytes).unwrap();
    assert_eq!(header.packet_id, decoded.packet_id);
    assert_eq!(header.fragment_index, decoded.fragment_index);
    assert_eq!(header.total_fragments, decoded.total_fragments);
}
