use anyhow::{Context, Result};
use clap::Args;
use dialoguer::{Input, Select};
use mqtt5::{MqttClient, QoS};
use tokio::signal;
use tracing::{debug, info};

#[derive(Args)]
pub struct SubCommand {
    /// MQTT topic to subscribe to (supports wildcards + and #)
    #[arg(long, short)]
    pub topic: Option<String>,

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
}

fn parse_qos(s: &str) -> Result<QoS, String> {
    match s {
        "0" => Ok(QoS::AtMostOnce),
        "1" => Ok(QoS::AtLeastOnce),
        "2" => Ok(QoS::ExactlyOnce),
        _ => Err(format!("QoS must be 0, 1, or 2, got: {}", s)),
    }
}

pub async fn execute(mut cmd: SubCommand) -> Result<()> {
    // Smart prompting for missing required arguments
    if cmd.topic.is_none() && !cmd.non_interactive {
        let topic = Input::<String>::new()
            .with_prompt("MQTT topic to subscribe to")
            .with_initial_text("sensors/+")
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
    let broker_url = format!("mqtt://{}:{}", cmd.host, cmd.port);
    debug!("Connecting to broker: {}", broker_url);

    // Create client
    let client_id = cmd
        .client_id
        .unwrap_or_else(|| format!("mqttv5-sub-{}", rand::rng().random::<u32>()));
    let client = MqttClient::new(&client_id);

    // Connect
    info!("Connecting to {}...", broker_url);
    client
        .connect(&broker_url)
        .await
        .context("Failed to connect to MQTT broker")?;

    // Subscribe with message counter
    let target_count = cmd.count;
    let verbose = cmd.verbose;

    info!("Subscribing to '{}' (QoS {})...", topic, qos as u8);
    println!(
        "✓ Subscribed to '{}' - waiting for messages (Ctrl+C to exit)",
        topic
    );

    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let message_count = Arc::new(AtomicU32::new(0));
    let message_count_clone = message_count.clone();

    let (packet_id, granted_qos) = client
        .subscribe(&topic, move |message| {
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
                println!("✓ Received {} messages, exiting", target_count);
                std::process::exit(0);
            }
        })
        .await?;

    debug!(
        "Subscription confirmed - packet_id: {}, granted_qos: {:?}",
        packet_id, granted_qos
    );

    // Wait for Ctrl+C
    match signal::ctrl_c().await {
        Ok(()) => {
            println!("\n✓ Received Ctrl+C, disconnecting...");
        }
        Err(err) => {
            anyhow::bail!("Unable to listen for shutdown signal: {}", err);
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
