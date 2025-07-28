#!/bin/bash

# Script to run integration tests with Mosquitto broker

set -e

echo "Starting MQTT broker for integration tests..."

# Start Mosquitto using docker-compose
docker-compose up -d mosquitto

# Wait for broker to be ready
echo "Waiting for broker to be ready..."
sleep 2

# Check if broker is running
if ! docker-compose ps | grep -q "mosquitto.*Up"; then
    echo "Failed to start Mosquitto broker"
    exit 1
fi

echo "Broker is running. Starting integration tests..."

# Run integration tests
echo "Running complete flow tests..."
cargo test --test integration_complete_flow -- --nocapture

echo "Running reconnection tests..."
cargo test --test integration_reconnection -- --nocapture

echo "Running MQTT v5.0 feature tests..."
cargo test --test integration_mqtt5_features -- --nocapture

# Run all integration tests
echo "Running all integration tests..."
cargo test --tests -- --nocapture

# Stop the broker
echo "Stopping MQTT broker..."
docker-compose down

echo "Integration tests completed!"