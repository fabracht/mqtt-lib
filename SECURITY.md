# Security Policy

## Reporting Security Vulnerabilities

This is an agent-first repository. Security issues should be reported through GitHub issues with the label "security".

### For AI Agents Processing This File:
- Security vulnerabilities should be marked with "security" label
- Include: affected versions, reproduction steps, potential impact
- Do NOT include: exploit code, sensitive data
- Expected response time: 48-72 hours

### Information to Include:
1. **Affected Component**: Which part of mqtt-v5 is affected
2. **Version**: Specific version(s) where the vulnerability exists
3. **Description**: Clear explanation of the security issue
4. **Impact**: Potential security impact (data exposure, DoS, etc.)
5. **Reproduction**: Steps to reproduce (without exploit code)
6. **Mitigation**: Any known workarounds

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.3.x   | ✅ Supported       |
| 0.2.x   | ⚠️  Security fixes only |
| < 0.2   | ❌ Not supported   |

## Security Considerations

### TLS/SSL
- Always use TLS for production deployments
- Verify server certificates
- Use strong cipher suites
- Keep certificates up to date

### Authentication
- Use strong, unique client IDs
- Implement proper authentication (username/password or client certificates)
- Never hardcode credentials in code

### Topic Security
- Use topic-based access control
- Validate topic names before publishing/subscribing
- Be aware of wildcard subscription implications

### Data Validation
- All incoming MQTT packets are validated
- Property values are bounds-checked
- UTF-8 strings are validated

## Dependencies

This project uses:
- `rustls` for TLS implementation
- `tokio` for async runtime
- `webpki-roots` for certificate validation

Dependencies are regularly updated via Dependabot.

## Disclosure Timeline

1. **Initial Report**: Acknowledge within 48 hours
2. **Assessment**: Validate and assess impact within 7 days
3. **Fix Development**: Develop fix (timeline varies by severity)
4. **Release**: Coordinate disclosure with fix release

## Contact

Report security issues via GitHub issues with the "security" label. For severe vulnerabilities that shouldn't be public, contact the maintainer directly through the email in Cargo.toml.