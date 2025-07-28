# Test Certificates

This directory contains test certificates for MQTT TLS testing. The certificates are not included in the repository for security reasons.

## Generating Test Certificates

To generate the required test certificates, run the following script from the project root:

```bash
./scripts/generate_test_certs.sh
```

This will create the following files in this directory:
- `ca.key` - Certificate Authority private key
- `ca.pem` - Certificate Authority certificate
- `ca.srl` - Certificate Authority serial number file
- `client.key` - Client private key
- `client.pem` - Client certificate

## Important Notes

- These certificates are for **testing purposes only**
- Do NOT use these certificates in production
- Do NOT commit private keys to the repository
- The generated certificates are valid for 365 days

## Using the Certificates in Tests

The test suite expects these certificates to exist in the `test_certs/` directory. Make sure to generate them before running TLS-related tests.