use anyhow::{Context, Result};
use clap::Args;
use dialoguer::{Input, Select};
use mqtt_v5::{MqttClient, QoS};
use std::fs;
use std::io::{self, Read};
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
}

fn parse_qos(s: &str) -> Result<QoS, String> {
    match s {
        "0" => Ok(QoS::AtMostOnce),
        "1" => Ok(QoS::AtLeastOnce),
        "2" => Ok(QoS::ExactlyOnce),
        _ => Err(format!("QoS must be 0, 1, or 2, got: {}", s)),
    }
}

pub async fn execute(mut cmd: PubCommand) -> Result<()> {
    // Smart prompting for missing required arguments
    if cmd.topic.is_none() && !cmd.non_interactive {
        let topic = Input::<String>::new()
            .with_prompt("MQTT topic")
            .with_initial_text("sensors/")
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
    let broker_url = format!("mqtt://{}:{}", cmd.host, cmd.port);
    debug!("Connecting to broker: {}", broker_url);

    // Create client
    let client_id = cmd
        .client_id
        .unwrap_or_else(|| format!("mqttv5-pub-{}", rand::thread_rng().gen::<u32>()));
    let client = MqttClient::new(&client_id);

    // Connect
    info!("Connecting to {}...", broker_url);
    client
        .connect(&broker_url)
        .await
        .context("Failed to connect to MQTT broker")?;

    // Publish message
    info!("Publishing to topic '{}'...", topic);
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

    println!("✓ Published message to '{}' (QoS {})", topic, qos as u8);
    if cmd.retain {
        println!("  Message retained on broker");
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
            .with_context(|| format!("Failed to read file: {}", file_path))
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
