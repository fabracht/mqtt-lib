use anyhow::{Context, Result};
use clap::Args;
use dialoguer::Confirm;
use mqtt5::broker::{BrokerConfig, MqttBroker};
use std::path::{Path, PathBuf};
use tokio::signal;
use tracing::{debug, info};

#[derive(Args)]
pub struct BrokerCommand {
    /// Configuration file path
    #[arg(long, short)]
    pub config: Option<PathBuf>,

    /// Bind address for the broker
    #[arg(long, short = 'H', default_value = "0.0.0.0:1883")]
    pub host: String,

    /// Maximum number of concurrent clients
    #[arg(long, default_value = "10000")]
    pub max_clients: usize,

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
    println!("  📡 Listening on: {}", cmd.host);
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
    let mut config = BrokerConfig::new();

    // Parse bind address
    let bind_addr: std::net::SocketAddr = cmd
        .host
        .parse()
        .with_context(|| format!("Invalid bind address: {}", cmd.host))?;
    config = config.with_bind_address(bind_addr);

    // Set max clients
    config = config.with_max_clients(cmd.max_clients);

    if !cmd.non_interactive {
        // Ask about authentication
        let enable_auth = Confirm::new()
            .with_prompt("Enable authentication?")
            .default(false)
            .interact()
            .context("Failed to get authentication preference")?;

        if enable_auth {
            println!("Authentication setup not implemented in this demo");
            println!("For now, broker will run with anonymous access allowed");
        }

        // Ask about TLS
        let enable_tls = Confirm::new()
            .with_prompt("Enable TLS?")
            .default(false)
            .interact()
            .context("Failed to get TLS preference")?;

        if enable_tls {
            println!("TLS setup not implemented in this demo");
            println!("For now, broker will run without TLS");
        }
    }

    Ok(config)
}

async fn load_config_from_file(_config_path: &Path) -> Result<BrokerConfig> {
    // For now, just return default config
    // TODO: Implement actual config file loading
    println!("Config file loading not implemented yet, using defaults");
    Ok(BrokerConfig::default())
}
