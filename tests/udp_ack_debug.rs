//! Debug test for UDP ACK packet issue

use mqtt5::test_utils::TestPacketBuilder;
use mqtt5::transport::udp::{UdpConfig, UdpTransport};
use mqtt5::transport::Transport;
use std::time::Duration;
use tokio::net::UdpSocket;

#[tokio::test]
async fn test_udp_ack_packet_flow() {
    // Start a simple UDP echo server that logs packets
    let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = server_socket.local_addr().unwrap();

    // Server task - just echo back what it receives
    let server_handle = tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        loop {
            match server_socket.recv_from(&mut buf).await {
                Ok((len, peer_addr)) => {
                    println!("Server received {} bytes from {}", len, peer_addr);
                    println!("First 20 bytes: {:02x?}", &buf[..len.min(20)]);

                    // Echo back
                    server_socket.send_to(&buf[..len], peer_addr).await.ok();
                }
                Err(e) => {
                    println!("Server error: {}", e);
                    break;
                }
            }
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create client
    let config = UdpConfig::new(server_addr);
    let mut transport = UdpTransport::new(config);

    // Connect (this starts the background reliability task)
    transport.connect().await.expect("Should connect");

    // Send some data
    let test_data = b"Hello UDP";
    transport.write(test_data).await.expect("Should write");

    // Try to read response
    let mut read_buf = vec![0u8; 1500];
    let result = tokio::time::timeout(Duration::from_secs(2), transport.read(&mut read_buf)).await;

    match result {
        Ok(Ok(len)) => {
            println!("Client read {} bytes", len);
            println!("Data: {:?}", &read_buf[..len]);
        }
        Ok(Err(e)) => {
            println!("Client read error: {}", e);
        }
        Err(_) => {
            println!("Client read timeout");
        }
    }

    server_handle.abort();
}

#[tokio::test]
async fn test_udp_split_ack_flow() {
    use mqtt5::packet::{connect::ConnectPacket, Packet};
    use mqtt5::transport::{PacketReader, PacketWriter};

    // Start a simple UDP echo server
    let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = server_socket.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        loop {
            match server_socket.recv_from(&mut buf).await {
                Ok((len, peer_addr)) => {
                    println!("Server received {} bytes", len);

                    // Check packet type
                    if len > 0 {
                        let packet_type = buf[0];
                        println!("Packet type: 0x{:02x}", packet_type);

                        if packet_type == 0x01 {
                            // Data packet - send back an ACK
                            let ack = vec![
                                0x02, // ACK packet type
                                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // sequence 1
                                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // delay 0
                            ];
                            println!("Sending ACK: {:02x?}", ack);
                            server_socket.send_to(&ack, peer_addr).await.ok();
                        }
                    }
                }
                Err(e) => {
                    println!("Server error: {}", e);
                    break;
                }
            }
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create client and split it
    let config = UdpConfig::new(server_addr);
    let mut transport = UdpTransport::new(config);
    transport.connect().await.expect("Should connect");

    let (mut reader, mut writer) = transport.into_split().expect("Should split");

    // Send a CONNECT packet
    let connect = ConnectPacket::test_default();
    writer
        .write_packet(Packet::Connect(Box::new(connect)))
        .await
        .expect("Should write");

    // Try to read response
    let result = tokio::time::timeout(Duration::from_secs(2), reader.read_packet()).await;

    match result {
        Ok(Ok(packet)) => {
            println!("Client received packet: {:?}", packet);
        }
        Ok(Err(e)) => {
            println!("Client read error: {}", e);
        }
        Err(_) => {
            println!("Client read timeout");
        }
    }

    server_handle.abort();
}
