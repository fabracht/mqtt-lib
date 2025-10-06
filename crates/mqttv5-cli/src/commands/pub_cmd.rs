use anyhow::{Context, Result};
use clap::Args;
use dialoguer::{Input, Select};
use mqtt5::{ConnectOptions, MqttClient, PublishOptions, QoS, WillMessage};
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, info};

#[derive(Args)]
pub struct PubCommand {
    /// MQTT topic to publish to
    #[arg(long, short)]
    pub topic: Option<String>,

    /// Message to publish
    #[arg(long, short)]
    pub message: Option<String>,

    /// Read message from file
    #[arg(long, short)]
    pub file: Option<String>,

    /// Read message from stdin
    #[arg(long)]
    pub stdin: bool,

    /// MQTT broker URL (e.g., mqtt://localhost:1883, mqtt-udp://host:1883, mqtts-dtls://host:8883)
    #[arg(long, short = 'U', conflicts_with_all = &["host", "port"])]
    pub url: Option<String>,

    /// MQTT broker host
    #[arg(long, short = 'H', default_value = "localhost")]
    pub host: String,

    /// MQTT broker port
    #[arg(long, short, default_value = "1883")]
    pub port: u16,

    /// Quality of Service level (0, 1, or 2)
    #[arg(long, short, value_parser = parse_qos)]
    pub qos: Option<QoS>,

    /// Retain message
    #[arg(long, short)]
    pub retain: bool,

    /// Username for authentication
    #[arg(long, short)]
    pub username: Option<String>,

    /// Password for authentication
    #[arg(long, short = 'P')]
    pub password: Option<String>,

    /// Client ID
    #[arg(long, short)]
    pub client_id: Option<String>,

    /// Skip prompts and use defaults/fail if required args missing
    #[arg(long)]
    pub non_interactive: bool,

    /// Don't clean start (resume existing session)
    #[arg(long = "no-clean-start")]
    pub no_clean_start: bool,

    /// Session expiry interval in seconds (0 = expire on disconnect)
    #[arg(long)]
    pub session_expiry: Option<u32>,

    /// Keep alive interval in seconds
    #[arg(long, short = 'k', default_value = "60")]
    pub keep_alive: u16,

    /// Will topic (last will and testament)
    #[arg(long)]
    pub will_topic: Option<String>,

    /// Will message payload
    #[arg(long)]
    pub will_message: Option<String>,

    /// Will QoS level (0, 1, or 2)
    #[arg(long, value_parser = parse_qos)]
    pub will_qos: Option<QoS>,

    /// Will retain flag
    #[arg(long)]
    pub will_retain: bool,

    /// DTLS/TLS certificate file (PEM format) for secure connections
    #[arg(long)]
    pub cert: Option<PathBuf>,

    /// DTLS/TLS private key file (PEM format) for secure connections
    #[arg(long)]
    pub key: Option<PathBuf>,

    /// DTLS/TLS CA certificate file (PEM format) for server verification
    #[arg(long)]
    pub ca_cert: Option<PathBuf>,

    /// Skip certificate verification for TLS connections (insecure, for testing only)
    #[arg(long)]
    pub insecure: bool,

    /// DTLS PSK identity (for pre-shared key authentication)
    #[arg(long)]
    pub psk_identity: Option<String>,

    /// DTLS PSK key in hex format (for pre-shared key authentication)
    #[arg(long)]
    pub psk_key: Option<String>,

    /// Will delay interval in seconds
    #[arg(long)]
    pub will_delay: Option<u32>,

    /// Keep connection alive after publishing (for testing will messages)
    #[arg(long, hide = true)]
    pub keep_alive_after_publish: bool,
}

fn parse_qos(s: &str) -> Result<QoS, String> {
    match s {
        "0" => Ok(QoS::AtMostOnce),
        "1" => Ok(QoS::AtLeastOnce),
        "2" => Ok(QoS::ExactlyOnce),
        _ => Err(format!("QoS must be 0, 1, or 2, got: {s}")),
    }
}

