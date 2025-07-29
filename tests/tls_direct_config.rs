use mqtt_v5::transport::tls::TlsConfig;
use mqtt_v5::validation::{RestrictiveValidator, StandardValidator, TopicValidator};
use mqtt_v5::{ConnectOptions, MqttClient};

// For AWS IoT validator, import from the submodule
use mqtt_v5::validation::namespace::NamespaceValidator;

#[tokio::test]
#[ignore = "Integration test - requires certificates and TLS broker"]
async fn test_direct_tls_config_with_alpn() {
    // This test demonstrates the new direct TLS configuration API
    let client = MqttClient::new("direct-tls-test");

    // Create TLS config with AWS IoT settings
    let tls_config = TlsConfig::new(
        "test-endpoint.iot.us-east-1.amazonaws.com:443"
            .parse()
            .unwrap(),
        "test-endpoint.iot.us-east-1.amazonaws.com",
    )
    .with_aws_iot_alpn() // Convenience method for AWS IoT ALPN
    .with_verify_server_cert(false); // For testing with self-signed certs

    // This would work with actual certificates:
    // .load_client_cert_pem("device-cert.pem")
    // .load_client_key_pem("device-key.pem")
    // .load_ca_cert_pem("AmazonRootCA1.pem")

    let result = client.connect_with_tls(tls_config).await;

    // This will fail in CI since we don't have a real AWS IoT endpoint
    // but demonstrates the API
    assert!(result.is_err());

    // Verify error is connection-related, not API-related
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("connect") || error_msg.contains("TLS"));
}

#[tokio::test]
#[ignore = "Integration test - requires certificates and TLS broker"]
async fn test_tls_config_with_custom_alpn() {
    let client = MqttClient::new("custom-alpn-test");

    // Demonstrate custom ALPN protocols
    let tls_config = TlsConfig::new(
        "broker.example.com:8883".parse().unwrap(),
        "broker.example.com",
    )
    .with_alpn_protocols(&["mqtt", "http/1.1"])
    .with_verify_server_cert(false);

    let result = client.connect_with_tls(tls_config).await;

    // Will fail without real broker, but API should work
    assert!(result.is_err());
}

#[tokio::test]
#[ignore = "Integration test - requires certificates and broker"]
async fn test_tls_config_with_connect_options() {
    // Demonstrate combining TLS config with MQTT options
    let mut options = ConnectOptions::new("tls-options-test")
        .with_clean_start(false)
        .with_keep_alive(std::time::Duration::from_secs(30));

    // Set AWS IoT message size limit
    options.properties.maximum_packet_size = Some(131072); // 128KB

    let tls_config = TlsConfig::new(
        "test-endpoint.iot.us-east-1.amazonaws.com:443"
            .parse()
            .unwrap(),
        "test-endpoint.iot.us-east-1.amazonaws.com",
    )
    .with_aws_iot_alpn();

    let client = MqttClient::with_options(options.clone());
    let result = client
        .connect_with_tls_and_options(tls_config, options)
        .await;

    // Will fail without real broker, but demonstrates combined API
    assert!(result.is_err());
}

#[test]
fn test_topic_validators() {
    // Test standard validator
    let standard = StandardValidator;
    assert!(standard.validate_topic_name("sensor/temperature").is_ok());
    assert!(standard.validate_topic_filter("sensor/+").is_ok());
    assert!(!standard.is_reserved_topic("any/topic"));

    // Test restrictive validator
    let restrictive = RestrictiveValidator::new()
        .with_reserved_prefix("internal/")
        .with_max_levels(3)
        .with_max_topic_length(50);

    assert!(restrictive
        .validate_topic_name("public/sensor/temp")
        .is_ok());
    assert!(restrictive.validate_topic_name("internal/secret").is_err());
    assert!(restrictive.validate_topic_name("a/b/c/d").is_err()); // Too many levels
    assert!(restrictive.is_reserved_topic("internal/anything"));

    // Test AWS IoT validator
    let aws_validator = NamespaceValidator::aws_iot().with_device_id("my-device");

    // Device-specific topics should work
    assert!(aws_validator
        .validate_topic_name("$aws/thing/my-device/shadow/update")
        .is_ok());

    // Other device topics should fail
    assert!(aws_validator
        .validate_topic_name("$aws/thing/other-device/shadow/update")
        .is_err());

    // System topics should fail by default
    assert!(aws_validator
        .validate_topic_name("$SYS/broker/version")
        .is_err());
    assert!(aws_validator.is_reserved_topic("$SYS/broker/version"));

    // Regular topics should work
    assert!(aws_validator
        .validate_topic_name("sensor/temperature")
        .is_ok());
    assert!(!aws_validator.is_reserved_topic("sensor/temperature"));
}

