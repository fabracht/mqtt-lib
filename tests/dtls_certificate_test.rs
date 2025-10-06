//! Tests for certificate-based DTLS support

use mqtt5::MqttClient;

#[tokio::test]
async fn test_dtls_certificate_support_in_transport() {
    use mqtt5::transport::dtls::DtlsConfig;
    use std::net::SocketAddr;

    let cert_pem = b"-----BEGIN CERTIFICATE-----
MIIBkTCB+wIJAKHHIG...
-----END CERTIFICATE-----";

    let key_pem = b"-----BEGIN PRIVATE KEY-----
MIICdgIBADANBgk...
-----END PRIVATE KEY-----";

    let addr: SocketAddr = "127.0.0.1:8884".parse().unwrap();
    let config = DtlsConfig::new(addr).with_certificates(cert_pem.to_vec(), key_pem.to_vec(), None);

    assert!(config.cert_pem.is_some());
    assert!(config.key_pem.is_some());
    assert!(config.ca_cert_pem.is_none());
}

#[tokio::test]
async fn test_dtls_psk_support_in_transport() {
    use mqtt5::transport::dtls::DtlsConfig;
    use std::net::SocketAddr;

    let addr: SocketAddr = "127.0.0.1:8884".parse().unwrap();
    let config = DtlsConfig::new(addr).with_psk(b"client1".to_vec(), vec![0x01, 0x02, 0x03, 0x04]);

    assert!(config.psk_identity.is_some());
    assert!(config.psk_key.is_some());
}

#[tokio::test]
async fn test_client_dtls_config_api() {
    let client = MqttClient::new("test-client");

    let cert_pem = b"cert".to_vec();
    let key_pem = b"key".to_vec();

    client
        .set_dtls_config(
            Some(cert_pem.clone()),
            Some(key_pem.clone()),
            None,
            None,
            None,
        )
        .await;

    let psk_identity = b"client1".to_vec();
    let psk_key = vec![0x01, 0x02, 0x03, 0x04];

    client
        .set_dtls_config(None, None, None, Some(psk_identity), Some(psk_key))
        .await;
}

#[test]
fn test_broker_dtls_config() {
    use mqtt5::broker::config::DtlsConfig;
    use std::path::PathBuf;

    let config = DtlsConfig {
        bind_addresses: vec!["0.0.0.0:8884".parse().unwrap()],
        mtu: 1500,
        psk_identity: Some(b"broker".to_vec()),
        psk_key: Some(vec![0x01, 0x02, 0x03, 0x04]),
        cert_file: None,
        key_file: None,
        ca_file: None,
    };

    assert!(config.psk_identity.is_some());
    assert!(config.cert_file.is_none());

    let cert_config = DtlsConfig {
        bind_addresses: vec!["0.0.0.0:8884".parse().unwrap()],
        mtu: 1500,
        psk_identity: None,
        psk_key: None,
        cert_file: Some(PathBuf::from("server.pem")),
        key_file: Some(PathBuf::from("server.key")),
        ca_file: Some(PathBuf::from("ca.pem")),
    };

    assert!(cert_config.cert_file.is_some());
    assert!(cert_config.psk_identity.is_none());
}
