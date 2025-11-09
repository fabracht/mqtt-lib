use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

#[derive(Args)]
pub struct AclCommand {
    #[command(subcommand)]
    pub action: AclAction,
}

#[derive(Subcommand)]
pub enum AclAction {
    Add {
        #[arg(help = "Username (or * for wildcard)")]
        username: String,

        #[arg(help = "Topic pattern (supports + and # wildcards)")]
        topic: String,

        #[arg(help = "Permission: read, write, readwrite, or deny")]
        permission: String,

        #[arg(long, short, help = "ACL file path")]
        file: PathBuf,
    },

    Remove {
        #[arg(help = "Username to remove rules for")]
        username: String,

        #[arg(help = "Topic pattern to remove (optional, removes all if not specified)")]
        topic: Option<String>,

        #[arg(long, short, help = "ACL file path")]
        file: PathBuf,
    },

    List {
        #[arg(help = "Username to list rules for (optional, lists all if not specified)")]
        username: Option<String>,

        #[arg(long, short, help = "ACL file path")]
        file: PathBuf,
    },

    Check {
        #[arg(help = "Username to check")]
        username: String,

        #[arg(help = "Topic to check")]
        topic: String,

        #[arg(help = "Action: read or write")]
        action: String,

        #[arg(long, short, help = "ACL file path")]
        file: PathBuf,
    },
}

pub async fn execute(cmd: AclCommand) -> Result<()> {
    match cmd.action {
        AclAction::Add {
            username,
            topic,
            permission,
            file,
        } => handle_add(&username, &topic, &permission, &file).await,
        AclAction::Remove {
            username,
            topic,
            file,
        } => handle_remove(&username, topic.as_deref(), &file).await,
        AclAction::List { username, file } => handle_list(username.as_deref(), &file).await,
        AclAction::Check {
            username,
            topic,
            action,
            file,
        } => handle_check(&username, &topic, &action, &file).await,
    }
}

async fn handle_add(
    username: &str,
    topic: &str,
    permission: &str,
    file_path: &PathBuf,
) -> Result<()> {
    let valid_permissions = ["read", "write", "readwrite", "deny"];
    if !valid_permissions.contains(&permission) {
        bail!(
            "Invalid permission '{}'. Must be one of: {}",
            permission,
            valid_permissions.join(", ")
        );
    }

    if username.contains(char::is_whitespace) {
        bail!("Username cannot contain whitespace");
    }

    if topic.contains(char::is_whitespace) {
        bail!("Topic pattern cannot contain whitespace");
    }

    let mut rules = if file_path.exists() {
        read_acl_file(file_path).await?
    } else {
        Vec::new()
    };

    let rule_line = format!(
        "user {} topic {} permission {}",
        username, topic, permission
    );

    if rules.iter().any(|r| r == &rule_line) {
        println!("Rule already exists: {}", rule_line);
        return Ok(());
    }

    rules.push(rule_line.clone());
    write_acl_file(file_path, &rules).await?;

    println!("Added ACL rule: {}", rule_line);
    Ok(())
}

async fn handle_remove(username: &str, topic: Option<&str>, file_path: &PathBuf) -> Result<()> {
    if !file_path.exists() {
        bail!("ACL file does not exist: {}", file_path.display());
    }

    let mut rules = read_acl_file(file_path).await?;
    let original_count = rules.len();

    rules.retain(|rule| {
        let parts: Vec<&str> = rule.split_whitespace().collect();
        if parts.len() != 6 || parts[0] != "user" || parts[2] != "topic" || parts[4] != "permission"
        {
            return true;
        }

        let rule_username = parts[1];
        let rule_topic = parts[3];

        if rule_username != username {
            return true;
        }

        if let Some(topic_filter) = topic {
            rule_topic != topic_filter
        } else {
            false
        }
    });

    let removed_count = original_count - rules.len();

    if removed_count == 0 {
        if let Some(topic_filter) = topic {
            bail!(
                "No rules found for user '{}' with topic '{}'",
                username,
                topic_filter
            );
        } else {
            bail!("No rules found for user '{}'", username);
        }
    }

    write_acl_file(file_path, &rules).await?;

    if let Some(topic_filter) = topic {
        println!(
            "Removed {} rule(s) for user '{}' with topic '{}'",
            removed_count, username, topic_filter
        );
    } else {
        println!("Removed {} rule(s) for user '{}'", removed_count, username);
    }

    Ok(())
}

async fn handle_list(username: Option<&str>, file_path: &PathBuf) -> Result<()> {
    if !file_path.exists() {
        bail!("ACL file does not exist: {}", file_path.display());
    }

    let rules = read_acl_file(file_path).await?;

    if rules.is_empty() {
        println!("No ACL rules found");
        return Ok(());
    }

    let filtered_rules: Vec<&String> = if let Some(user_filter) = username {
        rules
            .iter()
            .filter(|rule| {
                let parts: Vec<&str> = rule.split_whitespace().collect();
                parts.len() >= 2 && parts[0] == "user" && parts[1] == user_filter
            })
            .collect()
    } else {
        rules.iter().collect()
    };

    if filtered_rules.is_empty() {
        if let Some(user_filter) = username {
            println!("No ACL rules found for user '{}'", user_filter);
        } else {
            println!("No ACL rules found");
        }
        return Ok(());
    }

    if let Some(user_filter) = username {
        println!("ACL rules for user '{}':", user_filter);
    } else {
        println!("All ACL rules:");
    }

    for rule in &filtered_rules {
        println!("  {}", rule);
    }

    println!("\nTotal: {} rule(s)", filtered_rules.len());
    Ok(())
}

async fn handle_check(
    username: &str,
    topic: &str,
    action: &str,
    file_path: &PathBuf,
) -> Result<()> {
    if action != "read" && action != "write" {
        bail!("Action must be 'read' or 'write', got: {}", action);
    }

    if !file_path.exists() {
        bail!("ACL file does not exist: {}", file_path.display());
    }

    use mqtt5::broker::acl::AclManager;

    let acl_manager = AclManager::from_file(file_path).await?;

    let allowed = if action == "read" {
        acl_manager.check_subscribe(Some(username), topic).await
    } else {
        acl_manager.check_publish(Some(username), topic).await
    };

    if allowed {
        println!(
            "✓ User '{}' is ALLOWED to {} topic '{}'",
            username, action, topic
        );
    } else {
        println!(
            "✗ User '{}' is DENIED to {} topic '{}'",
            username, action, topic
        );
    }

    Ok(())
}

async fn read_acl_file(path: &PathBuf) -> Result<Vec<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read ACL file: {}", path.display()))?;

    let mut rules = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        rules.push(line.to_string());
    }

    Ok(rules)
}

async fn write_acl_file(path: &PathBuf, rules: &[String]) -> Result<()> {
    let mut content = String::new();

    for rule in rules {
        content.push_str(rule);
        content.push('\n');
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("Failed to open ACL file: {}", path.display()))?;

    file.write_all(content.as_bytes())
        .with_context(|| format!("Failed to write ACL file: {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, permissions)
            .with_context(|| format!("Failed to set file permissions: {}", path.display()))?;
    }

    Ok(())
}
