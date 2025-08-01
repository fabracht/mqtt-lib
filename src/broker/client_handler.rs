//! Client connection handler for the MQTT broker - simplified version

use crate::broker::auth::AuthProvider;
use crate::broker::config::BrokerConfig;
use crate::broker::router::MessageRouter;
use crate::broker::tcp_stream_wrapper::TcpStreamWrapper;
use crate::error::{MqttError, Result};
use crate::packet::connack::ConnAckPacket;
use crate::packet::connect::ConnectPacket;
use crate::packet::disconnect::DisconnectPacket;
use crate::packet::puback::PubAckPacket;
use crate::packet::pubcomp::PubCompPacket;
use crate::packet::publish::PublishPacket;
use crate::packet::pubrec::PubRecPacket;
use crate::packet::pubrel::PubRelPacket;
use crate::packet::suback::SubAckPacket;
use crate::packet::subscribe::SubscribePacket;
use crate::packet::unsuback::UnsubAckPacket;
use crate::packet::unsubscribe::UnsubscribePacket;
use crate::packet::Packet;
use crate::protocol::v5::reason_codes::ReasonCode;
use crate::transport::packet_io::PacketIo;
use crate::QoS;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, trace, warn};

/// Handles a single client connection
pub struct ClientHandler {
    transport: TcpStreamWrapper,
    client_addr: SocketAddr,
    config: Arc<BrokerConfig>,
    router: Arc<MessageRouter>,
    auth_provider: Arc<dyn AuthProvider>,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    client_id: Option<String>,
    user_id: Option<String>,
    keep_alive: Duration,
    publish_rx: mpsc::Receiver<PublishPacket>,
    publish_tx: mpsc::Sender<PublishPacket>,
    inflight_publishes: HashMap<u16, PublishPacket>,
}

