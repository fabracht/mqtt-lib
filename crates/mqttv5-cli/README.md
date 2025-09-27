# mqttv5 - Superior MQTT v5.0 Command Line Interface

[![Crates.io](https://img.shields.io/crates/v/mqttv5-cli.svg)](https://crates.io/crates/mqttv5-cli)
[![Downloads](https://img.shields.io/crates/d/mqttv5-cli.svg)](https://crates.io/crates/mqttv5-cli)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/fabriciobracht/mqtt-lib#license)

A unified MQTT v5.0 CLI tool that replaces mosquitto_pub, mosquitto_sub, and mosquitto with superior ergonomics and user experience.

## Features

- **Unified Interface**: Single binary with pub, sub, and broker subcommands
- **Smart Prompting**: Interactive prompts for missing arguments
- **Superior Error Messages**: Helpful validation with correction suggestions
- **Full MQTT v5.0**: Complete protocol support including all v5.0 features
- **Advanced Session Management**: Clean start, session expiry, and persistence
- **Will Message Support**: Last will and testament with delay and QoS options
- **Multi-Transport**: TCP, TLS, WebSocket, UDP, and DTLS support
- **Comprehensive Testing**: Thoroughly tested CLI with real broker integration
- **Cross-Platform**: Works on Linux, macOS, and Windows

## Installation

```bash
cargo install mqttv5-cli
```

## Usage

### Publishing Messages

```bash
# Basic publish
mqttv5 pub --topic "sensors/temperature" --message "23.5°C"

# With QoS and retain
mqttv5 pub -t "sensors/temperature" -m "23.5°C" --qos 1 --retain

# Interactive mode (prompts for missing args)
mqttv5 pub
```

### Subscribing to Topics

```bash
# Basic subscribe
mqttv5 sub --topic "sensors/+"

# Verbose mode shows topic names
mqttv5 sub -t "sensors/#" --verbose

# Subscribe for specific message count
mqttv5 sub -t "test/topic" --count 5

# Session persistence and QoS
mqttv5 sub -t "data/#" --qos 2 --no-clean-start --session-expiry 3600
```

### Running a Broker

```bash
# Start broker on default port
mqttv5 broker

# Custom port and bind address
mqttv5 broker --host 0.0.0.0:1883

# Interactive configuration
mqttv5 broker
```

## Key Advantages over mosquitto

1. **Better Error Messages**: Clear, actionable error messages with suggestions
2. **Smart Defaults**: Intelligent prompting for missing required arguments
3. **Unified Tool**: One binary replaces mosquitto_pub, mosquitto_sub, and mosquitto
4. **Modern CLI Design**: Consistent flags and intuitive interface
5. **Full v5.0 Support**: All MQTT v5.0 features including properties and reason codes

## Examples

### Publishing sensor data
```bash
mqttv5 pub -t "home/living-room/temperature" -m "22.5" --qos 1
```

### Monitoring all home sensors
```bash
mqttv5 sub -t "home/+/+" --verbose
```

### Testing with retained messages
```bash
mqttv5 pub -t "config/device1" -m '{"enabled": true}' --retain
```

### Advanced MQTT v5.0 Features
```bash
# Will messages for device monitoring
mqttv5 pub -t "sensors/data" -m "active" \
  --will-topic "sensors/status" --will-message "offline" --will-delay 5

# Authentication and session management
mqttv5 sub -t "secure/data" --username user1 --password secret \
  --no-clean-start --session-expiry 7200

# Custom keep-alive and transport options
mqttv5 pub -t "test/topic" -m "data" --keep-alive 120 \
  --url "mqtts://secure-broker:8883"

# UDP transport with automatic reliability
mqttv5 pub --url "mqtt-udp://broker:1884" -t "test/udp" -m "UDP message"
mqttv5 sub --url "mqtt-udp://broker:1884" -t "test/+"

# DTLS (secure UDP) transport
mqttv5 pub --url "mqtts-dtls://secure-broker:8884" -t "secure/data" -m "DTLS secured"
```

## Environment Variables

- `MQTT_HOST`: Default broker host (default: localhost)
- `MQTT_PORT`: Default broker port (default: 1883)

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.