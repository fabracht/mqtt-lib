# MQTT CLI Testing Guide

Comprehensive test commands for the mqttv5 CLI tool with Mosquitto broker.

## Prerequisites

```bash
# Install the CLI tool
cargo install --path crates/mqttv5-cli

# Mosquitto broker running on:
# - TCP: localhost:1883
# - TLS: localhost:8883
# - WebSocket: localhost:9001
# - WebSocket TLS: localhost:9443
```

## Terminal Setup

**Terminal 1 (Subscriber):** Run subscriber commands
**Terminal 2 (Publisher):** Run publisher commands

---

## Basic Connectivity Tests

### 1. TCP Connection (Default)

**Terminal 1 - Subscribe:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic test/basic --non-interactive
```

**Terminal 2 - Publish:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic test/basic --message "Hello TCP" --non-interactive
```

---

### 2. TLS Connection

**Terminal 1 - Subscribe:**

```bash
mqttv5 sub --url ssl://localhost:8883 --topic test/tls --insecure --non-interactive
```

**Terminal 2 - Publish:**

```bash
mqttv5 pub --url ssl://localhost:8883 --topic test/tls --message "Hello TLS" --insecure --non-interactive
```

---

### 2b. Mutual TLS (mTLS) Connection

Requires mosquitto broker configured with mTLS or our mqttv5 broker with mTLS enabled.

**Start mqttv5 broker with mTLS (for testing):**

```bash
mqttv5 broker \
  --tls-cert test_certs/server.pem \
  --tls-key test_certs/server.key \
  --tls-ca-cert test_certs/ca.pem \
  --tls-require-client-cert \
  --allow-anonymous
```

**Terminal 1 - Subscribe with client certificate:**

```bash
mqttv5 sub \
  --url ssl://localhost:8883 \
  --topic test/mtls \
  --ca-cert test_certs/ca.pem \
  --cert test_certs/client.pem \
  --key test_certs/client.key \
  --non-interactive
```

**Terminal 2 - Publish with client certificate:**

```bash
mqttv5 pub \
  --url ssl://localhost:8883 \
  --topic test/mtls \
  --message "Hello mTLS" \
  --ca-cert test_certs/ca.pem \
  --cert test_certs/client.pem \
  --key test_certs/client.key \
  --non-interactive
```

**Test connection rejection without client cert:**

```bash
mqttv5 sub \
  --url ssl://localhost:8883 \
  --topic test/mtls \
  --ca-cert test_certs/ca.pem \
  --insecure \
  --non-interactive
```

Expected: Connection fails with TLS handshake error (client certificate required)

---

### 3. WebSocket Connection

**Terminal 1 - Subscribe:**

```bash
mqttv5 sub --url ws://localhost:9001 --topic test/ws --non-interactive
```

**Terminal 2 - Publish:**

```bash
mqttv5 pub --url ws://localhost:9001 --topic test/ws --message "Hello WebSocket" --non-interactive
```

---

### 4. WebSocket TLS Connection

**Terminal 1 - Subscribe:**

```bash
mqttv5 sub --url wss://localhost:9443 --topic test/wss --insecure --non-interactive
```

**Terminal 2 - Publish:**

```bash
mqttv5 pub --url wss://localhost:9443 --topic test/wss --message "Hello WSS" --insecure --non-interactive
```

---

## QoS Level Tests

### QoS 0 (At Most Once)

**Terminal 1:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic test/qos0 --qos 0 --non-interactive
```

**Terminal 2:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic test/qos0 --message "QoS 0 message" --qos 0 --non-interactive
```

---

### QoS 1 (At Least Once)

**Terminal 1:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic test/qos1 --qos 1 --non-interactive
```

**Terminal 2:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic test/qos1 --message "QoS 1 message" --qos 1 --non-interactive
```

---

### QoS 2 (Exactly Once)

**Terminal 1:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic test/qos2 --qos 2 --non-interactive
```

**Terminal 2:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic test/qos2 --message "QoS 2 message" --qos 2 --non-interactive
```

---

### QoS 2 over WebSocket (NEW FIX)

**Terminal 1:**

```bash
mqttv5 sub --url ws://localhost:9001 --topic test/ws/qos2 --qos 2 --non-interactive
```

**Terminal 2:**

