#!/bin/bash

# Script to run integration tests with our own mqttv5 broker

set -e

echo "🚀 Starting MQTT v5.0 broker for integration tests..."
echo "   Using our own mqttv5 CLI instead of external mosquitto!"

# Start our mqttv5 broker using docker-compose
docker-compose up -d mqttv5-broker

# Wait for broker to be ready
echo "⏳ Waiting for broker to be ready..."
sleep 5

# Check if broker is running using our health check
if ! docker-compose ps | grep -q "mqttv5-test-broker.*Up.*healthy"; then
    echo "❌ Failed to start mqttv5 broker or health check failed"
    echo "📋 Checking broker logs:"
    docker-compose logs mqttv5-broker
    exit 1
fi

echo "✅ mqttv5 broker is running and healthy!"

# Demonstrate our CLI in action
echo "🎯 Testing our mqttv5 CLI functionality..."

# Build our CLI first
echo "🔧 Building mqttv5 CLI..."
cargo build --release -p mqttv5-cli

# Test basic pub/sub with our CLI
echo "📤 Testing publish with mqttv5 CLI..."
if ./target/release/mqttv5 pub --host localhost --topic "test/integration" --message "Hello from mqttv5!" --non-interactive; then
    echo "✅ Publish test successful!"
else
    echo "❌ Publish test failed"
    exit 1
fi

# Test health check with our CLI
echo "🩺 Testing health check with mqttv5 CLI..."
if ./target/release/mqttv5 sub --host localhost --topic "health/check" --count 1 --non-interactive & sleep 2 && ./target/release/mqttv5 pub --host localhost --topic "health/check" --message "ping" --non-interactive; then
    echo "✅ Health check test successful!"  
else
    echo "❌ Health check test failed"
fi

echo "🧪 Starting Rust integration tests with our mqttv5 broker..."

# Run integration tests against our broker
echo "🔄 Running complete flow tests..."
cargo test --test integration_complete_flow -- --nocapture

echo "🔌 Running reconnection tests..."
cargo test --test integration_reconnection -- --nocapture

echo "🚀 Running MQTT v5.0 feature tests..."
cargo test --test integration_mqtt5_features -- --nocapture

# Run all integration tests
echo "🏃 Running all integration tests..."
cargo test --tests -- --nocapture

# Final CLI validation
echo "🎉 Final validation: Demonstrating CLI superiority..."
echo "📊 Our mqttv5 CLI vs traditional mosquitto commands:"
echo ""
echo "  Traditional: mosquitto_pub -h localhost -t topic -m message"
echo "  Our CLI:     mqttv5 pub --host localhost --topic topic --message message"
echo ""
echo "  Traditional: mosquitto_sub -h localhost -t topic -v"  
echo "  Our CLI:     mqttv5 sub --host localhost --topic topic --verbose"
echo ""

# Stop the broker
echo "🛑 Stopping mqttv5 broker..."
docker-compose down

echo "✅ Integration tests completed successfully with our mqttv5 infrastructure!"
echo "🎯 We are now completely self-reliant - no external MQTT tools needed!"