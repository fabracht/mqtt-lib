use mqtt5::MqttClient;

#[tokio::test]
async fn test_minimal_connection() {
    println!("Creating client...");
    let client = MqttClient::new("test-minimal-client");

    println!("Connecting to mqtt://127.0.0.1:1883...");
    match client.connect("mqtt://127.0.0.1:1883").await {
        Ok(()) => {
            println!("Connected successfully!");
            assert!(client.is_connected().await);

            println!("Disconnecting...");
            client.disconnect().await.unwrap();
            assert!(!client.is_connected().await);
            println!("Test passed!");
        }
        Err(e) => {
            println!("Connection failed: {e:?}");
            println!("Error type: {e}");
            panic!("Failed to connect");
        }
    }
}
