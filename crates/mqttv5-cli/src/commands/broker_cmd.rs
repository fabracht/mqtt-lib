use anyhow::{Context, Result};
use clap::Args;
use mqtt5::broker::{BrokerConfig, MqttBroker};
use std::path::{Path, PathBuf};
use tokio::signal;
use tracing::{debug, info};

#[derive(Args)]
pub struct BrokerCommand {
    /// Configuration file path (JSON format)
    #[arg(long, short)]
    pub config: Option<PathBuf>,

    /// TCP bind address (e.g., 0.0.0.0:1883)
    #[arg(long, short = 'H', default_value = "0.0.0.0:1883")]
    pub host: String,

    /// Maximum number of concurrent clients
    #[arg(long, default_value = "10000")]
    pub max_clients: usize,

    /// Enable anonymous access (no authentication required)
    #[arg(long, default_value = "true")]
    pub allow_anonymous: bool,

    /// Password file path (format: username:password per line)
    #[arg(long)]
    pub auth_password_file: Option<PathBuf>,

    /// TLS certificate file path (PEM format)
    #[arg(long)]
    pub tls_cert: Option<PathBuf>,

    /// TLS private key file path (PEM format)
    #[arg(long)]
    pub tls_key: Option<PathBuf>,

    /// TLS bind address (e.g., 0.0.0.0:8883)
    #[arg(long, default_value = "0.0.0.0:8883")]
    pub tls_host: String,

    /// WebSocket bind address (e.g., 0.0.0.0:8080)
    #[arg(long)]
    pub ws_host: Option<String>,

    /// WebSocket path (e.g., /mqtt)
    #[arg(long, default_value = "/mqtt")]
    pub ws_path: String,

    /// UDP bind address (e.g., 0.0.0.0:1884)
    #[arg(long)]
    pub udp_host: Option<String>,

    /// DTLS bind address (e.g., 0.0.0.0:8884)
    #[arg(long)]
    pub dtls_host: Option<String>,

    /// DTLS certificate file path (PEM format)
    #[arg(long)]
    pub dtls_cert: Option<PathBuf>,

    /// DTLS private key file path (PEM format)
    #[arg(long)]
    pub dtls_key: Option<PathBuf>,

    /// Storage directory for persistent data
    #[arg(long, default_value = "./mqtt_storage")]
    pub storage_dir: PathBuf,

    /// Disable message persistence
    #[arg(long)]
    pub no_persistence: bool,

    /// Session expiry interval in seconds (default: 3600)
    #[arg(long, default_value = "3600")]
    pub session_expiry: u64,

    /// Maximum QoS level supported (0, 1, or 2)
    #[arg(long, default_value = "2")]
    pub max_qos: u8,

    /// Server keep-alive time in seconds (optional)
    #[arg(long)]
    pub keep_alive: Option<u16>,

    /// Disable retained messages
    #[arg(long)]
    pub no_retain: bool,

    /// Disable wildcard subscriptions
    #[arg(long)]
    pub no_wildcards: bool,

    /// Skip prompts and use defaults
    #[arg(long)]
    pub non_interactive: bool,

    /// Run in foreground (don't daemonize)
    #[arg(long, short)]
    pub foreground: bool,
}

pub async fn execute(mut cmd: BrokerCommand) -> Result<()> {
    info!("Starting MQTT v5.0 broker...");

    // Create broker configuration
    let config = if let Some(config_path) = &cmd.config {
        debug!("Loading configuration from: {:?}", config_path);
        load_config_from_file(config_path)
            .await
            .with_context(|| format!("Failed to load config from {config_path:?}"))?
    } else {
        // Smart prompting for configuration
        create_interactive_config(&mut cmd).await?
    };

    // Validate configuration
    config
        .validate()
        .context("Configuration validation failed")?;

    // Create and start broker
    info!("Creating broker with bind address: {}", config.bind_address);
    let mut broker = MqttBroker::with_config(config)
        .await
        .context("Failed to create MQTT broker")?;

    println!("🚀 MQTT v5.0 broker starting...");
    println!("  📡 TCP: {}", cmd.host);
    if cmd.tls_cert.is_some() {
        println!("  🔒 TLS: {}", cmd.tls_host);
    }
    if let Some(ref ws_host) = cmd.ws_host {
        println!("  🌐 WebSocket: {} (path: {})", ws_host, cmd.ws_path);
    }
    if let Some(ref udp_host) = cmd.udp_host {
        println!("  📦 UDP: {}", udp_host);
    }
    if let Some(ref dtls_host) = cmd.dtls_host {
        println!("  🔐 DTLS: {}", dtls_host);
    }
    println!("  👥 Max clients: {}", cmd.max_clients);
    println!("  📝 Press Ctrl+C to stop");

    // Set up signal handling
    let shutdown_signal = async {
        match signal::ctrl_c().await {
            Ok(()) => {
                println!("\n🛑 Received Ctrl+C, shutting down gracefully...");
            }
            Err(err) => {
                tracing::error!("Unable to listen for shutdown signal: {}", err);
            }
        }
    };

    // Run broker with graceful shutdown
    tokio::select! {
        result = broker.run() => {
            match result {
                Ok(()) => {
                    info!("Broker stopped normally");
                }
                Err(e) => {
                    anyhow::bail!("Broker error: {}", e);
                }
            }
        }
        _ = shutdown_signal => {
            info!("Shutdown signal received, stopping broker...");
        }
    }

    println!("✓ MQTT broker stopped");
    Ok(())
}

