use mqtt5::transport::{DtlsConfig, DtlsTransport, Transport};
use std::net::SocketAddr;
use std::time::Duration;

#[tokio::test]
async fn test_dtls_transport_creation() {
    let addr: SocketAddr = "127.0.0.1:8883".parse().unwrap();
    let config = DtlsConfig::new(addr)
        .with_connect_timeout(Duration::from_secs(5))
        .with_mtu(2048)
        .with_psk(b"client-identity".to_vec(), b"secret-psk-key".to_vec());

    let transport = DtlsTransport::new(config);
    assert_eq!(transport.remote_addr(), addr);
}

#[tokio::test]
async fn test_dtls_psk_connection() {
    // Initialize the crypto provider for tests
    let _ = rustls::crypto::ring::default_provider().install_default();

    let addr: SocketAddr = "127.0.0.1:8883".parse().unwrap();

    let config = DtlsConfig::new(addr)
        .with_connect_timeout(Duration::from_secs(5))
        .with_psk(b"client-identity".to_vec(), b"secret-psk-key".to_vec());

    let mut transport = DtlsTransport::new(config);

    // This will fail without a DTLS server, hence the ignore attribute
    let result = transport.connect().await;
    assert!(result.is_err()); // Expected to fail without server
}

#[tokio::test]
async fn test_dtls_config_builder() {
    let addr: SocketAddr = "127.0.0.1:8883".parse().unwrap();

    // Test PSK configuration
    let psk_config = DtlsConfig::new(addr)
        .with_psk(b"identity".to_vec(), b"key".to_vec())
        .with_mtu(1024)
        .with_connect_timeout(Duration::from_secs(10));

    assert_eq!(psk_config.addr, addr);
    assert_eq!(psk_config.mtu, 1024);
    assert_eq!(psk_config.connect_timeout, Duration::from_secs(10));
    assert!(psk_config.psk_identity.is_some());
    assert!(psk_config.psk_key.is_some());

    // Test certificate configuration (placeholder)
    let cert_config = DtlsConfig::new(addr).with_certificates(
        b"cert_pem".to_vec(),
        b"key_pem".to_vec(),
        Some(b"ca_cert".to_vec()),
    );

    assert!(cert_config.cert_pem.is_some());
    assert!(cert_config.key_pem.is_some());
    assert!(cert_config.ca_cert_pem.is_some());
}

#[tokio::test]
async fn test_dtls_fragmentation_header() {
    use mqtt5::transport::udp::FragmentHeader;

    // Test fragment header for DTLS (reuses UDP fragmentation)
    let header = FragmentHeader {
        packet_id: 0xABCD,
        fragment_index: 5,
        total_fragments: 10,
    };

    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), FragmentHeader::SIZE);

    let decoded = FragmentHeader::from_bytes(&bytes).unwrap();
    assert_eq!(header.packet_id, decoded.packet_id);
    assert_eq!(header.fragment_index, decoded.fragment_index);
    assert_eq!(header.total_fragments, decoded.total_fragments);
}
