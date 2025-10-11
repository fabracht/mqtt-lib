# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Enhanced Developer Experience** - Improved cargo-make workflow
  - Added comprehensive `cargo make help` command with organized menu
  - Categorized commands: Build, Test, Code Quality, CI/CD, Documentation, Security
  - Clear descriptions and emoji icons for better discoverability
- **Comprehensive CLI Testing Suite** - Full test coverage for CLI functionality
  - End-to-end tests verifying actual MQTT message delivery
  - Feature tests covering all MQTT v5.0 CLI parameters
  - Transport tests for TCP, TLS, and WebSocket support
  - Performance and throughput testing with real broker integration
- **Enhanced CLI MQTT v5.0 Support** - Complete feature parity with library
  - Session management options (--no-clean-start, --session-expiry)
  - Will message support for both pub and sub commands
  - Enhanced authentication options (--username, --password)
  - Custom keep-alive intervals and connection options

### Changed
- **Dependency Updates** - Updated to latest stable versions
  - `tokio-tungstenite` updated from 0.27 to 0.28
  - `dialoguer` updated from 0.11 to 0.12


### Fixed
- **MaximumQoS Property Handling (Issue #14)** - Fixed MQTT v5.0 spec compliance
  - Broker now correctly omits MaximumQoS property when QoS 2 is supported (per MQTT v5.0 spec 3.2.2.3.4)
  - MaximumQoS property value MUST be 0 or 1, never 2
  - Client now reads and enforces server's MaximumQoS limit
  - Fixes compatibility with other MQTT v5.0 clients
- **Will Message Testing** - Fixed CLI will message tests that were timing out
  - Added hidden --keep-alive-after-publish flag for proper will message testing
  - Will messages now properly trigger when CLI process is terminated
  - Test infrastructure validates actual will message delivery
- **Repository Cleanup** - Improved .gitignore configuration
  - Removed CLI_TESTING_GUIDE.md from version control
  - File remains available locally but is now properly ignored

## [0.4.1] - 2025-08-05

### Fixed
- **Reduced CLI verbosity** - Changed default log level from WARN to ERROR
- **Fixed logging levels** - Normal operations no longer logged as errors/warnings
  - Task lifecycle messages (starting/exiting) changed from error/warn to debug
  - Connection and DNS resolution messages changed from warn to debug
  - Reconnection monitoring messages changed from warn to info
  - Server disconnect changed from error to info

## [0.4.0] - 2025-08-04

### Added
- **Unified mqttv5 CLI Tool** - Complete MQTT CLI implementation
  - Single binary with pub, sub, and broker subcommands
  - Superior user experience with smart prompting for missing arguments
  - Input validation with helpful error messages and corrections
  - Both long and short flags for improved ergonomics
  - Complete self-reliance - no external MQTT tools needed
- **Complete MQTT v5.0 Broker Implementation**
  - Production-ready broker with full MQTT v5.0 compliance
  - Multi-transport support: TCP, TLS, WebSocket in single binary
  - Built-in authentication: Username/password, file-based, bcrypt
  - Access Control Lists (ACL) for fine-grained topic permissions
  - Broker-to-broker bridging with loop prevention
  - Resource monitoring with connection limits and rate limiting
  - Session persistence and retained message storage
  - Shared subscriptions for load balancing
  - Hot configuration reload without restart
- **Advanced Connection Retry System**
  - Smart error classification distinguishing recoverable from non-recoverable errors
  - AWS IoT-specific error handling (RST, connection limit detection)
  - Exponential backoff with configurable retry policies
  - Different retry strategies for different error types

### Changed
- **Platform Transformation**: Project evolved from client library to complete MQTT v5.0 platform
- **Unified CLI**: All documentation and examples now use mqttv5 CLI
- **Comprehensive Documentation Overhaul**:
  - Restructured docs/ with separate client/ and broker/ sections
  - Added complete broker configuration reference
  - Added authentication and security guides
  - Added deployment and monitoring documentation
  - Updated all examples to show dual-platform usage
- **Development Workflow**: Standardized on cargo-make for consistent CI/build commands
- **Architecture**: Maintained NO EVENT LOOPS principle throughout broker implementation

### Removed
- Unimplemented AuthMethod::External references from documentation

## [0.2.0] - 2025-07-30

### Added
- **Complete MQTT v5.0 protocol implementation** with full compliance
- **BeBytes 2.6.0 integration** for high-performance zero-copy serialization
- **Comprehensive async/await API** with no event loops (pure Rust async patterns)
- **Advanced security features**:
  - TLS/SSL support with certificate validation
  - Mutual TLS (mTLS) authentication support
  - Custom CA certificate support for enterprise environments
- **Production-ready connection management**:
  - Automatic reconnection with exponential backoff
  - Session persistence (clean_start=false support)
  - Client-side message queuing for offline scenarios
  - Flow control respecting broker receive maximum limits
- **Focused library examples**:
  - Simple client and broker examples demonstrating core API
  - Transport examples (TCP, TLS, WebSocket) showing configuration patterns
  - Shared subscription and bridging examples for advanced features
- **Testing infrastructure**:
  - Mock client trait for unit testing
  - Property-based testing with Proptest
  - Integration tests with real MQTT broker
  - Comprehensive benchmark suite
- **Advanced tracing and debugging**:
  - Structured logging with tracing integration
  - Comprehensive instrumentation throughout the codebase
  - Performance monitoring capabilities
- **Developer experience**:
  - AWS IoT SDK compatible API (subscribe returns packet_id + QoS)
  - Callback-based message routing
  - Zero-configuration for common use cases
  - Extensive documentation and examples

### Technical Highlights
- **Zero-copy message handling** using BeBytes derive macros
- **Direct async methods** instead of event loops or actor patterns
- **Comprehensive error handling** with proper error types
- **Thread-safe design** with Arc/RwLock patterns
- **Memory efficient** with bounded queues and cleanup tasks
- **Production tested** with extensive integration test suite

### Performance
- High-throughput message processing with BeBytes serialization
- Efficient memory usage with zero-copy patterns
- Concurrent connection handling
- Optimized packet parsing and generation

### Dependencies
- `bebytes ^2.6.0` - Core serialization framework
- `tokio ^1.46` - Async runtime
- `rustls ^0.23` - TLS implementation
- `bytes ^1.10` - Efficient byte handling
- `thiserror ^2.0` - Error handling
- `tracing ^0.1` - Structured logging

### Examples
Eight focused examples demonstrating library capabilities:
1. **simple_client** - Basic client API usage with callbacks and publishing
2. **simple_broker** - Minimal broker setup and configuration
3. **broker_with_tls** - Secure TLS/SSL transport configuration
4. **broker_with_websocket** - WebSocket transport for browser clients
5. **broker_all_transports** - Multi-transport broker (TCP/TLS/WebSocket)
6. **broker_bridge_demo** - Broker-to-broker bridging configuration
7. **broker_with_monitoring** - $SYS topics and resource monitoring
8. **shared_subscription_demo** - Load balancing with shared subscriptions

## [0.3.0] - 2025-08-01

### Added
- **Certificate loading from bytes**: Load TLS certificates from memory (PEM/DER formats)
  - `load_client_cert_pem_bytes()` - Load client certificates from PEM byte arrays
  - `load_client_key_pem_bytes()` - Load private keys from PEM byte arrays  
  - `load_ca_cert_pem_bytes()` - Load CA certificates from PEM byte arrays
  - `load_client_cert_der_bytes()` - Load client certificates from DER byte arrays
  - `load_client_key_der_bytes()` - Load private keys from DER byte arrays
  - `load_ca_cert_der_bytes()` - Load CA certificates from DER byte arrays
- **WebSocket transport support**: Full MQTT over WebSocket implementation
  - WebSocket (ws://) and secure WebSocket (wss://) URL support
  - TLS integration for secure WebSocket connections
  - Custom headers and subprotocol negotiation
  - Client certificate authentication over WebSocket
  - Comprehensive configuration options
- **Property-based testing**: Comprehensive test coverage with Proptest
  - 29 new property-based tests covering edge cases and failure modes
  - Certificate loading robustness testing with arbitrary inputs
  - WebSocket configuration validation across all input domains
  - Memory safety verification for all certificate operations
- **AWS IoT namespace validator**: Topic validation for AWS IoT Core
  - Enforces AWS IoT topic restrictions and length limits (256 chars)
  - Device-specific topic isolation
  - Reserved topic protection
- **Supply chain security**: Enhanced security measures
  - GPG commit signing setup
  - Dependabot configuration
  - Security policy documentation

### Fixed
- Topic validation now correctly follows MQTT v5.0 specification
- AWS IoT namespace uses correct "things" (plural) path
- Subscription management handles duplicate topics correctly (replacement behavior)
- All clippy warnings resolved (65+ uninlined format strings)

### Enhanced
- TLS configuration now supports loading certificates from memory for cloud deployments
- WebSocket configuration supports all TLS features (client auth, custom CA, etc.)
- Comprehensive examples showing certificate loading patterns for different deployment scenarios
- CI pipeline optimized and all tests passing

### Use Cases Enabled
- **Cloud deployments**: Load certificates from Kubernetes secrets, environment variables
- **Browser applications**: MQTT over WebSocket for web-based IoT dashboards
- **Firewall-restricted environments**: WebSocket transport bypasses TCP restrictions
- **Secret management integration**: Load certificates from Vault, AWS Secrets Manager, etc.

---

**Note**: This project was originally created as a showcase for the BeBytes derive macro capabilities,
demonstrating serialization in real-world MQTT applications. It has evolved into
a full-featured MQTT v5.0 platform with both client and broker implementations,
complete with a unified CLI tool.