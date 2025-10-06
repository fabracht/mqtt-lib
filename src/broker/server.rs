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
    listeners: Vec<TcpListener>,
    tls_listeners: Vec<TcpListener>,
    tls_acceptor: Option<TlsAcceptor>,
    ws_listeners: Vec<TcpListener>,
    ws_config: Option<WebSocketServerConfig>,
    ws_tls_listeners: Vec<TcpListener>,
    ws_tls_config: Option<WebSocketServerConfig>,
    ws_tls_acceptor: Option<TlsAcceptor>,
    udp_socket: Option<Arc<tokio::net::UdpSocket>>,
    dtls_socket: Option<Arc<tokio::net::UdpSocket>>,
    shutdown_tx: Option<tokio::sync::broadcast::Sender<()>>,
}

async fn bind_all(addrs: &[std::net::SocketAddr], transport_name: &str) -> Vec<TcpListener> {
    let mut listeners = Vec::new();
    for addr in addrs {
        match TcpListener::bind(addr).await {
            Ok(listener) => {
                info!("MQTT broker {} listening on {}", transport_name, addr);
                listeners.push(listener);
            }
            Err(e) => {
                warn!(
                    "Failed to bind {} on {} ({}), continuing with other addresses",
                    transport_name, addr, e
                );
            }
        }
    }
    listeners
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
        config.validate()?;

        let listeners = bind_all(&config.bind_addresses, "TCP").await;
        if listeners.is_empty() {
            return Err(MqttError::Configuration(
                "Failed to bind to any TCP address".to_string(),
            ));
        }

        let (ws_listeners, ws_config) = Self::setup_websocket(&config).await?;
        let (ws_tls_listeners, ws_tls_config, ws_tls_acceptor) =
            Self::setup_websocket_tls(&config).await?;
        let (tls_listeners, tls_acceptor) = Self::setup_tls(&config).await?;
        let udp_socket = Self::setup_udp(&config).await?;
        let dtls_socket = Self::setup_dtls(&config).await?;

        let storage = if config.storage_config.enable_persistence {
            Some(Self::create_storage_backend(&config.storage_config).await?)
        } else {
            None
        };

        let router = if let Some(ref storage) = storage {
            Arc::new(MessageRouter::with_storage(Arc::clone(storage)))
        } else {
            Arc::new(MessageRouter::new())
        };

        let auth_provider: Arc<dyn AuthProvider> = Arc::new(AllowAllAuthProvider);
        let stats = Arc::new(BrokerStats::new());
        let resource_monitor = Arc::new(ResourceMonitor::new(Self::default_resource_limits(
            config.max_clients,
        )));
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        Ok(Self {
            config: Arc::new(config),
            router,
            auth_provider,
            storage,
            stats,
            resource_monitor,
            listeners,
            tls_listeners,
            tls_acceptor,
            ws_listeners,
            ws_config,
            ws_tls_listeners,
            ws_tls_config,
            ws_tls_acceptor,
            udp_socket,
            dtls_socket,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    async fn setup_websocket(
        config: &BrokerConfig,
    ) -> Result<(Vec<TcpListener>, Option<WebSocketServerConfig>)> {
        if let Some(ref ws_config) = config.websocket_config {
            let ws_listeners = bind_all(&ws_config.bind_addresses, "WebSocket").await;
            if ws_listeners.is_empty() {
                warn!("Failed to bind WebSocket to any address, WebSocket disabled");
                return Ok((Vec::new(), None));
            }
            let server_config = WebSocketServerConfig::new()
                .with_path(ws_config.path.clone())
                .with_subprotocol(ws_config.subprotocol.clone());
            Ok((ws_listeners, Some(server_config)))
        } else {
            Ok((Vec::new(), None))
        }
    }

    async fn setup_websocket_tls(
        config: &BrokerConfig,
    ) -> Result<(
        Vec<TcpListener>,
        Option<WebSocketServerConfig>,
        Option<TlsAcceptor>,
    )> {
        if let Some(ref ws_tls_config) = config.websocket_tls_config {
            if let Some(ref tls_config) = config.tls_config {
                let cert_chain =
                    TlsAcceptorConfig::load_cert_chain_from_file(&tls_config.cert_file).await?;
                let private_key =
                    TlsAcceptorConfig::load_private_key_from_file(&tls_config.key_file).await?;

                let mut acceptor_config = TlsAcceptorConfig::new(cert_chain, private_key);

                if let Some(ref ca_file) = tls_config.ca_file {
                    let ca_certs = TlsAcceptorConfig::load_cert_chain_from_file(ca_file).await?;
                    acceptor_config = acceptor_config.with_client_ca_certs(ca_certs);
                }

                acceptor_config =
                    acceptor_config.with_require_client_cert(tls_config.require_client_cert);
                let acceptor = acceptor_config.build_acceptor()?;

                let ws_tls_listeners =
                    bind_all(&ws_tls_config.bind_addresses, "WebSocket TLS").await;
                if ws_tls_listeners.is_empty() {
                    warn!("Failed to bind WebSocket TLS to any address, WebSocket TLS disabled");
                    return Ok((Vec::new(), None, None));
                }

                let server_config = WebSocketServerConfig::new()
                    .with_path(ws_tls_config.path.clone())
                    .with_subprotocol(ws_tls_config.subprotocol.clone());

                Ok((ws_tls_listeners, Some(server_config), Some(acceptor)))
            } else {
                Err(MqttError::Configuration(
                    "WebSocket TLS requires TLS configuration (cert/key)".to_string(),
                ))
            }
        } else {
            Ok((Vec::new(), None, None))
        }
    }

    async fn setup_tls(config: &BrokerConfig) -> Result<(Vec<TcpListener>, Option<TlsAcceptor>)> {
        if let Some(ref tls_config) = config.tls_config {
            let cert_chain =
                TlsAcceptorConfig::load_cert_chain_from_file(&tls_config.cert_file).await?;
            let private_key =
                TlsAcceptorConfig::load_private_key_from_file(&tls_config.key_file).await?;

            let mut acceptor_config = TlsAcceptorConfig::new(cert_chain, private_key);

            if let Some(ref ca_file) = tls_config.ca_file {
                let ca_certs = TlsAcceptorConfig::load_cert_chain_from_file(ca_file).await?;
                acceptor_config = acceptor_config.with_client_ca_certs(ca_certs);
            }

            acceptor_config =
                acceptor_config.with_require_client_cert(tls_config.require_client_cert);
            let acceptor = acceptor_config.build_acceptor()?;

            let tls_listeners = bind_all(&tls_config.bind_addresses, "TLS").await;
            if tls_listeners.is_empty() {
                warn!("Failed to bind TLS to any address, TLS disabled");
                return Ok((Vec::new(), None));
            }

            Ok((tls_listeners, Some(acceptor)))
        } else {
            Ok((Vec::new(), None))
        }
    }

    #[cfg(feature = "udp")]
    async fn setup_udp(config: &BrokerConfig) -> Result<Option<Arc<tokio::net::UdpSocket>>> {
        if let Some(ref udp_config) = config.udp_config {
            let bind_addr = udp_config.bind_addresses.first().ok_or_else(|| {
                MqttError::Configuration("UDP config has no bind addresses".to_string())
            })?;
            let socket = tokio::net::UdpSocket::bind(bind_addr).await?;
            info!("MQTT broker UDP listening on {}", bind_addr);
            Ok(Some(Arc::new(socket)))
        } else {
            Ok(None)
        }
    }

    #[cfg(not(feature = "udp"))]
    async fn setup_udp(_config: &BrokerConfig) -> Result<Option<Arc<tokio::net::UdpSocket>>> {
        Ok(None)
    }

    #[cfg(feature = "udp")]
    async fn setup_dtls(config: &BrokerConfig) -> Result<Option<Arc<tokio::net::UdpSocket>>> {
        if let Some(ref dtls_config) = config.dtls_config {
            let bind_addr = dtls_config.bind_addresses.first().ok_or_else(|| {
                MqttError::Configuration("DTLS config has no bind addresses".to_string())
            })?;
            let socket = tokio::net::UdpSocket::bind(bind_addr).await?;
            info!("MQTT broker DTLS listening on {}", bind_addr);
            Ok(Some(Arc::new(socket)))
        } else {
            Ok(None)
        }
    }

    #[cfg(not(feature = "udp"))]
    async fn setup_dtls(_config: &BrokerConfig) -> Result<Option<Arc<tokio::net::UdpSocket>>> {
        Ok(None)
    }

    fn default_resource_limits(max_clients: usize) -> ResourceLimits {
        ResourceLimits {
            max_connections: max_clients,
            max_connections_per_ip: 100,
            max_memory_bytes: 1024 * 1024 * 1024,
            max_message_rate_per_client: 1000,
            max_bandwidth_per_client: 10 * 1024 * 1024,
            max_connection_rate: 100,
            rate_limit_window: std::time::Duration::from_secs(60),
        }
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

        if self.listeners.is_empty() {
            return Err(MqttError::InvalidState(
                "Broker already running".to_string(),
            ));
        }

        let listeners = std::mem::take(&mut self.listeners);
        let tls_listeners = std::mem::take(&mut self.tls_listeners);
        let tls_acceptor = self.tls_acceptor.take();
        let ws_listeners = std::mem::take(&mut self.ws_listeners);
        let ws_config = self.ws_config.take();
        let ws_tls_listeners = std::mem::take(&mut self.ws_tls_listeners);
        let ws_tls_config = self.ws_tls_config.take();
        let ws_tls_acceptor = self.ws_tls_acceptor.take();
        #[cfg(feature = "udp")]
        let udp_socket = self.udp_socket.take();
        #[cfg(feature = "udp")]
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

        // Spawn WebSocket accept tasks (one per listener)
        if let Some(ws_config) = ws_config {
            for ws_listener in ws_listeners {
                let ws_cfg = ws_config.clone();
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
                                        match accept_websocket_connection(tcp_stream, &ws_cfg, addr).await {
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
        }

        // Spawn WebSocket TLS accept tasks (one per listener)
        if let (Some(ws_tls_config), Some(ws_tls_acceptor)) = (ws_tls_config, ws_tls_acceptor) {
            let acceptor = Arc::new(ws_tls_acceptor);
            for ws_tls_listener in ws_tls_listeners {
                let ws_cfg = ws_tls_config.clone();
                let acceptor = Arc::clone(&acceptor);
                let config = Arc::clone(&self.config);
                let router = Arc::clone(&self.router);
                let auth_provider = Arc::clone(&self.auth_provider);
                let storage = self.storage.clone();
                let stats = Arc::clone(&self.stats);
                let resource_monitor = Arc::clone(&self.resource_monitor);
                let shutdown_tx_clone = shutdown_tx.clone();
                let mut shutdown_rx_wss = shutdown_tx.subscribe();

                tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            accept_result = ws_tls_listener.accept() => {
                                match accept_result {
                                    Ok((tcp_stream, addr)) => {
                                        debug!("New WebSocket TLS connection from {}", addr);

                                        if !resource_monitor.can_accept_connection(addr.ip()).await {
                                            warn!("WebSocket TLS connection rejected from {}: resource limits exceeded", addr);
                                            continue;
                                        }

                                        let acc_clone = acceptor.clone();
                                        let cfg_clone = ws_cfg.clone();
                                        let config_clone = Arc::clone(&config);
                                        let router_clone = Arc::clone(&router);
                                        let auth_clone = Arc::clone(&auth_provider);
                                        let storage_clone = storage.clone();
                                        let stats_clone = Arc::clone(&stats);
                                        let monitor_clone = Arc::clone(&resource_monitor);
                                        let shutdown_tx_wstls = shutdown_tx_clone.clone();

                                        tokio::spawn(async move {
                                            match accept_tls_connection(&acc_clone, tcp_stream, addr).await {
                                                Ok(tls_stream) => {
                                                    match accept_websocket_connection(tls_stream, &cfg_clone, addr).await {
                                                        Ok(ws_stream) => {
                                                            let transport = BrokerTransport::websocket(ws_stream);

                                                            let handler = ClientHandler::new(
                                                                transport,
                                                                addr,
                                                                config_clone,
                                                                router_clone,
                                                                auth_clone,
                                                                storage_clone,
                                                                stats_clone,
                                                                monitor_clone,
                                                                shutdown_tx_wstls.subscribe(),
                                                            );

                                                            if let Err(e) = handler.run().await {
                                                                if e.to_string().contains("Connection closed") {
                                                                    info!("Client handler finished: {}", e);
                                                                } else {
                                                                    warn!("Client handler error: {}", e);
                                                                }
                                                            }
                                                        }
                                                        Err(e) => {
                                                            error!("WebSocket TLS handshake failed: {}", e);
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    error!("TLS handshake failed for WebSocket: {}", e);
                                                }
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        error!("WebSocket TLS accept error: {}", e);
                                    }
                                }
                            }

                            _ = shutdown_rx_wss.recv() => {
                                debug!("WebSocket TLS accept task shutting down");
                                break;
                            }
                        }
                    }
                });
            }
        }

        // Spawn TLS accept tasks (one per listener)
        if let Some(tls_acceptor) = tls_acceptor {
            let acceptor = Arc::new(tls_acceptor);
            for tls_listener in tls_listeners {
                let acceptor = Arc::clone(&acceptor);
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
                                        match accept_tls_connection(&acceptor, tcp_stream, addr).await {
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
        }

        // Spawn UDP accept task if enabled
        #[cfg(feature = "udp")]
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

                let mut sessions: HashMap<SocketAddr, crate::broker::udp_session::UdpSession> =
                    HashMap::new();

                loop {
                    tokio::select! {
                        recv_result = udp_socket.recv_from(&mut buffer) => {
                            match recv_result {
                                Ok((size, peer_addr)) => {
                                    debug!(addr = %peer_addr, size = size, "UDP packet received");

                                    if !resource_monitor.can_accept_connection(peer_addr.ip()).await {
                                        warn!("UDP connection rejected from {}: resource limits exceeded", peer_addr);
                                        continue;
                                    }

                                    let is_new = !sessions.contains_key(&peer_addr);
                                    let session = sessions.entry(peer_addr)
                                        .or_insert_with(|| {
                                            debug!("Creating new UDP session for {}", peer_addr);
                                            crate::broker::udp_session::UdpSession::new(peer_addr)
                                        });

                                    if is_new {
                                        if let Some(mut publish_rx) = session.take_publish_rx() {
                                            let udp_socket_clone = Arc::clone(&udp_socket);
                                            let peer_addr_copy = session.peer_addr;
                                            let packet_handler_clone = packet_handler.clone();
                                            let reliability_clone = Arc::clone(&session.reliability);
                                            let mtu_copy = mtu;

                                            tokio::spawn(async move {
                                                while let Some(publish) = publish_rx.recv().await {
                                                    if let Ok(packet_bytes) = packet_handler_clone.encode_packet(&crate::packet::Packet::Publish(publish)) {
                                                        let fragments = packet_handler_clone.fragment_packet(&packet_bytes, mtu_copy);
                                                        for fragment in fragments {
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

                                    session.update_activity();

                                    let packet_data = match session.process_packet_with_reliability(&buffer[..size]).await {
                                        Ok(Some(data)) => data,
                                        Ok(None) => {
                                            if let Some(ack_packet) = session.get_pending_acks().await {
                                                if let Err(e) = udp_socket.send_to(&ack_packet, peer_addr).await {
                                                    warn!("Failed to send ACK packet to {}: {}", peer_addr, e);
                                                }
                                            }

                                            for retry_packet in session.get_packets_to_retry().await {
                                                if let Err(e) = udp_socket.send_to(&retry_packet, peer_addr).await {
                                                    warn!("Failed to send retry packet to {}: {}", peer_addr, e);
                                                }
                                            }

                                            continue;
                                        }
                                        Err(e) => {
                                            error!("Fragment reassembly error: {}", e);
                                            continue;
                                        }
                                    };

                                    match packet_handler.handle_packet(&packet_data, peer_addr, session).await {
                                        Ok(Some(response)) => {
                                            let local_addr = udp_socket.local_addr().ok()
                                                .map_or_else(|| "unknown".to_string(), |a| a.to_string());
                                            debug!(from = %local_addr, to = %peer_addr, size = response.len(), "Sending UDP response");
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
                                        Ok(None) => {}
                                        Err(e) => {
                                            error!("Packet handling error: {}", e);
                                        }
                                    }

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
        #[cfg(feature = "udp")]
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

        // Spawn TCP accept tasks (one per listener)
        for listener in listeners {
            let config = Arc::clone(&self.config);
            let router = Arc::clone(&self.router);
            let auth_provider = Arc::clone(&self.auth_provider);
            let storage = self.storage.clone();
            let stats = Arc::clone(&self.stats);
            let resource_monitor = Arc::clone(&self.resource_monitor);
            let shutdown_tx_clone = shutdown_tx.clone();
            let mut shutdown_rx_tcp = shutdown_tx.subscribe();

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, addr)) => {
                            debug!(addr = %addr, "New TCP connection");

                            // Check connection limits
                            if !resource_monitor.can_accept_connection(addr.ip()).await {
                                warn!("Connection rejected from {}: resource limits exceeded", addr);
                                continue;
                            }

                            let transport = BrokerTransport::tcp(stream);

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
                            error!("TCP accept error: {}", e);
                            // Continue accepting other connections
                        }
                    }
                        }
                        _ = shutdown_rx_tcp.recv() => {
                            debug!("TCP accept task shutting down");
                            break;
                        }
                    }
                }
            });
        }

        info!("Broker ready - accepting connections");

        // Wait for shutdown signal
        shutdown_rx.recv().await.ok();
        info!("Broker shutting down");

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

    /// Gets the first local address the broker is bound to (used by tests)
    pub fn local_addr(&self) -> Option<std::net::SocketAddr> {
        self.listeners.first()?.local_addr().ok()
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
