# MQTT v5.0 Client Integration Tests

This directory contains comprehensive integration tests for the MQTT v5.0 client library.

## Test Categories

### 1. Basic Connection Tests (`client_connection.rs`)
- Connection establishment
- Authentication
- Clean session handling
- Protocol version negotiation

### 2. Publishing Tests (`client_publish.rs`)
- QoS 0/1/2 publishing
- Retained messages
- Large payloads
- Concurrent publishing

### 3. Subscription Tests (`client_subscribe.rs`)
- Topic filters and wildcards
- Multiple subscriptions
- Subscription options
- Callback handling

### 4. Complete Flow Tests (`integration_complete_flow.rs`)
- End-to-end MQTT workflows
- Multiple subscriptions with wildcards
- QoS level handling and downgrade
- Session persistence
- Concurrent operations
- Large payload handling

### 5. Reconnection Tests (`integration_reconnection.rs`)
- Automatic reconnection
- Exponential backoff
- Message queuing during disconnection
- Subscription restoration
- Keep-alive timeout detection

### 6. MQTT v5.0 Features (`integration_mqtt5_features.rs`)
- Properties system
- Will messages with delay
- Topic aliases
- Flow control (Receive Maximum)
- Subscription identifiers
- Shared subscriptions
- Maximum packet size
- Reason codes and strings

### 7. Additional Feature Tests
- **Connection Events** (`connection_events.rs`): Event callback system
- **Message Queuing** (`message_queuing.rs`): Offline message handling
- **Error Recovery** (`error_recovery.rs`): Error handling and recovery
- **Retained Messages** (`retained_messages.rs`): Retained message functionality
- **Will Messages** (`will_messages.rs`): Last Will and Testament
- **Enhanced Authentication** (`enhanced_auth.rs`): AUTH packet support
- **Flow Control** (`flow_control.rs`): QoS flow management
- **Topic Aliases** (`topic_aliases.rs`): Bandwidth optimization
- **Packet Size Limits** (`packet_size_limits.rs`): Size constraints

### 8. UDP Transport Tests
- **UDP Transport** (`udp_transport_test.rs`): Basic UDP functionality
- **UDP Broker Integration** (`udp_broker_integration.rs`): UDP broker communication
- **UDP Reliability** (`udp_reliability_test.rs`): Reliability layer testing
- **UDP CLI** (`cli_udp_comprehensive.rs`, `cli_udp_reliability.rs`): CLI UDP support
- **UDP QoS2** (`udp_qos2_sequence_test.rs`): QoS2 over UDP
- **DTLS Transport** (`dtls_transport_test.rs`): Secure UDP with DTLS
- **DTLS Certificates** (`dtls_certificate_test.rs`): DTLS certificate handling

## Running the Tests

### Prerequisites
1. Docker and Docker Compose installed
2. Rust toolchain (stable)
3. Port 1883 available for Mosquitto

### Running All Integration Tests
```bash
./run_integration_tests.sh
```

### Running Individual Test Files
```bash
# Start the broker
docker-compose up -d mosquitto

# Run specific test (use raw cargo for specific tests)
cargo test --test integration_complete_flow

# Stop the broker
docker-compose down
```

### Running Specific Tests
```bash
# Use raw cargo for specific tests with filters
cargo test --test integration_complete_flow test_complete_mqtt_flow
```

## Test Configuration

The tests use a Mosquitto broker configured in `docker-compose.yml` with:
- Standard MQTT port: 1883
- TLS port: 8883
- Configuration files in `comparative_benchmarks/test_configs/`

## Writing New Tests

When adding new integration tests:

1. Use unique client IDs with `test_client_id()` helper
2. Clean up resources (disconnect clients, clear retained messages)
3. Use appropriate timeouts for async operations
4. Test both success and failure scenarios
5. Verify MQTT v5.0 specific features when applicable

## Troubleshooting

### Common Issues

1. **Port already in use**: Ensure port 1883 is not used by another service
2. **Docker not running**: Start Docker Desktop/daemon
3. **Permission denied**: Check file permissions and Docker permissions
4. **Tests timeout**: Increase timeout values or check broker connectivity

### Debug Output

Run tests with debug output:
```bash
# Use raw cargo for specific test debugging
RUST_LOG=debug cargo test --test integration_complete_flow -- --nocapture
```

### Broker Logs

View Mosquitto logs:
```bash
docker-compose logs mosquitto
```