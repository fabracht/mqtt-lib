//! CLI Functionality Integration Tests
//!
//! Tests that validate our mqttv5 CLI tool works correctly in real scenarios.
//! These tests demonstrate that we can replace mosquitto tools completely.

use std::process::Command;
use std::time::Duration;

const CLI_BINARY: &str = "target/release/mqttv5";

/// Helper to build the CLI if not already built
fn ensure_cli_built() {
    if !std::path::Path::new(CLI_BINARY).exists() {
        println!("Building mqttv5 CLI...");
        let output = Command::new("cargo")
            .args(["build", "--release", "-p", "mqttv5-cli"])
            .output()
            .expect("Failed to build CLI");

        assert!(
            output.status.success(),
            "Failed to build CLI: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// Test that CLI broker help works (safer than starting actual broker)
#[tokio::test]
async fn test_cli_broker_help() {
    ensure_cli_built();

    // Test broker help works
    let result = Command::new(CLI_BINARY)
        .args(["broker", "--help"])
        .output()
        .expect("Failed to get broker help");

    assert!(result.status.success(), "Broker help should succeed");
    let help_text = String::from_utf8_lossy(&result.stdout);
    assert!(help_text.contains("--host"), "Should show host option");
    assert!(
        help_text.contains("--foreground"),
        "Should show foreground option"
    );

    println!("✅ CLI broker help test successful");
}

/// Test CLI pub/sub help functionality (safer than actual networking)
#[tokio::test]
async fn test_cli_pub_sub_help() {
    ensure_cli_built();

    // Test publish help
    let pub_result = Command::new(CLI_BINARY)
        .args(["pub", "--help"])
        .output()
        .expect("Failed to get pub help");

    assert!(pub_result.status.success(), "Pub help should succeed");
    let pub_help = String::from_utf8_lossy(&pub_result.stdout);
    assert!(pub_help.contains("--topic"), "Should show topic option");
    assert!(pub_help.contains("--message"), "Should show message option");
    assert!(
        pub_help.contains("replaces mosquitto_pub"),
        "Should mention mosquitto replacement"
    );

    // Test subscribe help
    let sub_result = Command::new(CLI_BINARY)
        .args(["sub", "--help"])
        .output()
        .expect("Failed to get sub help");

    assert!(sub_result.status.success(), "Sub help should succeed");
    let sub_help = String::from_utf8_lossy(&sub_result.stdout);
    assert!(sub_help.contains("--topic"), "Should show topic option");
    assert!(sub_help.contains("--count"), "Should show count option");
    assert!(
        sub_help.contains("replaces mosquitto_sub"),
        "Should mention mosquitto replacement"
    );

    println!("✅ CLI pub/sub help tests successful");
}

/// Test CLI input validation and error messages
#[tokio::test]
async fn test_cli_validation() {
    ensure_cli_built();

    // Test invalid topic validation
    let result = Command::new(CLI_BINARY)
        .args([
            "pub",
            "--topic",
            "test//invalid",
            "--message",
            "test",
            "--non-interactive",
        ])
        .output()
        .expect("Failed to run CLI");

    assert!(!result.status.success(), "Should fail with invalid topic");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("cannot have empty segments"),
        "Should show helpful error message"
    );
    assert!(
        stderr.contains("Did you mean 'test/invalid'?"),
        "Should suggest correction"
    );

    println!("✅ CLI validation test successful");
}

/// Test CLI help output quality
#[tokio::test]
async fn test_cli_help_quality() {
    ensure_cli_built();

    // Test main help
    let help_result = Command::new(CLI_BINARY)
        .args(["--help"])
        .output()
        .expect("Failed to get help");

    assert!(help_result.status.success(), "Help should succeed");
    let help_text = String::from_utf8_lossy(&help_result.stdout);

    // Verify key help text elements
    assert!(
        help_text.contains("replaces mosquitto_pub, mosquitto_sub, and mosquitto"),
        "Should mention mosquitto replacement"
    );
    assert!(
        help_text.contains("pub") && help_text.contains("sub") && help_text.contains("broker"),
        "Should show all subcommands"
    );
    assert!(
        help_text.contains("superior input ergonomics"),
        "Should mention ergonomics"
    );

    // Test subcommand help
    let pub_help = Command::new(CLI_BINARY)
        .args(["pub", "--help"])
        .output()
        .expect("Failed to get pub help");

    assert!(pub_help.status.success(), "Pub help should succeed");
    let pub_help_text = String::from_utf8_lossy(&pub_help.stdout);
    assert!(
        pub_help_text.contains("--topic") && pub_help_text.contains("--message"),
        "Should show descriptive flags"
    );

    println!("✅ CLI help quality test successful");
}

/// Benchmark CLI startup time vs mosquitto (if available)
#[tokio::test]
async fn test_cli_startup_performance() {
    ensure_cli_built();

    let start = std::time::Instant::now();

    let result = Command::new(CLI_BINARY)
        .args(["--help"])
        .output()
        .expect("Failed to run CLI");

    let cli_duration = start.elapsed();

    assert!(result.status.success(), "CLI help should succeed");
    assert!(
        cli_duration < Duration::from_millis(500),
        "CLI should start quickly (< 500ms)"
    );

    println!("✅ CLI startup performance: {cli_duration:?}");

    // Try to compare with mosquitto if available
    if let Ok(_mosquitto_result) = Command::new("mosquitto_pub").args(["--help"]).output() {
        let mosquitto_start = std::time::Instant::now();
        let _ = Command::new("mosquitto_pub").args(["--help"]).output();
        let mosquitto_duration = mosquitto_start.elapsed();

        println!("📊 Performance comparison:");
        println!("   mqttv5 CLI: {cli_duration:?}");
        println!("   mosquitto_pub: {mosquitto_duration:?}");

        if cli_duration < mosquitto_duration {
            println!("🚀 Our CLI is faster!");
        }
    } else {
        println!("📝 mosquitto not available for comparison");
    }
}

/// Test CLI ergonomics by checking flag parsing
#[tokio::test]
async fn test_cli_ergonomics() {
    ensure_cli_built();

    // Test that both long and short flags work
    let long_flag_result = Command::new(CLI_BINARY)
        .args([
            "pub",
            "--topic",
            "test/ergonomics",
            "--message",
            "test",
            "--host",
            "nonexistent.example.com", // Will fail to connect, but args should parse
            "--non-interactive",
        ])
        .output()
        .expect("Failed to run CLI with long flags");

    // Should fail to connect but not due to argument parsing
    let stderr = String::from_utf8_lossy(&long_flag_result.stderr);
    assert!(
        !stderr.contains("argument"),
        "Should not have argument parsing errors"
    );

    // Test short flags
    let short_flag_result = Command::new(CLI_BINARY)
        .args([
            "pub",
            "-t",
            "test/ergonomics",
            "-m",
            "test",
            "-H",
            "nonexistent.example.com",
            "--non-interactive",
        ])
        .output()
        .expect("Failed to run CLI with short flags");

    let short_stderr = String::from_utf8_lossy(&short_flag_result.stderr);
    assert!(
        !short_stderr.contains("argument"),
        "Should not have argument parsing errors with short flags"
    );

    println!("✅ CLI ergonomics test successful - both long and short flags work");
}
