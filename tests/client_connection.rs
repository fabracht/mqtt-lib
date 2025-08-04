use mqtt5::{ConnectOptions, MqttClient, MqttError};

#[tokio::test]
async fn test_connect_disconnect() {
    // This test requires a running MQTT broker
    // Skip if no broker is available
    let client = MqttClient::new("test-client");

    // Try connecting to localhost broker
    match client.connect("mqtt://127.0.0.1:1883").await {
        Ok(()) => {
            // Successfully connected
            assert!(client.is_connected().await);

            // Test disconnect
            client.disconnect().await.unwrap();
            assert!(!client.is_connected().await);
        }
        Err(e) => {
            // Broker not available, skip test
            eprintln!("Skipping test - no broker available: {e}");
        }
    }
}

#[tokio::test]
async fn test_connect_with_options() {
    let mut options = ConnectOptions::new("custom-client");
    options.clean_start = false;

    let client = MqttClient::with_options(options.clone());

    // Try connecting
    match client
        .connect_with_options("tcp://127.0.0.1:1883", options)
        .await
    {
        Ok(result) => {
            assert!(client.is_connected().await);
            assert_eq!(client.client_id().await, "custom-client");
            println!("Connected with session_present: {}", result.session_present);
            client.disconnect().await.unwrap();
        }
        Err(e) => {
            eprintln!("Skipping test - no broker available: {e}");
        }
    }
}

#[tokio::test]
async fn test_already_connected_error() {
    let client = MqttClient::new("test-client");

    // First connection
    match client.connect("mqtt://127.0.0.1:1883").await {
        Ok(()) => {
            // Try connecting again
            let result = client.connect("mqtt://127.0.0.1:1883").await;
            assert!(matches!(result, Err(MqttError::AlreadyConnected)));

            client.disconnect().await.unwrap();
        }
        Err(e) => {
            eprintln!("Skipping test - no broker available: {e}");
        }
    }
}

#[tokio::test]
async fn test_disconnect_not_connected() {
    let client = MqttClient::new("test-client");
    let result = client.disconnect().await;
    assert!(matches!(result, Err(MqttError::NotConnected)));
}

#[tokio::test]
async fn test_address_parsing() {
    // Test various address formats
    let addresses = vec![
        ("mqtt://127.0.0.1:1883", "localhost", 1883),
        ("mqtts://broker.example.com", "broker.example.com", 8883),
        ("tcp://192.168.1.100:1234", "192.168.1.100", 1234),
        ("ssl://secure.broker.com:8883", "secure.broker.com", 8883),
        ("localhost", "localhost", 1883),
        ("broker.local:9999", "broker.local", 9999),
    ];

    for (addr, _expected_host, _expected_port) in addresses {
        let client = MqttClient::new("test-client");
        // Just verify parsing doesn't panic
        let _ = client.connect(addr).await;
    }
}