async fn create_interactive_config(cmd: &mut BrokerCommand) -> Result<BrokerConfig> {
    use mqtt5::broker::config::{
        AuthConfig, AuthMethod, StorageConfig, TlsConfig, UdpConfig, WebSocketConfig,
    };

    let mut config = BrokerConfig::new();

    // Parse bind address
    let bind_addr: std::net::SocketAddr = cmd
        .host
        .parse()
        .with_context(|| format!("Invalid bind address: {}", cmd.host))?;
    config = config.with_bind_address(bind_addr);

    // Set basic broker parameters
    config = config.with_max_clients(cmd.max_clients);
    config.session_expiry_interval = std::time::Duration::from_secs(cmd.session_expiry);
    config.maximum_qos = cmd.max_qos;
    config.retain_available = !cmd.no_retain;
    config.wildcard_subscription_available = !cmd.no_wildcards;

    if let Some(keep_alive) = cmd.keep_alive {
        config.server_keep_alive = Some(std::time::Duration::from_secs(keep_alive as u64));
    }

    // Configure authentication
    if let Some(password_file) = &cmd.auth_password_file {
        let auth_config = AuthConfig {
            allow_anonymous: cmd.allow_anonymous,
            password_file: Some(password_file.clone()),
            auth_method: AuthMethod::Password,
            auth_data: None,
        };
        config = config.with_auth(auth_config);
        info!(
            "Authentication enabled with password file: {:?}",
            password_file
        );
    } else if !cmd.allow_anonymous {
        anyhow::bail!("--auth-password-file is required when anonymous access is disabled");
    }

    // Configure TLS
    if let (Some(cert), Some(key)) = (&cmd.tls_cert, &cmd.tls_key) {
        let tls_addr: std::net::SocketAddr = cmd
            .tls_host
            .parse()
            .with_context(|| format!("Invalid TLS bind address: {}", cmd.tls_host))?;

        let tls_config = TlsConfig::new(cert.clone(), key.clone()).with_bind_address(tls_addr);
        config = config.with_tls(tls_config);
        info!("TLS enabled on {}", cmd.tls_host);
    } else if cmd.tls_cert.is_some() || cmd.tls_key.is_some() {
        anyhow::bail!("Both --tls-cert and --tls-key must be provided together");
    }

    // Configure WebSocket
    if let Some(ws_host) = &cmd.ws_host {
        let ws_addr: std::net::SocketAddr = ws_host
            .parse()
            .with_context(|| format!("Invalid WebSocket bind address: {}", ws_host))?;

        let ws_config = WebSocketConfig::default()
            .with_bind_address(ws_addr)
            .with_path(cmd.ws_path.clone());
        config = config.with_websocket(ws_config);
        info!("WebSocket enabled on {} path {}", ws_host, cmd.ws_path);
    }

    // Configure UDP
    if let Some(udp_host) = &cmd.udp_host {
        let udp_addr: std::net::SocketAddr = udp_host
            .parse()
            .with_context(|| format!("Invalid UDP bind address: {}", udp_host))?;

        let mut udp_config = UdpConfig::new();
        udp_config.bind_address = udp_addr;
        config = config.with_udp(udp_config);
        info!("UDP enabled on {}", udp_host);
    }

    // Configure DTLS
    if let Some(dtls_host) = &cmd.dtls_host {
        if let (Some(cert), Some(key)) = (&cmd.dtls_cert, &cmd.dtls_key) {
            let dtls_addr: std::net::SocketAddr = dtls_host
                .parse()
                .with_context(|| format!("Invalid DTLS bind address: {}", dtls_host))?;

            let dtls_config = mqtt5::broker::config::DtlsConfig {
                bind_address: dtls_addr,
                mtu: 1500,
                psk_identity: None,
                psk_key: None,
                cert_file: Some(cert.clone()),
                key_file: Some(key.clone()),
                ca_file: None, // Optional CA certificate for client verification
            };
            config = config.with_dtls(dtls_config);
            info!("DTLS enabled on {}", dtls_host);
        } else {
            anyhow::bail!(
                "Both --dtls-cert and --dtls-key must be provided when using --dtls-host"
            );
        }
    } else if cmd.dtls_cert.is_some() || cmd.dtls_key.is_some() {
        anyhow::bail!("--dtls-host must be provided when using --dtls-cert or --dtls-key");
    }

    // Configure storage
    let storage_config = StorageConfig {
        enable_persistence: !cmd.no_persistence,
        base_dir: cmd.storage_dir.clone(),
        backend: mqtt5::broker::config::StorageBackend::File,
        cleanup_interval: std::time::Duration::from_secs(300), // 5 minutes
    };
    config.storage_config = storage_config;

    Ok(config)
}

async fn load_config_from_file(config_path: &Path) -> Result<BrokerConfig> {
    let contents = std::fs::read_to_string(config_path).context("Failed to read config file")?;

    serde_json::from_str(&contents).context("Failed to parse config file as JSON")
}
