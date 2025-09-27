use crate::broker::auth::AuthProvider;
use crate::broker::config::BrokerConfig;
use crate::broker::resource_monitor::ResourceMonitor;
use crate::broker::router::MessageRouter;
use crate::broker::storage::{DynamicStorage, StorageBackend};
use crate::broker::sys_topics::BrokerStats;
use crate::error::Result;
use crate::packet::connack::ConnAckPacket;
use crate::packet::connect::ConnectPacket;
use crate::packet::disconnect::DisconnectPacket;
use crate::packet::puback::PubAckPacket;
use crate::packet::pubcomp::PubCompPacket;
use crate::packet::publish::PublishPacket;
use crate::packet::pubrec::PubRecPacket;
use crate::packet::pubrel::PubRelPacket;
use crate::packet::suback::{SubAckPacket, SubAckReasonCode};
use crate::packet::subscribe::SubscribePacket;
use crate::packet::unsuback::{UnsubAckPacket, UnsubAckReasonCode};
use crate::packet::unsubscribe::UnsubscribePacket;
use crate::packet::{FixedHeader, Packet};
use crate::packet_id::PacketIdGenerator;
use crate::protocol::v5::properties::Properties;
use crate::transport::packet_io::encode_packet_to_buffer;
use crate::transport::udp_fragmentation::fragment_packet;
use crate::types::ReasonCode;
use crate::QoS;
use bytes::BytesMut;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Clone)]
pub struct UdpPacketHandler {
    config: Arc<BrokerConfig>,
    router: Arc<MessageRouter>,
    auth_provider: Arc<dyn AuthProvider>,
    storage: Option<Arc<DynamicStorage>>,
    stats: Arc<BrokerStats>,
    resource_monitor: Arc<ResourceMonitor>,
    packet_id_generator: PacketIdGenerator,
}

impl UdpPacketHandler {
    pub fn new(
        config: Arc<BrokerConfig>,
        router: Arc<MessageRouter>,
        auth_provider: Arc<dyn AuthProvider>,
        storage: Option<Arc<DynamicStorage>>,
        stats: Arc<BrokerStats>,
        resource_monitor: Arc<ResourceMonitor>,
    ) -> Self {
        Self {
            config,
            router,
            auth_provider,
            storage,
            stats,
            resource_monitor,
            packet_id_generator: PacketIdGenerator::new(),
        }
    }

    pub async fn handle_packet(
        &self,
        packet_data: &[u8],
        peer_addr: SocketAddr,
        session: &mut crate::broker::udp_session::UdpSession,
    ) -> Result<Option<Vec<u8>>> {
        let mut bytes = BytesMut::from(packet_data);
        let fixed_header = FixedHeader::decode(&mut bytes)?;

        let packet = Packet::decode_from_body(fixed_header.packet_type, &fixed_header, &mut bytes)?;

        match packet {
            Packet::Connect(connect) => self.handle_connect(*connect, peer_addr, session).await,
            Packet::Disconnect(disconnect) => self.handle_disconnect(disconnect, session).await,
            Packet::PingReq => self.handle_pingreq(session),
            Packet::Publish(publish) => self.handle_publish(publish, session).await,
            Packet::PubAck(puback) => Ok(self.handle_puback(puback, session)),
            Packet::PubRec(pubrec) => self.handle_pubrec(&pubrec, session),
            Packet::PubRel(pubrel) => self.handle_pubrel(&pubrel, session),
            Packet::PubComp(pubcomp) => Ok(self.handle_pubcomp(&pubcomp, session)),
            Packet::Subscribe(subscribe) => self.handle_subscribe(subscribe, session).await,
            Packet::Unsubscribe(unsubscribe) => self.handle_unsubscribe(unsubscribe, session).await,
            _ => {
                warn!("Unhandled packet type: {:?}", packet);
                Ok(None)
            }
        }
    }

