use mqtt_v5::MqttClient;
use std::time::Duration;

#[tokio::test]
async fn test_simple_connection_and_subscribe() {
    println!("Creating client...");
    let client = MqttClient::new("debug-client");

    println!("Connecting to 127.0.0.1:1883...");
    match client.connect("mqtt://127.0.0.1:1883").await {
        Ok(_) => println!("Connected successfully"),
        Err(e) => {
            eprintln!("Failed to connect: {:?}", e);
            return;
        }
    }

    println!("Setting up subscription...");
    let result = client
        .subscribe("test/topic", |msg| {
            println!("Received message on {}: {:?}", msg.topic, msg.payload);
        })
        .await;

    match result {
        Ok(qos) => println!("Subscribed with QoS: {:?}", qos),
        Err(e) => {
            eprintln!("Failed to subscribe: {:?}", e);
            return;
        }
    }

    println!("Publishing message...");
    match client.publish("test/topic", b"Hello MQTT").await {
        Ok(packet_id) => println!("Published with packet_id: {:?}", packet_id),
        Err(e) => {
            eprintln!("Failed to publish: {:?}", e);
            return;
        }
    }

    println!("Waiting for message...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    println!("Disconnecting...");
    match client.disconnect().await {
        Ok(_) => println!("Disconnected successfully"),
        Err(e) => eprintln!("Failed to disconnect: {:?}", e),
    }
}
