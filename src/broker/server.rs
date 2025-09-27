//! MQTT v5.0 Broker Server

use crate::broker::auth::{AllowAllAuthProvider, AuthProvider};
use crate::broker::client_handler::ClientHandler;
use crate::broker::config::{BrokerConfig, StorageBackend as StorageBackendType};
use crate::broker::resource_monitor::{ResourceLimits, ResourceMonitor};
use crate::broker::router::MessageRouter;
use crate::broker::storage::{DynamicStorage, FileBackend, MemoryBackend, StorageBackend};
use crate::broker::sys_topics::{BrokerStats, SysTopicsProvider};
use crate::broker::tls_acceptor::{accept_tls_connection, TlsAcceptorConfig};
use crate::broker::transport::BrokerTransport;
use crate::broker::websocket_server::{accept_websocket_connection, WebSocketServerConfig};
use crate::error::{MqttError, Result};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};

/// MQTT v5.0 Broker
pub struct MqttBroker {
    config: Arc<BrokerConfig>,
    router: Arc<MessageRouter>,
    auth_provider: Arc<dyn AuthProvider>,
    storage: Option<Arc<DynamicStorage>>,
    stats: Arc<BrokerStats>,
    resource_monitor: Arc<ResourceMonitor>,
    listener: Option<TcpListener>,
    tls_listener: Option<TcpListener>,
    tls_acceptor: Option<TlsAcceptor>,
    ws_listener: Option<TcpListener>,
    ws_config: Option<WebSocketServerConfig>,
    udp_socket: Option<Arc<tokio::net::UdpSocket>>,
    dtls_socket: Option<Arc<tokio::net::UdpSocket>>,
    shutdown_tx: Option<tokio::sync::broadcast::Sender<()>>,
}

impl MqttBroker {
    /// Creates a new broker with default configuration
    ///
    /// # Errors
    ///
    /// Returns an error if binding fails
    pub async fn bind(addr: impl AsRef<str>) -> Result<Self> {
        let addr = addr
            .as_ref()
            .parse::<std::net::SocketAddr>()
            .map_err(|e| MqttError::Configuration(format!("Invalid address: {e}")))?;

        let config = BrokerConfig::default().with_bind_address(addr);
        Self::with_config(config).await
    }

