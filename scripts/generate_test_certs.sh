#!/bin/bash

# Script to generate test certificates for MQTT TLS testing
# These certificates are for testing purposes only

set -e

# Configuration
CERT_DIR="test_certs"
DAYS_VALID=365
COUNTRY="US"
STATE="Test"
CITY="Test"
ORG="Test"
CN_CA="Test CA"
CN_CLIENT="Test Client"

# Create directory if it doesn't exist
mkdir -p "$CERT_DIR"

echo "Generating test certificates in $CERT_DIR..."

# Generate CA private key
openssl genrsa -out "$CERT_DIR/ca.key" 2048

# Generate CA certificate
openssl req -new -x509 -days "$DAYS_VALID" -key "$CERT_DIR/ca.key" -out "$CERT_DIR/ca.pem" \
    -subj "/C=$COUNTRY/ST=$STATE/L=$CITY/O=$ORG/CN=$CN_CA"

# Generate client private key
openssl genrsa -out "$CERT_DIR/client.key" 2048

# Generate client certificate request
openssl req -new -key "$CERT_DIR/client.key" -out "$CERT_DIR/client.csr" \
    -subj "/C=$COUNTRY/ST=$STATE/L=$CITY/O=$ORG/CN=$CN_CLIENT"

# Sign client certificate with CA
openssl x509 -req -days "$DAYS_VALID" -in "$CERT_DIR/client.csr" \
    -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca.key" \
    -CAcreateserial -out "$CERT_DIR/client.pem"

# Clean up CSR file (optional)
rm -f "$CERT_DIR/client.csr"

echo "Test certificates generated successfully!"
echo "Files created:"
echo "  - $CERT_DIR/ca.key     (CA private key)"
echo "  - $CERT_DIR/ca.pem     (CA certificate)"
echo "  - $CERT_DIR/ca.srl     (CA serial number file)"
echo "  - $CERT_DIR/client.key (Client private key)"
echo "  - $CERT_DIR/client.pem (Client certificate)"
echo ""
echo "These certificates are for testing only and should NOT be used in production."