```bash
mqttv5 pub --url ws://localhost:9001 --topic test/ws/qos2 --message "QoS 2 over WebSocket" --qos 2 --non-interactive
```

---

## Wildcard Subscription Tests

### Single-level Wildcard (+)

**Terminal 1:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic sensors/+/temperature --verbose --non-interactive
```

**Terminal 2:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic sensors/room1/temperature --message "22.5" --non-interactive
mqttv5 pub --url tcp://localhost:1883 --topic sensors/room2/temperature --message "23.1" --non-interactive
mqttv5 pub --url tcp://localhost:1883 --topic sensors/outdoor/temperature --message "18.3" --non-interactive
```

---

### Multi-level Wildcard (#)

**Terminal 1:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic home/# --verbose --non-interactive
```

**Terminal 2:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic home/living/light --message "on" --non-interactive
mqttv5 pub --url tcp://localhost:1883 --topic home/bedroom/temp --message "20" --non-interactive
mqttv5 pub --url tcp://localhost:1883 --topic home/kitchen/humidity --message "45" --non-interactive
```

---

## Retained Messages

### Publish Retained Message

**Terminal 2:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic status/system --message "online" --retain --non-interactive
```

**Terminal 1 (subscribe AFTER publishing):**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic status/system --count 1 --non-interactive
```

Expected: Subscriber receives "online" immediately

---

## Session Persistence

### Persistent Session (Receive Offline Messages)

**Terminal 1 - Start subscriber with persistent session:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic test/offline --qos 1 --client-id test-client-123 --no-clean-start --session-expiry 300 --count 3 --non-interactive
```

**Press Ctrl+C to disconnect subscriber**

**Terminal 2 - Publish while subscriber offline:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic test/offline --message "Offline message 1" --qos 1 --non-interactive
mqttv5 pub --url tcp://localhost:1883 --topic test/offline --message "Offline message 2" --qos 1 --non-interactive
mqttv5 pub --url tcp://localhost:1883 --topic test/offline --message "Offline message 3" --qos 1 --non-interactive
```

**Terminal 1 - Reconnect with same client ID:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic test/offline --qos 1 --client-id test-client-123 --no-clean-start --session-expiry 300 --count 3 --non-interactive
```

Expected: Receives all 3 offline messages immediately upon reconnection

**Important Notes:**

- The `--qos` flag MUST match the QoS used when publishing (use QoS 1 or 2 for persistent sessions)
- QoS 0 messages are NOT queued by the broker and will be lost when offline
- Both subscriber and publisher must use the same QoS level for message persistence to work
- The broker queues messages server-side; when you reconnect and subscribe, queued messages are delivered

---

## Will Messages (Last Will and Testament)

**Terminal 1 - Monitor will topic:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic status/client --non-interactive
```

**Terminal 2 - Connect with will message (keep alive for testing):**

```bash
mqttv5 pub \
  --url tcp://localhost:1883 \
  --topic dummy/topic \
  --message "test" \
  --will-topic status/client \
  --will-message "client disconnected unexpectedly" \
  --will-qos 1 \
  --will-retain \
  --keep-alive-after-publish \
  --non-interactive
```

**Kill Terminal 2 process abruptly (Ctrl+C or kill)**

Expected: Terminal 1 receives "client disconnected unexpectedly"

---

## Authentication Tests

### Username/Password Authentication

**Terminal 1:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic test/auth --username testuser --password testpass --non-interactive
```

**Terminal 2:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic test/auth --message "Authenticated message" --username testuser --password testpass --non-interactive
```

Note: Requires Mosquitto configured with password file

---

## Cross-Transport Tests

### Publish TCP, Subscribe WebSocket

**Terminal 1:**

```bash
mqttv5 sub --url ws://localhost:9001 --topic test/cross --non-interactive
```

**Terminal 2:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic test/cross --message "Cross-transport test" --non-interactive
```

---

### Publish WebSocket, Subscribe TLS

**Terminal 1:**

```bash
mqttv5 sub --url ssl://localhost:8883 --topic test/cross2 --insecure --non-interactive
```

**Terminal 2:**

```bash
mqttv5 pub --url ws://localhost:9001 --topic test/cross2 --message "WS to TLS" --non-interactive
```

---

## Multiple Subscribers (Same Topic)

**Terminal 1:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic broadcast/news --client-id sub1 --verbose --non-interactive
```

