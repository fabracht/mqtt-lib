# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- **Comprehensive IoT examples**:
  - IoT device simulator with circuit breaker patterns
  - Smart home hub with concurrent device management
  - Industrial sensor network with failover logic
  - Observability dashboard with real-time metrics collection
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
Four comprehensive examples demonstrating real-world usage patterns:
1. **IoT Device Simulator** - Error handling, circuit breakers, retry logic
2. **Smart Home Hub** - Concurrent device management, mTLS authentication
3. **Industrial Sensor Network** - Enterprise security, custom CA, failover
4. **Observability Dashboard** - Metrics collection, HTTP endpoints, monitoring

## [Unreleased]

### Planned
- WebSocket transport support
- Enhanced authentication methods
- Additional IoT integration examples
- Performance optimizations

---

**Note**: This project was originally created as a showcase for the BeBytes derive macro capabilities,
demonstrating high-performance serialization in real-world MQTT applications. It has evolved into
a full-featured, production-ready MQTT v5.0 client library suitable for industrial IoT deployments.