pub async fn execute(mut cmd: PubCommand) -> Result<()> {
    // Smart prompting for missing required arguments
    if cmd.topic.is_none() && !cmd.non_interactive {
        let topic = Input::<String>::new()
            .with_prompt("MQTT topic (e.g., sensors/temperature, home/status)")
            .interact()
            .context("Failed to get topic input")?;
        cmd.topic = Some(topic);
    }

    let topic = cmd.topic.clone().ok_or_else(|| {
        anyhow::anyhow!("Topic is required. Use --topic or run without --non-interactive")
    })?;

    // Validate topic
    if topic.is_empty() {
        anyhow::bail!("Topic cannot be empty");
    }
    if topic.contains("//") {
        anyhow::bail!(
            "Invalid topic '{}' - cannot have empty segments\nDid you mean '{}'?",
            topic,
            topic.replace("//", "/")
        );
    }
    if topic.ends_with('/') {
        anyhow::bail!(
            "Invalid topic '{}' - cannot end with '/'\nDid you mean '{}'?",
            topic,
            topic.trim_end_matches('/')
        );
    }

    // Get message content with smart prompting
    let message = get_message_content(&mut cmd)
        .await
        .context("Failed to get message content")?;

    // Smart QoS prompting
    let qos = if cmd.qos.is_none() && !cmd.non_interactive {
        let qos_options = vec![
            "0 (At most once - fire and forget)",
            "1 (At least once - acknowledged)",
            "2 (Exactly once - assured)",
        ];
        let selection = Select::new()
            .with_prompt("Quality of Service level")
            .items(&qos_options)
            .default(0)
            .interact()
            .context("Failed to get QoS selection")?;

        match selection {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            2 => QoS::ExactlyOnce,
            _ => QoS::AtMostOnce,
        }
    } else {
        cmd.qos.unwrap_or(QoS::AtMostOnce)
    };

    // Build broker URL
    let broker_url = cmd
        .url
        .unwrap_or_else(|| format!("mqtt://{}:{}", cmd.host, cmd.port));
    debug!("Connecting to broker: {}", broker_url);

    // Create client
    let client_id = cmd
        .client_id
        .unwrap_or_else(|| format!("mqttv5-pub-{}", rand::rng().random::<u32>()));
    let client = MqttClient::new(&client_id);

    // Build connection options
    let mut options = ConnectOptions::new(client_id.clone())
        .with_clean_start(!cmd.no_clean_start)
        .with_keep_alive(Duration::from_secs(cmd.keep_alive.into()));

    // Add session expiry if specified
    if let Some(expiry) = cmd.session_expiry {
        options = options.with_session_expiry_interval(expiry);
    }

    // Add authentication if provided
    if let (Some(username), Some(password)) = (cmd.username.clone(), cmd.password.clone()) {
        options = options.with_credentials(username, password.into_bytes());
    } else if let Some(username) = cmd.username.clone() {
        options = options.with_credentials(username, Vec::new());
    }

    // Add will message if specified
    if let Some(topic) = cmd.will_topic.clone() {
        let payload = cmd.will_message.clone().unwrap_or_default();
        let mut will = WillMessage::new(topic, payload.into_bytes()).with_retain(cmd.will_retain);

        if let Some(qos) = cmd.will_qos {
            will = will.with_qos(qos);
        }

        if let Some(delay) = cmd.will_delay {
            will.properties.will_delay_interval = Some(delay);
        }

        options = options.with_will(will);
    }

    // Configure insecure TLS mode if requested
    if cmd.insecure {
        client.set_insecure_tls(true).await;
        info!("Insecure TLS mode enabled (certificate verification disabled)");
    }

    // Configure DTLS if using mqtts-dtls://
    if broker_url.starts_with("mqtts-dtls://") {
        // Configure with PSK if provided
        if let (Some(identity), Some(key_hex)) = (&cmd.psk_identity, &cmd.psk_key) {
            let psk_key =
                hex::decode(key_hex).context("Invalid PSK key format - must be hex encoded")?;
            client
                .set_dtls_config(
                    None,
                    None,
                    None,
                    Some(identity.as_bytes().to_vec()),
                    Some(psk_key),
                )
                .await;
        }
        // Or configure with certificates if provided
        else if let (Some(cert_path), Some(key_path)) = (&cmd.cert, &cmd.key) {
            let cert_pem = std::fs::read(cert_path)
                .with_context(|| format!("Failed to read certificate file: {cert_path:?}"))?;
            let key_pem = std::fs::read(key_path)
                .with_context(|| format!("Failed to read key file: {key_path:?}"))?;
            let ca_pem =
                if let Some(ca_path) = &cmd.ca_cert {
                    Some(std::fs::read(ca_path).with_context(|| {
                        format!("Failed to read CA certificate file: {ca_path:?}")
                    })?)
                } else {
                    None
                };

            client
                .set_dtls_config(Some(cert_pem), Some(key_pem), ca_pem, None, None)
                .await;
        }
    }

    // Connect
    info!("Connecting to {}...", broker_url);
    let result = client
        .connect_with_options(&broker_url, options)
        .await
        .context("Failed to connect to MQTT broker")?;

    if result.session_present {
        info!("Resumed existing session");
    }

    // Publish message
    info!("Publishing to topic '{}'...", topic);

    if cmd.retain {
        // Use publish_with_options when retain flag is set
        let options = PublishOptions {
            qos,
            retain: true,
            ..Default::default()
        };
        client
            .publish_with_options(&topic, message.as_bytes(), options)
            .await?;
    } else {
        // Use the regular publish methods when retain is not set
        match qos {
            QoS::AtMostOnce => {
                client.publish(&topic, message.as_bytes()).await?;
            }
            QoS::AtLeastOnce => {
                client.publish_qos1(&topic, message.as_bytes()).await?;
            }
            QoS::ExactlyOnce => {
                client.publish_qos2(&topic, message.as_bytes()).await?;
            }
        }
    }

    println!("✓ Published message to '{}' (QoS {})", topic, qos as u8);
    if cmd.retain {
        println!("  Message retained on broker");
    }

    // Keep connection alive if requested (for testing will messages)
    if cmd.keep_alive_after_publish {
        info!("Keeping connection alive (--keep-alive-after-publish)");
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    // Disconnect
    client.disconnect().await?;

    Ok(())
}

async fn get_message_content(cmd: &mut PubCommand) -> Result<String> {
    // Priority: stdin > file > message > prompt
    if cmd.stdin {
        debug!("Reading message from stdin");
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;
        return Ok(buffer.trim().to_string());
    }

    if let Some(file_path) = &cmd.file {
        debug!("Reading message from file: {}", file_path);
        return fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {file_path}"))
            .map(|s| s.trim().to_string());
    }

    if let Some(message) = &cmd.message {
        return Ok(message.clone());
    }

    // Smart prompting for message
    if !cmd.non_interactive {
        let message = Input::<String>::new()
            .with_prompt("Message content")
            .interact()
            .context("Failed to get message input")?;
        return Ok(message);
    }

    anyhow::bail!("Message content is required. Use one of:\n  --message \"your message\"\n  --file message.txt\n  --stdin\n  Or run without --non-interactive to be prompted");
}

use rand::Rng;