**Open Terminal 3:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic broadcast/news --client-id sub2 --verbose --non-interactive
```

**Terminal 2:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic broadcast/news --message "Breaking news!" --non-interactive
```

Expected: Both subscribers receive the message

---

## Message Count Test

**Terminal 1 - Receive exactly 5 messages then exit:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic test/count --count 5 --non-interactive
```

**Terminal 2 - Send 10 messages:**

```bash
for i in {1..10}; do
  mqttv5 pub --url tcp://localhost:1883 --topic test/count --message "Message $i" --non-interactive
  sleep 0.5
done
```

Expected: Subscriber exits after 5 messages

---

## Stress Tests

### High-Frequency Publishing

**Terminal 1:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic stress/test --verbose --non-interactive
```

**Terminal 2:**

```bash
for i in {1..100}; do
  mqttv5 pub --url tcp://localhost:1883 --topic stress/test --message "Stress message $i" --qos 1 --non-interactive
done
```

---

### Large Payload

**Terminal 1:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic test/large --non-interactive
```

**Terminal 2:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic test/large --message "$(python3 -c 'print("A" * 10000)')" --non-interactive
```

---

## Keep-Alive Tests

**Terminal 1 - Long-running subscriber:**

```bash
mqttv5 sub --url tcp://localhost:1883 --topic test/keepalive --keep-alive 10 --non-interactive
```

**Wait 30+ seconds, then Terminal 2:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic test/keepalive --message "Still connected?" --non-interactive
```

Expected: Message received (connection kept alive)

---

## All Transports Same Message Test

**Start 4 subscribers on different transports:**

```bash
# Terminal 1 - TCP
mqttv5 sub --url tcp://localhost:1883 --topic all/transports --client-id tcp-sub --verbose --non-interactive

# Terminal 2 - TLS
mqttv5 sub --url ssl://localhost:8883 --topic all/transports --client-id tls-sub --insecure --verbose --non-interactive

# Terminal 3 - WebSocket
mqttv5 sub --url ws://localhost:9001 --topic all/transports --client-id ws-sub --verbose --non-interactive

# Terminal 4 - WebSocket TLS
mqttv5 sub --url wss://localhost:9443 --topic all/transports --client-id wss-sub --insecure --verbose --non-interactive
```

**Terminal 5 - Publish:**

```bash
mqttv5 pub --url tcp://localhost:1883 --topic all/transports --message "Message to all transports" --non-interactive
```

Expected: All 4 subscribers receive the message

---

## Interactive Mode Tests

### Interactive Publish (omit required args)

```bash
mqttv5 pub
```

Follow prompts to enter topic and message

---

### Interactive Subscribe

```bash
mqttv5 sub
```

Follow prompts to enter topic and connection details

---

## Error Handling Tests

### Invalid QoS

```bash
mqttv5 pub --url tcp://localhost:1883 --topic test --message "test" --qos 3 --non-interactive
```

Expected: Error message about invalid QoS

---

### Connection Refused

```bash
mqttv5 pub --url tcp://localhost:9999 --topic test --message "test" --non-interactive
```

Expected: Connection error

---

### Invalid URL Scheme

```bash
mqttv5 pub --url http://localhost:1883 --topic test --message "test" --non-interactive
```

Expected: Error about unsupported URL scheme

---

## Notes

- Use `--verbose` on subscribe to see topic names with messages
- Use `--count N` on subscribe to automatically exit after N messages
- Use `--client-id` to set specific client identifiers
- Use `--insecure` for self-signed certificates (testing only)
- Press `Ctrl+C` to gracefully disconnect subscribers
- All commands support `--help` for full options list

## Quick Test Checklist

- [x] TCP basic pub/sub
- [x] TLS with insecure flag
- [x] WebSocket basic
- [x] WebSocket TLS with insecure
- [x] QoS 0, 1, 2 on each transport
- [x] QoS 2 specifically on WebSocket (regression test)
- [x] Retained messages
- [x] Wildcard subscriptions (+, #)
- [x] Session persistence (offline messages)
- [x] Will messages
- [x] Cross-transport communication
- [x] Multiple subscribers same topic
- [x] Large payloads
- [x] High-frequency messages