    async fn handle_connect(
        &self,
        connect: ConnectPacket,
        peer_addr: SocketAddr,
        session: &mut crate::broker::udp_session::UdpSession,
    ) -> Result<Option<Vec<u8>>> {
        info!(
            "UDP CONNECT from {} with client_id: {}",
            peer_addr, connect.client_id
        );

        if session.connected {
            warn!("Already connected client attempted CONNECT");
            return Ok(None);
        }

        let mut connack = ConnAckPacket::new(false, ReasonCode::Success);

        if let Ok(auth_result) = self.auth_provider.authenticate(&connect, peer_addr).await {
            if !auth_result.authenticated {
                connack.reason_code = ReasonCode::BadUsernameOrPassword;
                return Ok(Some(self.encode_packet(&Packet::ConnAck(connack))?));
            }
        } else {
            connack.reason_code = ReasonCode::UnspecifiedError;
            return Ok(Some(self.encode_packet(&Packet::ConnAck(connack))?));
        }

        session.client_id = Some(connect.client_id.clone());
        session.connected = true;
        session.clean_start = connect.clean_start;

        // Handle session persistence
        let mut session_present = false;
        if let Some(storage) = &self.storage {
            let existing_session = storage.get_session(&connect.client_id).await?;

            if connect.clean_start || existing_session.is_none() {
                // Create new session
                let client_session = crate::broker::storage::ClientSession {
                    client_id: connect.client_id.clone(),
                    persistent: !connect.clean_start,
                    expiry_interval: connect.properties.get_session_expiry_interval(),
                    subscriptions: std::collections::HashMap::new(),
                    created_at: std::time::SystemTime::now(),
                    last_seen: std::time::SystemTime::now(),
                    will_message: connect.will.clone().map(|w| crate::types::WillMessage {
                        topic: w.topic,
                        payload: w.payload,
                        qos: w.qos,
                        retain: w.retain,
                        properties: w.properties,
                    }),
                    will_delay_interval: connect
                        .will
                        .as_ref()
                        .and_then(|w| w.properties.will_delay_interval),
                };
                storage.store_session(client_session).await?;
            } else if let Some(mut existing) = existing_session {
                session_present = true;
                // Restore subscriptions
                for (topic_filter, qos) in &existing.subscriptions {
                    self.router
                        .subscribe(connect.client_id.clone(), topic_filter.clone(), *qos, None)
                        .await;
                }
                // Update session
                existing.last_seen = std::time::SystemTime::now();
                existing.expiry_interval = connect.properties.get_session_expiry_interval();
                storage.store_session(existing).await?;
            }
        }

        // Register client with router to receive messages
        self.router
            .register_client(connect.client_id.clone(), session.publish_tx.clone())
            .await;

        self.stats.client_connected();

        connack.session_present = session_present;
        info!(
            "UDP client {} connected, session_present: {}",
            connect.client_id, session_present
        );

        // Deliver queued messages if session is present
        if session_present {
            self.deliver_queued_messages(&connect.client_id, session)
                .await?;
        }

        Ok(Some(self.encode_packet(&Packet::ConnAck(connack))?))
    }

    async fn handle_disconnect(
        &self,
        disconnect: DisconnectPacket,
        session: &mut crate::broker::udp_session::UdpSession,
    ) -> Result<Option<Vec<u8>>> {
        if let Some(client_id) = &session.client_id {
            info!("UDP client {} disconnecting", client_id);
            self.router.unregister_client(client_id).await;
            self.stats.client_disconnected();

            // Update session status in storage
            if let Some(ref storage) = self.storage {
                if let Ok(Some(mut client_session)) = storage.get_session(client_id).await {
                    client_session.last_seen = std::time::SystemTime::now();

                    // Handle session expiry from disconnect properties
                    if let Some(expiry) = disconnect.properties.get_session_expiry_interval() {
                        client_session.expiry_interval = Some(expiry);
                    }

                    // If session expiry is 0, remove the session
                    if client_session.expiry_interval == Some(0) {
                        if let Err(e) = storage.remove_session(client_id).await {
                            warn!("Failed to remove session for client {}: {}", client_id, e);
                        }
                    } else if let Err(e) = storage.store_session(client_session).await {
                        warn!("Failed to update session for client {}: {}", client_id, e);
                    }
                }
            }
        }
        session.connected = false;
        Ok(None)
    }

