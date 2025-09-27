use crate::error::{MqttError, Result};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tracing::{debug, error, info};
use webrtc_dtls::cipher_suite::CipherSuiteId;
use webrtc_dtls::config::Config as WebRtcDtlsConfig;
use webrtc_dtls::crypto::Certificate;

pub struct DtlsHandler {
    socket: Arc<UdpSocket>,
    dtls_config: WebRtcDtlsConfig,
}

impl DtlsHandler {
    pub fn new(socket: Arc<UdpSocket>, config: &crate::broker::config::DtlsConfig) -> Result<Self> {
        let dtls_config = Self::create_dtls_config(config)?;

        Ok(Self {
            socket,
            dtls_config,
        })
    }

    fn create_dtls_config(config: &crate::broker::config::DtlsConfig) -> Result<WebRtcDtlsConfig> {
        if let (Some(identity), Some(key)) = (&config.psk_identity, &config.psk_key) {
            let key_clone = key.clone();
            let psk_callback = Arc::new(
                move |_hint: &[u8]| -> std::result::Result<Vec<u8>, webrtc_dtls::Error> {
                    Ok(key_clone.clone())
                },
            );

            info!("DTLS configured with PSK authentication");
            Ok(WebRtcDtlsConfig {
                psk: Some(psk_callback),
                psk_identity_hint: Some(identity.clone()),
                cipher_suites: vec![
                    CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8,
                    CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256,
                ],
                ..Default::default()
            })
        } else if let (Some(cert_path), Some(key_path)) = (&config.cert_file, &config.key_file) {
            let cert_pem = std::fs::read_to_string(cert_path)
                .map_err(|e| MqttError::Io(format!("Failed to read certificate: {e}")))?;
            let key_pem = std::fs::read_to_string(key_path)
                .map_err(|e| MqttError::Io(format!("Failed to read key: {e}")))?;

            let combined_pem = format!("{}{}", key_pem, cert_pem);
            let certificate = Certificate::from_pem(&combined_pem)
                .map_err(|e| MqttError::ConnectionError(format!("Invalid certificate: {e}")))?;

            info!("DTLS configured with certificate authentication");
            Ok(WebRtcDtlsConfig {
                certificates: vec![certificate],
                cipher_suites: vec![
                    CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256,
                    CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256,
                    CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm,
                    CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm_8,
                ],
                insecure_skip_verify: config.ca_file.is_none(),
                ..Default::default()
            })
        } else {
            Err(MqttError::Configuration(
                "DTLS configuration requires either PSK or certificates".to_string(),
            ))
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("DTLS handler ready - certificate support enabled, full session management pending");

        let mut buf = vec![0u8; 65507];
        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    debug!(addr = %addr, len = len, "Received DTLS packet - session management not yet implemented");
                }
                Err(e) => {
                    error!(error = %e, "Failed to receive DTLS datagram");
                    return Err(MqttError::Io(e.to_string()));
                }
            }
        }
    }
}