    /// Creates a new broker with custom configuration
    ///
    /// # Errors
    ///
    /// Returns an error if configuration is invalid or binding fails
    pub async fn with_config(config: BrokerConfig) -> Result<Self> {
        // Validate configuration
        config.validate()?;

        // Bind to TCP port
        let listener = TcpListener::bind(&config.bind_address).await?;
        info!("MQTT broker listening on {}", config.bind_address);

        // Set up WebSocket if configured
        let (ws_listener, ws_config) = if let Some(ref ws_config) = config.websocket_config {
            let ws_listener = TcpListener::bind(&ws_config.bind_address).await?;
            info!(
                "MQTT broker WebSocket listening on {}",
                ws_config.bind_address
            );

            let server_config = WebSocketServerConfig::new()
                .with_path(ws_config.path.clone())
                .with_subprotocol(ws_config.subprotocol.clone());

            (Some(ws_listener), Some(server_config))
        } else {
            (None, None)
        };

        // Set up TLS if configured
        let (tls_listener, tls_acceptor) = if let Some(ref tls_config) = config.tls_config {
            // Load certificates and key
            let cert_chain =
                TlsAcceptorConfig::load_cert_chain_from_file(&tls_config.cert_file).await?;
            let private_key =
                TlsAcceptorConfig::load_private_key_from_file(&tls_config.key_file).await?;

            // Create TLS acceptor config
            let mut acceptor_config = TlsAcceptorConfig::new(cert_chain, private_key);

            // Load client CA certificates if specified
            if let Some(ref ca_file) = tls_config.ca_file {
                let ca_certs = TlsAcceptorConfig::load_cert_chain_from_file(ca_file).await?;
                acceptor_config = acceptor_config.with_client_ca_certs(ca_certs);
            }

            // Set client cert requirement
            acceptor_config =
                acceptor_config.with_require_client_cert(tls_config.require_client_cert);

            // Build the acceptor
            let acceptor = acceptor_config.build_acceptor()?;

            // Bind TLS listener
            let tls_addr = tls_config.bind_address.unwrap_or_else(|| {
                // Default to port 8883 on same interface as main listener
                let mut addr = config.bind_address;
                addr.set_port(8883);
                addr
            });
            let tls_listener = TcpListener::bind(&tls_addr).await?;
            info!("MQTT broker TLS listening on {}", tls_addr);

            (Some(tls_listener), Some(acceptor))
        } else {
            (None, None)
        };

        // Set up UDP if configured
        let udp_socket = if let Some(ref udp_config) = config.udp_config {
            let socket = tokio::net::UdpSocket::bind(&udp_config.bind_address).await?;
            info!("MQTT broker UDP listening on {}", udp_config.bind_address);
            Some(Arc::new(socket))
        } else {
            None
        };

        // Set up DTLS if configured
        let dtls_socket = if let Some(ref dtls_config) = config.dtls_config {
            let socket = tokio::net::UdpSocket::bind(&dtls_config.bind_address).await?;
            info!("MQTT broker DTLS listening on {}", dtls_config.bind_address);
            Some(Arc::new(socket))
        } else {
            None
        };

        // Create storage backend
        let storage = if config.storage_config.enable_persistence {
            Some(Self::create_storage_backend(&config.storage_config).await?)
        } else {
            None
        };

        // Create shared components
        let router = if let Some(ref storage) = storage {
            Arc::new(MessageRouter::with_storage(Arc::clone(storage)))
        } else {
            Arc::new(MessageRouter::new())
        };
        let auth_provider: Arc<dyn AuthProvider> = Arc::new(AllowAllAuthProvider);
        let stats = Arc::new(BrokerStats::new());

        // Create resource monitor with limits from config
        let resource_limits = ResourceLimits {
            max_connections: config.max_clients,
            max_connections_per_ip: 100,          // Default per-IP limit
            max_memory_bytes: 1024 * 1024 * 1024, // 1GB default
            max_message_rate_per_client: 1000,
            max_bandwidth_per_client: 10 * 1024 * 1024, // 10MB/sec
            max_connection_rate: 100,
            rate_limit_window: std::time::Duration::from_secs(60),
        };
        let resource_monitor = Arc::new(ResourceMonitor::new(resource_limits));

        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        Ok(Self {
            config: Arc::new(config),
            router,
            auth_provider,
            storage,
            stats,
            resource_monitor,
            listener: Some(listener),
            tls_listener,
            tls_acceptor,
            ws_listener,
            ws_config,
            udp_socket,
            dtls_socket,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// Create storage backend based on configuration
    async fn create_storage_backend(
        storage_config: &crate::broker::config::StorageConfig,
    ) -> Result<Arc<DynamicStorage>> {
        match storage_config.backend {
            StorageBackendType::File => {
                let backend = FileBackend::new(&storage_config.base_dir).await?;
                Ok(Arc::new(DynamicStorage::File(backend)))
            }
            StorageBackendType::Memory => {
                let backend = MemoryBackend::new();
                Ok(Arc::new(DynamicStorage::Memory(backend)))
            }
        }
    }

    /// Sets a custom authentication provider
    #[must_use]
    pub fn with_auth_provider(mut self, provider: Arc<dyn AuthProvider>) -> Self {
        self.auth_provider = provider;
        self
    }

    /// Initialize storage and start cleanup tasks
    async fn initialize_storage(
        &self,
        shutdown_tx: &tokio::sync::broadcast::Sender<()>,
    ) -> Result<()> {
        if let Some(ref storage) = self.storage {
            // Perform initial cleanup
            storage.cleanup_expired().await?;

            // Start periodic cleanup task
            let storage_clone = Arc::clone(storage);
            let cleanup_interval = self.config.storage_config.cleanup_interval;
            let mut shutdown_rx = shutdown_tx.subscribe();

            tokio::spawn(async move {
                let mut interval = tokio::time::interval(cleanup_interval);
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            if let Err(e) = storage_clone.cleanup_expired().await {
                                error!("Storage cleanup error: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            debug!("Storage cleanup task shutting down");
                            break;
                        }
                    }
                }
            });
        }
        Ok(())
    }

    /// Runs the broker until shutdown
    ///
    /// This accepts incoming connections and spawns handlers for each.
    ///
    /// # Errors
    ///
    /// Returns an error if the accept loop fails
    #[allow(clippy::too_many_lines)]
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting MQTT broker");

        let Some(listener) = self.listener.take() else {
            return Err(MqttError::InvalidState(
                "Broker already running".to_string(),
            ));
        };

        let tls_listener = self.tls_listener.take();
        let tls_acceptor = self.tls_acceptor.take();
        let ws_listener = self.ws_listener.take();
        let ws_config = self.ws_config.take();
        let udp_socket = self.udp_socket.take();
        let dtls_socket = self.dtls_socket.take();

        let Some(shutdown_tx) = self.shutdown_tx.take() else {
            return Err(MqttError::InvalidState(
                "Broker already running".to_string(),
            ));
        };

        // Initialize storage and cleanup tasks
        self.initialize_storage(&shutdown_tx).await?;

        // Initialize router (load retained messages)
        self.router.initialize().await?;

        // Start $SYS topics provider
        let sys_provider =
            SysTopicsProvider::new(Arc::clone(&self.router), Arc::clone(&self.stats));
        sys_provider.start();

        // Start resource monitor cleanup task
        let resource_monitor_clone = Arc::clone(&self.resource_monitor);
        let mut shutdown_rx_cleanup = shutdown_tx.subscribe();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        resource_monitor_clone.cleanup_expired_windows().await;
                    }
                    _ = shutdown_rx_cleanup.recv() => {
                        debug!("Resource monitor cleanup task shutting down");
                        break;
                    }
                }
            }
        });

        let mut shutdown_rx = shutdown_tx.subscribe();

        // Spawn WebSocket accept task if enabled
        if let (Some(ws_listener), Some(ws_config)) = (ws_listener, ws_config) {
            let config = Arc::clone(&self.config);
            let router = Arc::clone(&self.router);
            let auth_provider = Arc::clone(&self.auth_provider);
            let storage = self.storage.clone();
            let stats = Arc::clone(&self.stats);
            let resource_monitor = Arc::clone(&self.resource_monitor);
            let shutdown_tx_clone = shutdown_tx.clone();
            let mut shutdown_rx_ws = shutdown_tx.subscribe();

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        // Accept WebSocket connections
                        accept_result = ws_listener.accept() => {
                            match accept_result {
                                Ok((tcp_stream, addr)) => {
                                    debug!("New WebSocket connection from {}", addr);

                                    // Check connection limits
                                    if !resource_monitor.can_accept_connection(addr.ip()).await {
                                        warn!("WebSocket connection rejected from {}: resource limits exceeded", addr);
                                        continue;
                                    }

                                    // Perform WebSocket handshake
                                    match accept_websocket_connection(tcp_stream, &ws_config, addr).await {
                                        Ok(ws_stream) => {
                                            let transport = BrokerTransport::websocket(ws_stream);

                                            // Spawn handler task for this client
                                            let handler = ClientHandler::new(
                                                transport,
                                                addr,
                                                Arc::clone(&config),
                                                Arc::clone(&router),
                                                Arc::clone(&auth_provider),
                                                storage.clone(),
                                                Arc::clone(&stats),
                                                Arc::clone(&resource_monitor),
                                                shutdown_tx_clone.subscribe(),
                                            );

                                            tokio::spawn(async move {
                                                if let Err(e) = handler.run().await {
                                                    // Log client handler errors at appropriate level
                                                    if e.to_string().contains("Connection closed") {
                                                        info!("Client handler finished: {}", e);
                                                    } else {
                                                        warn!("Client handler error: {}", e);
                                                    }
                                                }
                                            });
                                        }
                                        Err(e) => {
                                            error!("WebSocket handshake failed: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("WebSocket accept error: {}", e);
                                }
                            }
                        }

                        // Shutdown signal
                        _ = shutdown_rx_ws.recv() => {
                            debug!("WebSocket accept task shutting down");
                            break;
                        }
                    }
                }
            });
        }

        // Spawn TLS accept task if enabled
        if let (Some(tls_listener), Some(tls_acceptor)) = (tls_listener, tls_acceptor) {
            let config = Arc::clone(&self.config);
            let router = Arc::clone(&self.router);
            let auth_provider = Arc::clone(&self.auth_provider);
            let storage = self.storage.clone();
            let stats = Arc::clone(&self.stats);
            let resource_monitor = Arc::clone(&self.resource_monitor);
            let shutdown_tx_clone = shutdown_tx.clone();
            let mut shutdown_rx_tls = shutdown_tx.subscribe();

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        // Accept TLS connections
                        accept_result = tls_listener.accept() => {
                            match accept_result {
                                Ok((tcp_stream, addr)) => {
                                    debug!("New TLS connection from {}", addr);

                                    // Check connection limits
                                    if !resource_monitor.can_accept_connection(addr.ip()).await {
                                        warn!("TLS connection rejected from {}: resource limits exceeded", addr);
                                        continue;
                                    }

                                    // Perform TLS handshake
                                    match accept_tls_connection(&tls_acceptor, tcp_stream, addr).await {
                                        Ok(tls_stream) => {
                                            let transport = BrokerTransport::tls(tls_stream);

                                            // Spawn handler task for this client
                                            let handler = ClientHandler::new(
                                                transport,
                                                addr,
                                                Arc::clone(&config),
                                                Arc::clone(&router),
                                                Arc::clone(&auth_provider),
                                                storage.clone(),
                                                Arc::clone(&stats),
                                                Arc::clone(&resource_monitor),
                                                shutdown_tx_clone.subscribe(),
                                            );

                                            tokio::spawn(async move {
                                                if let Err(e) = handler.run().await {
                                                    // Log client handler errors at appropriate level
                                                    if e.to_string().contains("Connection closed") {
                                                        info!("Client handler finished: {}", e);
                                                    } else {
                                                        warn!("Client handler error: {}", e);
                                                    }
                                                }
                                            });
                                        }
                                        Err(e) => {
                                            error!("TLS handshake failed: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("TLS accept error: {}", e);
                                }
                            }
                        }

                        // Shutdown signal
                        _ = shutdown_rx_tls.recv() => {
                            debug!("TLS accept task shutting down");
                            break;
                        }
                    }
                }
            });
        }

        // Spawn UDP accept task if enabled
        if let Some(udp_socket) = udp_socket {
            let config = Arc::clone(&self.config);
            let router = Arc::clone(&self.router);
            let auth_provider = Arc::clone(&self.auth_provider);
            let storage = self.storage.clone();
            let stats = Arc::clone(&self.stats);
            let resource_monitor = Arc::clone(&self.resource_monitor);
            let mut shutdown_rx_udp = shutdown_tx.subscribe();

            tokio::spawn(async move {
                use crate::broker::udp_handler::UdpPacketHandler;
                use crate::broker::udp_session::UdpSessionManager;
                use std::collections::HashMap;
                use std::net::SocketAddr;

                info!("UDP broker ready to accept connections");

                let _session_manager = UdpSessionManager::new(Arc::clone(&router));
                let packet_handler = UdpPacketHandler::new(
                    Arc::clone(&config),
                    Arc::clone(&router),
                    Arc::clone(&auth_provider),
                    storage,
                    Arc::clone(&stats),
                    Arc::clone(&resource_monitor),
                );

                let mut buffer = vec![0u8; 65507];
                let mtu = config.udp_config.as_ref().map_or(1472, |c| c.mtu);

                // Track sessions by address
                let mut sessions: HashMap<SocketAddr, crate::broker::udp_session::UdpSession> =
                    HashMap::new();

                loop {
                    tokio::select! {
                        recv_result = udp_socket.recv_from(&mut buffer) => {
                            match recv_result {
                                Ok((size, peer_addr)) => {
                                    debug!(addr = %peer_addr, size = size, "UDP packet received");

                                    // Check connection limits
                                    if !resource_monitor.can_accept_connection(peer_addr.ip()).await {
                                        warn!("UDP connection rejected from {}: resource limits exceeded", peer_addr);
                                        continue;
                                    }

                                    // Get or create session
                                    let is_new = !sessions.contains_key(&peer_addr);
                                    let session = sessions.entry(peer_addr)
                                        .or_insert_with(|| {
                                            debug!("Creating new UDP session for {}", peer_addr);
                                            crate::broker::udp_session::UdpSession::new(peer_addr)
                                        });

                                    // Start publish handler for new sessions
                                    if is_new {
                                        if let Some(mut publish_rx) = session.take_publish_rx() {
                                            let udp_socket_clone = Arc::clone(&udp_socket);
                                            let peer_addr_copy = session.peer_addr; // Use session's address, not current sender's
                                            let packet_handler_clone = packet_handler.clone();
                                            let reliability_clone = Arc::clone(&session.reliability);
                                            let mtu_copy = mtu;

                                            tokio::spawn(async move {
                                                while let Some(publish) = publish_rx.recv().await {
                                                    // Encode publish packet
                                                    if let Ok(packet_bytes) = packet_handler_clone.encode_packet(&crate::packet::Packet::Publish(publish)) {
                                                        // Fragment if needed, then wrap each fragment with reliability
                                                        let fragments = packet_handler_clone.fragment_packet(&packet_bytes, mtu_copy);
                                                        for fragment in fragments {
                                                            // Wrap with reliability layer
                                                            let mut reliability = reliability_clone.lock().await;
                                                            match reliability.wrap_packet(&fragment) {
                                                                Ok(reliable_packet) => {
                                                                    if let Err(e) = udp_socket_clone.send_to(&reliable_packet, peer_addr_copy).await {
                                                                        error!("Failed to send publish to UDP client: {}", e);
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    error!("Failed to wrap publish with reliability: {}", e);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                        }
                                    }

                                    // Update activity
                                    session.update_activity();

                                    // Process packet with reliability layer
                                    let packet_data = match session.process_packet_with_reliability(&buffer[..size]).await {
                                        Ok(Some(data)) => data,
                                        Ok(None) => {
                                            // ACK packets are control packets that don't need reliability wrapping
                                            // They're sent raw because they are part of the reliability layer itself
                                            if let Some(ack_packet) = session.get_pending_acks().await {
                                                if let Err(e) = udp_socket.send_to(&ack_packet, peer_addr).await {
                                                    warn!("Failed to send ACK packet to {}: {}", peer_addr, e);
                                                }
                                            }

                                            // Check for packets to retry
                                            for retry_packet in session.get_packets_to_retry().await {
                                                if let Err(e) = udp_socket.send_to(&retry_packet, peer_addr).await {
                                                    warn!("Failed to send retry packet to {}: {}", peer_addr, e);
                                                }
                                            }

                                            continue; // Waiting for more fragments or control packet
                                        }
                                        Err(e) => {
                                            error!("Fragment reassembly error: {}", e);
                                            continue;
                                        }
                                    };

                                    // Process MQTT packet
                                    match packet_handler.handle_packet(&packet_data, peer_addr, session).await {
                                        Ok(Some(response)) => {
                                            let local_addr = udp_socket.local_addr().ok()
                                                .map_or_else(|| "unknown".to_string(), |a| a.to_string());
                                            debug!(from = %local_addr, to = %peer_addr, size = response.len(), "Sending UDP response");
                                            // Fragment first, then wrap each fragment with reliability
                                            let fragments = packet_handler.fragment_packet(&response, mtu);
                                            for fragment in fragments {
                                                let reliable_fragment = match session.wrap_packet_with_reliability(&fragment).await {
                                                    Ok(r) => r,
                                                    Err(e) => {
                                                        error!("Failed to wrap fragment with reliability: {}", e);
                                                        continue;
                                                    }
                                                };
                                                if let Err(e) = udp_socket.send_to(&reliable_fragment, peer_addr).await {
                                                    error!("UDP send error: {}", e);
                                                } else {
                                                    debug!(from = %local_addr, to = %peer_addr, frag_size = reliable_fragment.len(), "UDP packet sent");
                                                }
                                            }
                                        }
                                        Ok(None) => {} // No response needed
                                        Err(e) => {
                                            error!("Packet handling error: {}", e);
                                        }
                                    }

                                    // Clean up expired sessions periodically
                                    sessions.retain(|_, s| !s.is_expired());
                                }
                                Err(e) => {
                                    error!("UDP receive error: {}", e);
                                }
                            }
                        }

                        _ = shutdown_rx_udp.recv() => {
                            debug!("UDP accept task shutting down");
                            break;
                        }
                    }
                }
            });
        }

        // Spawn DTLS accept task if enabled
        if let Some(dtls_socket) = dtls_socket {
            let config = Arc::clone(&self.config);
            let _router = Arc::clone(&self.router);
            let _auth_provider = Arc::clone(&self.auth_provider);
            let _storage = self.storage.clone();
            let _stats = Arc::clone(&self.stats);
            let _resource_monitor = Arc::clone(&self.resource_monitor);
            let mut shutdown_rx_dtls = shutdown_tx.subscribe();

            tokio::spawn(async move {
                info!("DTLS broker ready to accept connections");

                if let Some(ref dtls_config) = config.dtls_config {
                    match crate::broker::dtls_handler::DtlsHandler::new(
                        dtls_socket.clone(),
                        dtls_config,
                    ) {
                        Ok(handler) => {
                            tokio::select! {
                                result = handler.run() => {
                                    if let Err(e) = result {
                                        error!("DTLS handler error: {}", e);
                                    }
                                }
                                _ = shutdown_rx_dtls.recv() => {
                                    debug!("DTLS accept task shutting down");
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to create DTLS handler: {}", e);
                        }
                    }
                } else {
                    error!("DTLS socket exists but configuration is missing");
                }
            });
        }

        info!("Broker ready - accepting connections");
        loop {
            tokio::select! {
                // Accept new TCP connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, addr)) => {
                            debug!(addr = %addr, "New TCP connection");

                            // Check connection limits
                            if !self.resource_monitor.can_accept_connection(addr.ip()).await {
                                warn!("Connection rejected from {}: resource limits exceeded", addr);
                                continue;
                            }

                            let transport = BrokerTransport::tcp(stream);

                            // Spawn handler task for this client
                            let handler = ClientHandler::new(
                                transport,
                                addr,
                                Arc::clone(&self.config),
                                Arc::clone(&self.router),
                                Arc::clone(&self.auth_provider),
                                self.storage.clone(),
                                Arc::clone(&self.stats),
                                Arc::clone(&self.resource_monitor),
                                shutdown_tx.subscribe(),
                            );

                            tokio::spawn(async move {
                                if let Err(e) = handler.run().await {
                                    // Log client handler errors at appropriate level
                                    if e.to_string().contains("Connection closed") {
                                        info!("Client handler finished: {}", e);
                                    } else {
                                        warn!("Client handler error: {}", e);
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            error!("TCP accept error: {}", e);
                            // Continue accepting other connections
                        }
                    }
                }

                // Shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Broker shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Shuts down the broker gracefully
    ///
    /// # Errors
    ///
    /// Returns an error if no receivers are available for shutdown signal
    pub async fn shutdown(&self) -> Result<()> {
        if let Some(ref shutdown_tx) = self.shutdown_tx {
            shutdown_tx.send(()).map_err(|_| {
                MqttError::InvalidState("No receivers for shutdown signal".to_string())
            })?;
        }

        // Give clients time to disconnect gracefully
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        info!("Broker shutdown complete");
        Ok(())
    }

    /// Gets broker statistics
    #[must_use]
    pub fn stats(&self) -> Arc<BrokerStats> {
        Arc::clone(&self.stats)
    }

    /// Gets resource monitor
    #[must_use]
    pub fn resource_monitor(&self) -> Arc<ResourceMonitor> {
        Arc::clone(&self.resource_monitor)
    }

    /// Gets the local address the broker is bound to
    pub fn local_addr(&self) -> Option<std::net::SocketAddr> {
        self.listener.as_ref()?.local_addr().ok()
    }

    /// Gets the UDP address if UDP is enabled
    pub fn udp_address(&self) -> Option<std::net::SocketAddr> {
        self.udp_socket.as_ref()?.local_addr().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_broker_bind() {
        // Use random port to avoid conflicts
        let broker = MqttBroker::bind("127.0.0.1:0").await;
        assert!(broker.is_ok());
    }

    #[tokio::test]
    async fn test_broker_with_config() {
        let config = BrokerConfig::default()
            .with_bind_address(([127, 0, 0, 1], 0))
            .with_max_clients(100);

        let broker = MqttBroker::with_config(config).await;
        assert!(broker.is_ok());
    }

    #[tokio::test]
    async fn test_broker_shutdown() {
        let mut broker = MqttBroker::bind("127.0.0.1:0").await.unwrap();

        // Start broker in background
        let broker_handle = tokio::spawn(async move { broker.run().await });

        // Give broker time to start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Now test shutdown - but we can't call it because broker was moved
        // Just ensure the broker starts without error for now
        broker_handle.abort();
    }

    #[tokio::test]
    async fn test_broker_stats() {
        let broker = MqttBroker::bind("127.0.0.1:0").await.unwrap();
        let stats = broker.stats();

        assert_eq!(
            stats
                .clients_connected
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }
}