    fn handle_pingreq(
        &self,
        session: &mut crate::broker::udp_session::UdpSession,
    ) -> Result<Option<Vec<u8>>> {
        if !session.connected {
            return Ok(None);
        }

        debug!("PINGREQ from UDP client {:?}", session.client_id);
        Ok(Some(self.encode_packet(&Packet::PingResp)?))
    }

    async fn handle_publish(
        &self,
        publish: PublishPacket,
        session: &mut crate::broker::udp_session::UdpSession,
    ) -> Result<Option<Vec<u8>>> {
        if !session.connected {
            return Ok(None);
        }

        let client_id = session.client_id.as_ref().unwrap();
        debug!(
            "PUBLISH from UDP client {} on topic {}",
            client_id, publish.topic_name
        );

        self.router.route_message(&publish).await;

        match publish.qos {
            QoS::AtMostOnce => Ok(None),
            QoS::AtLeastOnce => {
                let mut puback = PubAckPacket::new(publish.packet_id.unwrap_or(0));
                puback.reason_code = ReasonCode::Success;
                puback.properties = Properties::new();
                Ok(Some(self.encode_packet(&Packet::PubAck(puback))?))
            }
            QoS::ExactlyOnce => {
                // Send PUBREC for QoS 2
                let packet_id = publish.packet_id.unwrap_or(0);
                let mut pubrec = PubRecPacket::new(packet_id);
                pubrec.reason_code = ReasonCode::Success;
                pubrec.properties = Properties::new();
                Ok(Some(self.encode_packet(&Packet::PubRec(pubrec))?))
            }
        }
    }

    fn handle_puback(
        &self,
        _puback: PubAckPacket,
        session: &mut crate::broker::udp_session::UdpSession,
    ) -> Option<Vec<u8>> {
        if !session.connected {
            return None;
        }

        debug!("PUBACK from UDP client {:?}", session.client_id);
        None
    }

    fn handle_pubrec(
        &self,
        pubrec: &PubRecPacket,
        session: &mut crate::broker::udp_session::UdpSession,
    ) -> Result<Option<Vec<u8>>> {
        if !session.connected {
            return Ok(None);
        }

        debug!(
            "PUBREC from UDP client {:?} for packet {}",
            session.client_id, pubrec.packet_id
        );

        // Send PUBREL in response
        let mut pubrel_packet = PubRelPacket::new(pubrec.packet_id);
        pubrel_packet.reason_code = ReasonCode::Success;
        pubrel_packet.properties = Properties::new();

        Ok(Some(self.encode_packet(&Packet::PubRel(pubrel_packet))?))
    }

    fn handle_pubrel(
        &self,
        pubrel: &PubRelPacket,
        session: &mut crate::broker::udp_session::UdpSession,
    ) -> Result<Option<Vec<u8>>> {
        if !session.connected {
            return Ok(None);
        }

        debug!(
            "PUBREL from UDP client {:?} for packet {}",
            session.client_id, pubrel.packet_id
        );

        // Send PUBCOMP in response
        let mut pubcomp = PubCompPacket::new(pubrel.packet_id);
        pubcomp.reason_code = ReasonCode::Success;
        pubcomp.properties = Properties::new();

        Ok(Some(self.encode_packet(&Packet::PubComp(pubcomp))?))
    }

    fn handle_pubcomp(
        &self,
        pubcomp: &PubCompPacket,
        session: &mut crate::broker::udp_session::UdpSession,
    ) -> Option<Vec<u8>> {
        if !session.connected {
            return None;
        }

        debug!(
            "PUBCOMP from UDP client {:?} for packet {}",
            session.client_id, pubcomp.packet_id
        );
        // QoS 2 flow complete
        None
    }