#[test]
fn test_aws_iot_validator_device_specific() {
    let validator = NamespaceValidator::aws_iot().with_device_id("sensor-001");

    // Allowed device topics
    assert!(validator
        .validate_topic_name("$aws/thing/sensor-001/shadow/update")
        .is_ok());
    assert!(validator
        .validate_topic_name("$aws/thing/sensor-001/jobs/get")
        .is_ok());

    // Rejected device topics
    assert!(validator
        .validate_topic_name("$aws/thing/sensor-002/shadow/update")
        .is_err());
    assert!(validator.is_reserved_topic("$aws/thing/sensor-002/shadow/update"));

    // General AWS topics should be allowed
    assert!(validator
        .validate_topic_name("$aws/events/presence/connected/sensor-001")
        .is_ok());
}

#[test]
fn test_aws_iot_validator_system_topics() {
    let validator = NamespaceValidator::new("$aws", "thing").with_system_topics(true);

    // System topics should now be allowed
    assert!(validator.validate_topic_name("$SYS/broker/version").is_ok());
    assert!(validator.validate_topic_name("$SYS/broker/uptime").is_ok());
    assert!(!validator.is_reserved_topic("$SYS/broker/version"));
}

#[test]
fn test_restrictive_validator_features() {
    let validator = RestrictiveValidator::new()
        .with_reserved_prefix("company/")
        .with_reserved_prefix("internal/")
        .with_max_levels(4)
        .with_max_topic_length(30)
        .with_prohibited_char(' ')
        .with_prohibited_char('\t');

    // Test reserved prefixes
    assert!(validator.validate_topic_name("company/secret").is_err());
    assert!(validator.validate_topic_name("internal/admin").is_err());
    assert!(validator.is_reserved_topic("company/anything"));

    // Test max levels
    assert!(validator.validate_topic_name("a/b/c/d").is_ok()); // 4 levels OK
    assert!(validator.validate_topic_name("a/b/c/d/e").is_err()); // 5 levels too many

    // Test max length
    assert!(validator.validate_topic_name("short").is_ok());
    assert!(validator
        .validate_topic_name("this_is_a_very_long_topic_name_that_exceeds_limit")
        .is_err());

    // Test prohibited characters
    assert!(validator.validate_topic_name("topic with space").is_err());
    assert!(validator.validate_topic_name("topic\twith\ttab").is_err());
    assert!(validator.validate_topic_name("normal/topic").is_ok());
}

#[test]
fn test_alpn_protocol_validation() {
    // Test valid ALPN protocols
    let config = TlsConfig::new("127.0.0.1:8883".parse().unwrap(), "localhost")
        .with_alpn_protocols(&["mqtt", "http/1.1"]);

    // Should not panic
    drop(config);

    // Test AWS IoT convenience method
    let aws_config =
        TlsConfig::new("127.0.0.1:443".parse().unwrap(), "localhost").with_aws_iot_alpn();

    drop(aws_config);
}

#[test]
#[should_panic(expected = "ALPN protocol cannot be empty")]
fn test_alpn_empty_protocol_validation() {
    // This will panic due to empty protocol validation
    TlsConfig::new("127.0.0.1:8883".parse().unwrap(), "localhost")
        .with_alpn_protocols(&["mqtt", ""]);
}

#[test]
#[should_panic(expected = "ALPN protocol cannot exceed 255 bytes")]
fn test_alpn_long_protocol_validation() {
    let long_protocol = "a".repeat(256);
    // This will panic due to protocol length validation
    TlsConfig::new("127.0.0.1:8883".parse().unwrap(), "localhost")
        .with_alpn_protocols(&[&long_protocol]);
}
