use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "mqttv5")]
#[command(about = "Superior MQTT v5.0 CLI - unified client and broker tool")]
#[command(version)]
#[command(
    long_about = "A unified CLI tool that replaces mosquitto_pub, mosquitto_sub, and mosquitto with superior input ergonomics and smart defaults."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(long, short, global = true)]
    verbose: bool,

    /// Enable debug logging
    #[arg(long, global = true)]
    debug: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Publish messages to MQTT topics (replaces mosquitto_pub)
    Pub(commands::pub_cmd::PubCommand),
    /// Subscribe to MQTT topics (replaces mosquitto_sub)
    Sub(commands::sub_cmd::SubCommand),
    /// Start MQTT broker (replaces mosquitto daemon)
    Broker(commands::broker_cmd::BrokerCommand),
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize rustls crypto provider for TLS support
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cli = Cli::parse();

    // Initialize logging based on verbosity
    let log_level = if cli.debug {
        tracing::Level::DEBUG
    } else if cli.verbose {
        tracing::Level::INFO
    } else {
        tracing::Level::ERROR
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(false)
        .without_time()
        .init();

    match cli.command {
        Commands::Pub(cmd) => commands::pub_cmd::execute(cmd).await,
        Commands::Sub(cmd) => commands::sub_cmd::execute(cmd).await,
        Commands::Broker(cmd) => commands::broker_cmd::execute(cmd).await,
    }
}