    async fn handle_subscribe(
        &self,
        subscribe: SubscribePacket,
        session: &mut crate::broker::udp_session::UdpSession,
    ) -> Result<Option<Vec<u8>>> {
        if !session.connected {
            return Ok(None);
        }

        let client_id = session.client_id.as_ref().unwrap();
        debug!(
            "SUBSCRIBE from UDP client {} to {} topics",
            client_id,
            subscribe.filters.len()
        );

        let mut return_codes = Vec::new();
        for sub in &subscribe.filters {
            self.router
                .subscribe(client_id.clone(), sub.filter.clone(), sub.options.qos, None)
                .await;
            return_codes.push(SubAckReasonCode::GrantedQoS0);

            // Update session with subscription
            if let Some(ref storage) = self.storage {
                if let Ok(Some(mut client_session)) = storage.get_session(client_id).await {
                    client_session
                        .subscriptions
                        .insert(sub.filter.clone(), sub.options.qos);
                    if let Err(e) = storage.store_session(client_session).await {
                        warn!("Failed to store session for client {}: {}", client_id, e);
                    }
                }
            }

            // Send retained messages if requested
            if sub.options.retain_handling != crate::packet::subscribe::RetainHandling::DoNotSend {
                let retained = self.router.get_retained_messages(&sub.filter).await;
                for mut msg in retained {
                    msg.retain = true;
                    // Send retained message to client via publish channel
                    if let Err(e) = session.publish_tx.try_send(msg) {
                        warn!("Failed to queue retained message for UDP client: {}", e);
                    }
                }
            }
        }

        let mut suback = SubAckPacket::new(subscribe.packet_id);
        suback.reason_codes = return_codes;
        suback.properties = Properties::new();
        Ok(Some(self.encode_packet(&Packet::SubAck(suback))?))
    }

    async fn handle_unsubscribe(
        &self,
        unsubscribe: UnsubscribePacket,
        session: &mut crate::broker::udp_session::UdpSession,
    ) -> Result<Option<Vec<u8>>> {
        if !session.connected {
            return Ok(None);
        }

        let client_id = session.client_id.as_ref().unwrap();
        debug!(
            "UNSUBSCRIBE from UDP client {} from {} topics",
            client_id,
            unsubscribe.filters.len()
        );

        let mut return_codes = Vec::new();
        for filter in &unsubscribe.filters {
            self.router.unsubscribe(client_id, filter).await;
            return_codes.push(UnsubAckReasonCode::Success);
        }

        let mut unsuback = UnsubAckPacket::new(unsubscribe.packet_id);
        unsuback.reason_codes = return_codes;
        unsuback.properties = Properties::new();
        Ok(Some(self.encode_packet(&Packet::UnsubAck(unsuback))?))
    }

    pub fn encode_packet(&self, packet: &Packet) -> Result<Vec<u8>> {
        let mut buf = BytesMut::with_capacity(1024);
        encode_packet_to_buffer(packet, &mut buf)?;
        Ok(buf.to_vec())
    }

    async fn deliver_queued_messages(
        &self,
        client_id: &str,
        session: &mut crate::broker::udp_session::UdpSession,
    ) -> Result<()> {
        // Get messages and clear them from storage
        let queued_messages = if let Some(ref storage) = self.storage {
            let messages = storage.get_queued_messages(client_id).await?;
            storage.remove_queued_messages(client_id).await?;
            messages
        } else {
            Vec::new()
        };

        if !queued_messages.is_empty() {
            info!(
                "Delivering {} queued messages to UDP client {}",
                queued_messages.len(),
                client_id
            );

            for msg in queued_messages {
                if let Err(e) = session.publish_tx.try_send(msg.to_publish_packet()) {
                    warn!("Failed to queue message for UDP client: {}", e);
                }
            }
        }

        Ok(())
    }

    pub fn fragment_packet(&self, packet_bytes: &[u8], mtu: usize) -> Vec<Vec<u8>> {
        // Get next packet ID for fragmentation
        let packet_id = self.packet_id_generator.next();

        // Use the shared fragmentation function
        fragment_packet(packet_bytes, mtu, packet_id)
    }
}