impl ClientHandler {
    /// Creates a new client handler
    pub fn new(
        stream: TcpStream,
        client_addr: SocketAddr,
        config: Arc<BrokerConfig>,
        router: Arc<MessageRouter>,
        auth_provider: Arc<dyn AuthProvider>,
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Self {
        let (publish_tx, publish_rx) = mpsc::channel(100);
        
        Self {
            transport: TcpStreamWrapper::new(stream),
            client_addr,
            config,
            router,
            auth_provider,
            shutdown_rx,
            client_id: None,
            user_id: None,
            keep_alive: Duration::from_secs(60),
            publish_rx,
            publish_tx,
            inflight_publishes: HashMap::new(),
        }
    }
    
    /// Runs the client handler until disconnection or error
    /// 
    /// # Errors
    /// 
    /// Returns an error if transport operations fail or authentication fails
    /// 
    /// # Panics
    /// 
    /// Panics if `client_id` is None after successful connection
    pub async fn run(mut self) -> Result<()> {
        // Wait for CONNECT packet
        let connect_timeout = Duration::from_secs(10);
        match timeout(connect_timeout, self.wait_for_connect()).await {
            Ok(Ok(())) => {
                // Successfully connected
                info!("Client {} connected from {}", self.client_id.as_ref().unwrap(), self.client_addr);
            }
            Ok(Err(e)) => {
                error!("Connect error: {}", e);
                return Err(e);
            }
            Err(_) => {
                warn!("Connect timeout from {}", self.client_addr);
                return Err(MqttError::Timeout);
            }
        }
        
        // Register with router
        let client_id = self.client_id.as_ref().unwrap().clone();
        self.router.register_client(client_id.clone(), self.publish_tx.clone()).await;
        
        // Start keep-alive timer
        let mut keep_alive_interval = interval(self.keep_alive);
        keep_alive_interval.reset();
        
        // Handle packets until disconnect
        let result = self.handle_packets(&mut keep_alive_interval).await;
        
        // Cleanup
        self.router.unregister_client(&client_id).await;
        info!("Client {} disconnected", client_id);
        
        result
    }
    
    /// Waits for and processes CONNECT packet
    async fn wait_for_connect(&mut self) -> Result<()> {
        let packet = self.transport.read_packet().await?;
        
        match packet {
            Packet::Connect(connect) => {
                self.handle_connect(*connect).await
            }
            _ => {
                Err(MqttError::ProtocolError("Expected CONNECT packet".to_string()))
            }
        }
    }
    
    /// Handles incoming packets
    async fn handle_packets(&mut self, keep_alive_interval: &mut tokio::time::Interval) -> Result<()> {
        let mut last_packet_time = tokio::time::Instant::now();
        
        loop {
            tokio::select! {
                // Read incoming packets
                packet_result = self.transport.read_packet() => {
                    match packet_result {
                        Ok(packet) => {
                            last_packet_time = tokio::time::Instant::now();
                            self.handle_packet(packet).await?;
                        }
                        Err(MqttError::Io(e)) if e.contains("stream has been shut down") => {
                            debug!("Client disconnected");
                            return Ok(());
                        }
                        Err(e) => {
                            error!("Read error: {}", e);
                            return Err(e);
                        }
                    }
                }
                
                // Send outgoing publishes
                Some(publish) = self.publish_rx.recv() => {
                    self.send_publish(publish).await?;
                }
                
                // Keep-alive check
                _ = keep_alive_interval.tick() => {
                    let elapsed = last_packet_time.elapsed();
                    if elapsed > self.keep_alive + Duration::from_secs(self.keep_alive.as_secs() / 2) {
                        warn!("Keep-alive timeout");
                        return Err(MqttError::KeepAliveTimeout);
                    }
                }
                
                // Shutdown signal
                _ = self.shutdown_rx.recv() => {
                    debug!("Shutdown signal received");
                    let disconnect = DisconnectPacket::new(ReasonCode::ServerShuttingDown);
                    let _ = self.transport.write_packet(Packet::Disconnect(disconnect)).await;
                    return Ok(());
                }
            }
        }
    }
    
    /// Handles a single packet
    async fn handle_packet(&mut self, packet: Packet) -> Result<()> {
        trace!("Received packet: {:?}", packet);
        
        match packet {
            Packet::Connect(_) => {
                // Duplicate CONNECT
                let disconnect = DisconnectPacket::new(ReasonCode::ProtocolError);
                self.transport.write_packet(Packet::Disconnect(disconnect)).await?;
                Err(MqttError::ProtocolError("Duplicate CONNECT".to_string()))
            }
            Packet::Subscribe(subscribe) => self.handle_subscribe(subscribe).await,
            Packet::Unsubscribe(unsubscribe) => self.handle_unsubscribe(unsubscribe).await,
            Packet::Publish(publish) => self.handle_publish(publish).await,
            Packet::PubAck(puback) => {
                self.handle_puback(puback);
                Ok(())
            }
            Packet::PubRec(pubrec) => self.handle_pubrec(pubrec).await,
            Packet::PubRel(pubrel) => self.handle_pubrel(pubrel).await,
            Packet::PubComp(pubcomp) => {
                self.handle_pubcomp(pubcomp);
                Ok(())
            }
            Packet::PingReq => self.handle_pingreq().await,
            Packet::Disconnect(disconnect) => self.handle_disconnect(disconnect),
            _ => {
                warn!("Unexpected packet type");
                Ok(())
            }
        }
    }
    
    /// Handles CONNECT packet
    async fn handle_connect(&mut self, connect: ConnectPacket) -> Result<()> {
        // Authenticate
        let auth_result = self.auth_provider.authenticate(&connect, self.client_addr).await?;
        
        if !auth_result.authenticated {
            let connack = ConnAckPacket::new(false, auth_result.reason_code);
            self.transport.write_packet(Packet::ConnAck(connack)).await?;
            return Err(MqttError::AuthenticationFailed);
        }
        
        // Store client info
        self.client_id = Some(connect.client_id.clone());
        self.user_id = auth_result.user_id;
        self.keep_alive = Duration::from_secs(u64::from(connect.keep_alive));
        
        // Send CONNACK
        let mut connack = ConnAckPacket::new(false, ReasonCode::Success);
        
        // Set broker properties using setter methods
        connack.properties.set_topic_alias_maximum(self.config.topic_alias_maximum);
        connack.properties.set_retain_available(self.config.retain_available);
        connack.properties.set_maximum_packet_size(u32::try_from(self.config.max_packet_size).unwrap_or(u32::MAX));
        connack.properties.set_wildcard_subscription_available(self.config.wildcard_subscription_available);
        connack.properties.set_subscription_identifier_available(self.config.subscription_identifier_available);
        connack.properties.set_shared_subscription_available(self.config.shared_subscription_available);
        connack.properties.set_maximum_qos(self.config.maximum_qos);
        
        if let Some(keep_alive) = self.config.server_keep_alive {
            connack.properties.set_server_keep_alive(u16::try_from(keep_alive.as_secs()).unwrap_or(u16::MAX));
        }
        
        self.transport.write_packet(Packet::ConnAck(connack)).await
    }
    
    /// Handles SUBSCRIBE packet
    async fn handle_subscribe(&mut self, subscribe: SubscribePacket) -> Result<()> {
        let client_id = self.client_id.as_ref().unwrap();
        let mut reason_codes: Vec<crate::packet::suback::SubAckReasonCode> = Vec::new();
        
        for filter in &subscribe.filters {
            // Check authorization
            let authorized = self.auth_provider
                .authorize_subscribe(client_id, self.user_id.as_deref(), &filter.filter)
                .await?;
            
            if !authorized {
                reason_codes.push(crate::packet::suback::SubAckReasonCode::NotAuthorized);
                continue;
            }
            
            // Check QoS limit
            let granted_qos = if filter.options.qos as u8 > self.config.maximum_qos {
                self.config.maximum_qos
            } else {
                filter.options.qos as u8
            };
            
            // Add subscription
            self.router.subscribe(
                client_id.clone(),
                filter.filter.clone(),
                QoS::from(granted_qos),
                None,
            ).await;
            
            // Send retained messages if requested
            if filter.options.retain_handling != crate::packet::subscribe::RetainHandling::DoNotSend {
                let retained = self.router.get_retained_messages(&filter.filter).await;
                for mut msg in retained {
                    msg.retain = true;
                    self.publish_tx.send(msg).await.map_err(|_| {
                        MqttError::InvalidState("Failed to queue retained message".to_string())
                    })?;
                }
            }
            
            reason_codes.push(crate::packet::suback::SubAckReasonCode::from_qos(QoS::from(granted_qos)));
        }
        
        let mut suback = SubAckPacket::new(subscribe.packet_id);
        suback.reason_codes = reason_codes;
        self.transport.write_packet(Packet::SubAck(suback)).await
    }
    
    /// Handles UNSUBSCRIBE packet
    async fn handle_unsubscribe(&mut self, unsubscribe: UnsubscribePacket) -> Result<()> {
        let client_id = self.client_id.as_ref().unwrap();
        let mut reason_codes = Vec::new();
        
        for topic_filter in &unsubscribe.filters {
            let removed = self.router.unsubscribe(client_id, topic_filter).await;
            reason_codes.push(if removed {
                crate::packet::unsuback::UnsubAckReasonCode::Success
            } else {
                crate::packet::unsuback::UnsubAckReasonCode::NoSubscriptionExisted
            });
        }
        
        let mut unsuback = UnsubAckPacket::new(unsubscribe.packet_id);
        unsuback.reason_codes = reason_codes;
        self.transport.write_packet(Packet::UnsubAck(unsuback)).await
    }
    
    /// Handles PUBLISH packet
    async fn handle_publish(&mut self, publish: PublishPacket) -> Result<()> {
        let client_id = self.client_id.as_ref().unwrap();
        
        // Check authorization
        let authorized = self.auth_provider
            .authorize_publish(client_id, self.user_id.as_deref(), &publish.topic_name)
            .await?;
        
        if !authorized {
            if publish.qos != QoS::AtMostOnce {
                // Send negative acknowledgment
                match publish.qos {
                    QoS::AtLeastOnce => {
                        let mut puback = PubAckPacket::new(publish.packet_id.unwrap());
                        puback.reason_code = ReasonCode::NotAuthorized;
                        self.transport.write_packet(Packet::PubAck(puback)).await?;
                    }
                    QoS::ExactlyOnce => {
                        let mut pubrec = PubRecPacket::new(publish.packet_id.unwrap());
                        pubrec.reason_code = ReasonCode::NotAuthorized;
                        self.transport.write_packet(Packet::PubRec(pubrec)).await?;
                    }
                    QoS::AtMostOnce => {}
                }
            }
            return Ok(());
        }
        
        // Handle based on QoS
        match publish.qos {
            QoS::AtMostOnce => {
                // Route immediately
                self.router.route_message(&publish).await;
            }
            QoS::AtLeastOnce => {
                // Route and acknowledge
                self.router.route_message(&publish).await;
                let mut puback = PubAckPacket::new(publish.packet_id.unwrap());
                puback.reason_code = ReasonCode::Success;
                self.transport.write_packet(Packet::PubAck(puback)).await?;
            }
            QoS::ExactlyOnce => {
                // Store and send PUBREC
                let packet_id = publish.packet_id.unwrap();
                self.inflight_publishes.insert(packet_id, publish);
                let mut pubrec = PubRecPacket::new(packet_id);
                pubrec.reason_code = ReasonCode::Success;
                self.transport.write_packet(Packet::PubRec(pubrec)).await?;
            }
        }
        
        Ok(())
    }
    
    /// Handles PUBACK packet
    fn handle_puback(&self, _puback: PubAckPacket) {
        // For QoS 1 publishes we sent
    }
    
    /// Handles PUBREC packet
    async fn handle_pubrec(&mut self, pubrec: PubRecPacket) -> Result<()> {
        // Send PUBREL
        let mut pub_rel = PubRelPacket::new(pubrec.packet_id);
        pub_rel.reason_code = ReasonCode::Success;
        self.transport.write_packet(Packet::PubRel(pub_rel)).await
    }
    
    /// Handles PUBREL packet
    async fn handle_pubrel(&mut self, pubrel: PubRelPacket) -> Result<()> {
        // Complete QoS 2 flow
        if let Some(publish) = self.inflight_publishes.remove(&pubrel.packet_id) {
            self.router.route_message(&publish).await;
        }
        
        let mut pubcomp = PubCompPacket::new(pubrel.packet_id);
        pubcomp.reason_code = ReasonCode::Success;
        self.transport.write_packet(Packet::PubComp(pubcomp)).await
    }
    
    /// Handles PUBCOMP packet
    fn handle_pubcomp(&self, _pubcomp: PubCompPacket) {
        // For QoS 2 publishes we sent
    }
    
    /// Handles PINGREQ packet
    async fn handle_pingreq(&mut self) -> Result<()> {
        self.transport.write_packet(Packet::PingResp).await
    }
    
    /// Handles DISCONNECT packet
    fn handle_disconnect(&self, _disconnect: DisconnectPacket) -> Result<()> {
        // Client initiated disconnect
        Err(MqttError::ClientClosed)
    }
    
    /// Sends a publish to the client
    async fn send_publish(&mut self, publish: PublishPacket) -> Result<()> {
        self.transport.write_packet(Packet::Publish(publish)).await
    }
}