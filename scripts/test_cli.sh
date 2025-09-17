#!/bin/bash
# Test script for CLI functionality

set -e

echo "=== MQTT CLI Test Suite ==="
echo

# Build the CLI
echo "Building CLI..."
cargo build --release -p mqttv5-cli

CLI="./target/release/mqttv5"

# Test 1: Help text
echo "Test 1: Checking help text..."
$CLI --help >/dev/null 2>&1 && echo "✅ Main help works"
$CLI pub --help >/dev/null 2>&1 && echo "✅ Pub help works"
$CLI sub --help >/dev/null 2>&1 && echo "✅ Sub help works"
$CLI broker --help >/dev/null 2>&1 && echo "✅ Broker help works"
echo

# Test 2: Verify new MQTT v5.0 features in help
echo "Test 2: Checking MQTT v5.0 features..."
$CLI pub --help | grep -q "no-clean-start" && echo "✅ no-clean-start option present"
$CLI pub --help | grep -q "session-expiry" && echo "✅ session-expiry option present"
$CLI pub --help | grep -q "keep-alive" && echo "✅ keep-alive option present"
$CLI pub --help | grep -q "username" && echo "✅ username option present"
$CLI pub --help | grep -q "password" && echo "✅ password option present"
$CLI pub --help | grep -q "will-topic" && echo "✅ will-topic option present"
$CLI pub --help | grep -q "will-message" && echo "✅ will-message option present"
$CLI pub --help | grep -q "will-qos" && echo "✅ will-qos option present"
$CLI pub --help | grep -q "will-retain" && echo "✅ will-retain option present"
$CLI pub --help | grep -q "will-delay" && echo "✅ will-delay option present"
echo

# Test 3: Verify transport support
echo "Test 3: Checking transport support..."
$CLI pub --help | grep -q "mqtt://" && echo "✅ TCP transport documented"
$CLI pub --help | grep -q "mqtt-udp://" && echo "✅ UDP transport documented"
$CLI pub --help | grep -q "mqtts-dtls://" && echo "✅ DTLS transport documented"
echo

# Test 4: Topic validation
echo "Test 4: Testing topic validation..."
if $CLI pub --topic "test//invalid" --message "test" --non-interactive 2>&1 | grep -q "cannot have empty segments"; then
    echo "✅ Topic validation works"
else
    echo "❌ Topic validation failed"
fi
echo

# Test 5: Test with non-existent broker (should fail gracefully)
echo "Test 5: Testing connection error handling..."
if $CLI pub --url mqtt://localhost:19999 --topic test --message test --non-interactive 2>&1 | grep -q "Failed to connect"; then
    echo "✅ Connection error handling works"
else
    echo "❌ Connection error handling failed"
fi
echo

# Test 6: Test parameter parsing
echo "Test 6: Testing parameter parsing..."
# This should parse but fail to connect
if $CLI pub \
    --url mqtt://localhost:19999 \
    --topic test/params \
    --message "param test" \
    --no-clean-start \
    --session-expiry 30 \
    --keep-alive 120 \
    --username testuser \
    --password testpass \
    --will-topic test/will \
    --will-message "offline" \
    --will-qos 1 \
    --will-retain \
    --will-delay 5 \
    --non-interactive 2>&1 | grep -q "Failed to connect"; then
    echo "✅ All parameters accepted"
else
    echo "❌ Parameter parsing failed"
fi
echo

# Test 7: File input
echo "Test 7: Testing file input..."
echo "Test message from file" > /tmp/mqtt_test.txt
if $CLI pub --url mqtt://localhost:19999 --topic test --file /tmp/mqtt_test.txt --non-interactive 2>&1 | grep -q "Failed to connect"; then
    echo "✅ File input accepted"
else
    echo "❌ File input failed"
fi
rm -f /tmp/mqtt_test.txt
echo

# Test 8: QoS validation
echo "Test 8: Testing QoS validation..."
for qos in 0 1 2; do
    if $CLI pub --url mqtt://localhost:19999 --topic test --message test --qos $qos --non-interactive 2>&1 | grep -q "Failed to connect"; then
        echo "✅ QoS $qos accepted"
    else
        echo "❌ QoS $qos failed"
    fi
done
echo

# Test 9: URL parsing
echo "Test 9: Testing URL parsing..."
for url in "mqtt://test:1883" "mqtts://test:8883" "mqtt-udp://test:1883" "mqtts-dtls://test:8883"; do
    if $CLI pub --url $url --topic test --message test --non-interactive 2>&1 | grep -E "(Failed to connect|Connection refused|failed to lookup)"; then
        echo "✅ URL $url parsed correctly"
    else
        echo "❌ URL $url parsing failed"
    fi
done
echo

echo "=== CLI Test Suite Complete ==="#]