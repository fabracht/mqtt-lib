use anyhow::{Context, Result};
use clap::Args;
use dialoguer::{Input, Select};
use hex;
use mqtt5::{ConnectOptions, MqttClient, QoS, WillMessage};
use std::path::PathBuf;
use std::time::Duration;
use tokio::signal;
use tracing::{debug, info};

#[derive(Args)]
pub struct SubCommand {
    /// MQTT topic to subscribe to (supports wildcards + and #)
    #[arg(long, short)]
    pub topic: Option<String>,

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

    /// Username for authentication
    #[arg(long, short)]
    pub username: Option<String>,

    /// Password for authentication
    #[arg(long, short = 'P')]
    pub password: Option<String>,

    /// Client ID
    #[arg(long, short)]
    pub client_id: Option<String>,

    /// Print verbose output (include topic names)
    #[arg(long, short)]
    pub verbose: bool,

    /// Skip prompts and use defaults/fail if required args missing
    #[arg(long)]
    pub non_interactive: bool,

    /// Number of messages to receive before exiting (0 = infinite)
    #[arg(long, short = 'n', default_value = "0")]
    pub count: u32,

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

    /// Will delay interval in seconds
    #[arg(long)]
    pub will_delay: Option<u32>,

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
}

fn parse_qos(s: &str) -> Result<QoS, String> {
    match s {
        "0" => Ok(QoS::AtMostOnce),
        "1" => Ok(QoS::AtLeastOnce),
        "2" => Ok(QoS::ExactlyOnce),
        _ => Err(format!("QoS must be 0, 1, or 2, got: {s}")),
    }
}

pub async fn execute(mut cmd: SubCommand) -> Result<()> {
    // Smart prompting for missing required arguments
    if cmd.topic.is_none() && !cmd.non_interactive {
        let topic = Input::<String>::new()
            .with_prompt("MQTT topic to subscribe to (e.g., sensors/+, home/#)")
            .interact()
            .context("Failed to get topic input")?;
        cmd.topic = Some(topic);
    }

    let topic = cmd.topic.ok_or_else(|| {
        anyhow::anyhow!("Topic is required. Use --topic or run without --non-interactive")
    })?;

    // Validate topic
    validate_topic_filter(&topic)?;

    // Smart QoS prompting
    let qos = if cmd.qos.is_none() && !cmd.non_interactive {
        let qos_options = vec!["0 (At most once)", "1 (At least once)", "2 (Exactly once)"];
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
        .unwrap_or_else(|| format!("mqttv5-sub-{}", rand::rng().random::<u32>()));
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
            let ca_pem = if let Some(ca_path) = &cmd.ca_cert {
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

    // Subscribe with message counter
    let target_count = cmd.count;
    let verbose = cmd.verbose;

    info!("Subscribing to '{}' (QoS {})...", topic, qos as u8);
    println!("✓ Subscribed to '{topic}' - waiting for messages (Ctrl+C to exit)");

    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use tokio::sync::Notify;

    let message_count = Arc::new(AtomicU32::new(0));
    let message_count_clone = message_count.clone();
    let done_notify = Arc::new(Notify::new());
    let done_notify_clone = done_notify.clone();

    let subscribe_options = mqtt5::SubscribeOptions {
        qos,
        ..Default::default()
    };

    let (packet_id, granted_qos) = client
        .subscribe_with_options(&topic, subscribe_options, move |message| {
            let count = message_count_clone.fetch_add(1, Ordering::Relaxed) + 1;

            if verbose {
                println!(
                    "{}: {}",
                    message.topic,
                    String::from_utf8_lossy(&message.payload)
                );
            } else {
                println!("{}", String::from_utf8_lossy(&message.payload));
            }

            // Check if we've reached the target count
            if target_count > 0 && count >= target_count {
                println!("✓ Received {target_count} messages, exiting");
                done_notify_clone.notify_one();
            }
        })
        .await?;

    debug!(
        "Subscription confirmed - packet_id: {}, granted_qos: {:?}",
        packet_id, granted_qos
    );

    // Wait for either Ctrl+C or target count reached
    if target_count > 0 || cmd.non_interactive {
        // If we have a target count or are in non-interactive mode,
        // wait for either the count to be reached or Ctrl+C
        tokio::select! {
            _ = done_notify.notified() => {
                // Target count reached
            }
            _ = signal::ctrl_c() => {
                println!("\n✓ Received Ctrl+C, disconnecting...");
            }
        }
    } else {
        // Interactive mode with no target count - just wait for Ctrl+C
        match signal::ctrl_c().await {
            Ok(()) => {
                println!("\n✓ Received Ctrl+C, disconnecting...");
            }
            Err(err) => {
                anyhow::bail!("Unable to listen for shutdown signal: {}", err);
            }
        }
    }

    // Disconnect
    client.disconnect().await?;

    Ok(())
}

fn validate_topic_filter(topic: &str) -> Result<()> {
    if topic.is_empty() {
        anyhow::bail!("Topic cannot be empty");
    }

    // Check for invalid patterns
    if topic.contains("//") {
        anyhow::bail!(
            "Invalid topic filter '{}' - cannot have empty segments\nDid you mean '{}'?",
            topic,
            topic.replace("//", "/")
        );
    }

    // Validate wildcard usage
    let segments: Vec<&str> = topic.split('/').collect();
    for (i, segment) in segments.iter().enumerate() {
        if segment == &"#" {
            // # must be last segment and alone
            if i != segments.len() - 1 {
                anyhow::bail!(
                    "Invalid topic filter '{}' - '#' wildcard must be the last segment",
                    topic
                );
            }
        } else if segment.contains('#') {
            anyhow::bail!(
                "Invalid topic filter '{}' - '#' wildcard must be alone in its segment",
                topic
            );
        } else if segment.contains('+') && segment != &"+" {
            anyhow::bail!(
                "Invalid topic filter '{}' - '+' wildcard must be alone in its segment",
                topic
            );
        }
    }

    Ok(())
}

use rand::Rng;
