use crate::broker::router::MessageRouter;
use crate::error::{MqttError, Result};
use crate::packet::publish::PublishPacket;
use crate::transport::udp_fragmentation::FragmentReassembler;
use crate::transport::udp_reliability::UdpReliability;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tracing::debug;

const SESSION_TIMEOUT: Duration = Duration::from_secs(300);

pub struct UdpSession {
    pub peer_addr: SocketAddr,
    pub client_id: Option<String>,
    pub last_activity: Instant,
    pub connected: bool,
    pub clean_start: bool,
    fragment_reassembler: FragmentReassembler,
    response_tx: mpsc::Sender<Vec<u8>>,
    response_rx: Option<mpsc::Receiver<Vec<u8>>>,
    pub publish_tx: mpsc::Sender<PublishPacket>,
    publish_rx: Option<mpsc::Receiver<PublishPacket>>,
    pub reliability: Arc<Mutex<UdpReliability>>,
}

impl UdpSession {
    pub fn new(peer_addr: SocketAddr) -> Self {
        let (response_tx, response_rx) = mpsc::channel(100);
        let (publish_tx, publish_rx) = mpsc::channel(100);
        Self {
            peer_addr,
            client_id: None,
            last_activity: Instant::now(),
            connected: false,
            clean_start: true,
            fragment_reassembler: FragmentReassembler::new(),
            response_tx,
            response_rx: Some(response_rx),
            publish_tx,
            publish_rx: Some(publish_rx),
            reliability: Arc::new(Mutex::new(UdpReliability::new())),
        }
    }

    pub fn take_response_rx(&mut self) -> Option<mpsc::Receiver<Vec<u8>>> {
        self.response_rx.take()
    }

    pub fn take_publish_rx(&mut self) -> Option<mpsc::Receiver<PublishPacket>> {
        self.publish_rx.take()
    }

    pub fn send_response(&self, data: Vec<u8>) -> Result<()> {
        self.response_tx
            .try_send(data)
            .map_err(|_| MqttError::ConnectionError("Channel send failed".to_string()))?;
        Ok(())
    }

    pub fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    pub fn is_expired(&self) -> bool {
        self.last_activity.elapsed() > SESSION_TIMEOUT
    }

    pub async fn process_packet_with_reliability(
        &mut self,
        data: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        // First, unwrap the reliability layer
        let mut reliability = self.reliability.lock().await;
        let result = reliability.unwrap_packet(data)?;
        drop(reliability); // Drop the lock before calling mutable methods

        match result {
            Some(payload) => {
                // Now process the actual MQTT packet or fragment
                self.reassemble_fragment(&payload)
            }
            None => {
                // This was a reliability control packet (ACK, heartbeat, etc.)
                Ok(None)
            }
        }
    }

    pub async fn wrap_packet_with_reliability(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let mut reliability = self.reliability.lock().await;
        reliability.wrap_packet(data)
    }

    pub async fn get_pending_acks(&mut self) -> Option<Vec<u8>> {
        let mut reliability = self.reliability.lock().await;
        reliability.generate_ack()
    }

    pub async fn get_packets_to_retry(&mut self) -> Vec<Vec<u8>> {
        let mut reliability = self.reliability.lock().await;
        reliability.get_packets_to_retry()
    }

    pub fn reassemble_fragment(&mut self, data: &[u8]) -> Result<Option<Vec<u8>>> {
        // Use the shared fragment reassembler
        self.fragment_reassembler.reassemble_fragment(data)
    }
}

pub struct UdpSessionManager {
    sessions: Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<UdpSession>>>>>,
    router: Arc<MessageRouter>,
}

impl UdpSessionManager {
    pub fn new(router: Arc<MessageRouter>) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            router,
        }
    }

    pub async fn get_or_create_session(&self, peer_addr: SocketAddr) -> Arc<Mutex<UdpSession>> {
        let mut sessions = self.sessions.lock().await;

        // Clean up expired sessions
        let expired: Vec<SocketAddr> = sessions
            .iter()
            .filter(|(_, session)| {
                let session = session.blocking_lock();
                session.is_expired()
            })
            .map(|(addr, _)| *addr)
            .collect();

        for addr in expired {
            sessions.remove(&addr);
        }

        let session = sessions
            .entry(peer_addr)
            .or_insert_with(|| {
                debug!("Creating new UDP session for {}", peer_addr);
                Arc::new(Mutex::new(UdpSession::new(peer_addr)))
            })
            .clone();

        session.blocking_lock().update_activity();
        session
    }

    pub async fn remove_session(&self, peer_addr: &SocketAddr) {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.remove(peer_addr) {
            let session = session.lock().await;
            if let Some(client_id) = &session.client_id {
                debug!("Removing UDP session for client {}", client_id);
                self.router.unregister_client(client_id).await;
            }
        }
    }

    pub async fn cleanup_expired(&self) {
        let mut sessions = self.sessions.lock().await;
        let expired: Vec<SocketAddr> = sessions
            .iter()
            .filter(|(_, session)| {
                let session = session.blocking_lock();
                session.is_expired()
            })
            .map(|(addr, _)| *addr)
            .collect();

        for addr in expired {
            if let Some(session) = sessions.remove(&addr) {
                let session = session.lock().await;
                if let Some(client_id) = &session.client_id {
                    debug!("Removing expired UDP session for client {}", client_id);
                    self.router.unregister_client(client_id).await;
                }
            }
        }
    }
}
