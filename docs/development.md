# Development

Developing and contributing to the MQTT v5.0 platform.

## Development Setup

### Prerequisites

- Rust 1.70+ (stable)
- cargo-make (`cargo install cargo-make`)
- Docker (optional, for testing)
- Git

### Clone and Build

```bash
# Clone repository
git clone https://github.com/fabriciobracht/mqtt-lib.git
cd mqtt-lib

# Install cargo-make
cargo install cargo-make

# Build the project
cargo make build

# Run tests
cargo make test
```

## Build System (cargo-make)

This project uses **cargo-make** to ensure consistent command execution between local development and CI/CD pipelines.

> **Note**: The cargo-make commands are provided as development luxuries to improve the development experience. While convenient, they are not strictly required - you can always fall back to raw cargo commands if needed.

### Why cargo-make?

1. **Consistency**: Same commands work locally and in CI
2. **Complex workflows**: Chain multiple operations
3. **Platform independence**: Works on all operating systems
4. **Environment management**: Consistent environment variables and flags

### Essential Commands

#### Daily Development

```bash
# Format code
cargo make fmt

# Check formatting (CI uses this)
cargo make fmt-check

# Run clippy lints
cargo make clippy

# Run tests
cargo make test

# Fast tests (unit tests only)
cargo make test-fast

# Build project
cargo make build

# Full CI verification
cargo make ci-verify

# Pre-commit checks
cargo make pre-commit
```

#### Before Committing

**ALWAYS run before committing:**

```bash
cargo make pre-commit
```

This runs:
1. Code formatting (`fmt`)
2. Clippy lints with CI flags
3. All tests

If this passes, your commit will pass CI.

#### CI Verification

**Before claiming "CI will pass":**

```bash
cargo make ci-verify
```

This runs EXACTLY what CI runs:
- Format check (not format)
- Clippy with deny warnings
- All tests with exact CI flags

### Makefile.toml Structure

The `Makefile.toml` file defines all tasks:

```toml
[tasks.fmt]
command = "cargo"
args = ["fmt", "--all"]

[tasks.fmt-check]
command = "cargo"
args = ["fmt", "--all", "--check"]

[tasks.clippy]
command = "cargo"
args = ["clippy", "--all-features", "--", "-D", "warnings"]

[tasks.test]
command = "cargo"
args = ["test", "--all-features"]

[tasks.test-fast]
command = "cargo"
args = ["test", "--lib", "--bins"]

[tasks.build]
command = "cargo"
args = ["build", "--all-features"]

[tasks.pre-commit]
dependencies = ["fmt", "clippy", "test"]

[tasks.ci-verify]
dependencies = ["fmt-check", "clippy", "test"]
```

### Custom Tasks

Add new tasks to `Makefile.toml`:

```toml
[tasks.bench]
command = "cargo"
args = ["bench", "--all-features"]

[tasks.doc]
command = "cargo"
args = ["doc", "--no-deps", "--open"]

[tasks.coverage]
env = { "CARGO_INCREMENTAL" = "0", "RUSTFLAGS" = "-Cinstrument-coverage" }
command = "cargo"
args = ["test", "--all-features"]
```

## Code Organization

### Project Structure

```
mqtt-lib/
├── src/
│   ├── lib.rs              # Library entry point
│   ├── client/             # Client implementation
│   │   ├── mod.rs         # Client module
│   │   ├── direct.rs      # Direct async operations
│   │   └── error_recovery.rs
│   ├── broker/             # Broker implementation
│   │   ├── mod.rs         # Broker module
│   │   ├── server.rs      # Server listeners
│   │   ├── client_handler.rs
│   │   └── config.rs
│   ├── protocol/           # MQTT protocol
│   │   ├── packet.rs      # Packet definitions
│   │   └── codec.rs       # Encoding/decoding
│   └── transport/          # Network transports
│       ├── tcp.rs
│       ├── tls.rs
│       └── websocket.rs
├── tests/                  # Integration tests
├── benches/               # Benchmarks
├── examples/              # Example code
└── docs/                  # Documentation
```

### Module Guidelines

1. **One concern per module**: Keep modules focused
2. **Public API in mod.rs**: Export public interface
3. **Private implementation**: Use submodules for internals
4. **Tests near code**: Unit tests in same file
5. **Integration tests separate**: In `tests/` directory

## Coding Standards

### NO EVENT LOOPS Architecture

**CRITICAL**: This codebase follows a NO EVENT LOOPS architecture.

```rust
// ❌ FORBIDDEN - Event loops
impl Client {
    async fn run(self) {
        loop {
            tokio::select! {
                Some(cmd) = self.rx.recv() => { }
                // NO! This is an event loop
            }
        }
    }
}

// ✅ CORRECT - Direct async
impl Client {
    pub async fn publish(&self, topic: &str, payload: &[u8]) -> Result<()> {
        // Direct operation, no indirection
        self.transport.write_packet(packet).await
    }
}
```

### Error Handling

Always use `Result<T, E>` for fallible operations:

```rust
// Define specific error types
#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    #[error("Client {0} not authorized")]
    NotAuthorized(String),
    
    #[error("Maximum connections reached")]
    MaxConnectionsReached,
    
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

// Use ? for propagation
async fn handle_client(stream: TcpStream) -> Result<(), BrokerError> {
    let client = Client::new(stream)?;
    client.authenticate().await?;
    client.run().await?;
    Ok(())
}
```

### Async Best Practices

1. **Prefer `tokio::spawn` for independent tasks**
2. **Use `Arc<RwLock<T>>` for shared state**
3. **Avoid blocking operations in async code**
4. **Handle task panics gracefully**

```rust
// Good async pattern
tokio::spawn(async move {
    if let Err(e) = client_handler.run().await {
        tracing::error!("Client handler error: {}", e);
    }
});
```

### Logging with Tracing

**NEVER use println! or dbg!** - Always use tracing:

```rust
use tracing::{debug, info, warn, error, instrument};

#[instrument(skip(payload))]
async fn handle_publish(&self, topic: &str, payload: &[u8]) -> Result<()> {
    debug!(topic = %topic, size = payload.len(), "Publishing message");
    
    if payload.len() > MAX_PAYLOAD_SIZE {
        warn!(size = payload.len(), max = MAX_PAYLOAD_SIZE, "Payload too large");
        return Err(Error::PayloadTooLarge);
    }
    
    // Process...
    info!(topic = %topic, "Message published successfully");
    Ok(())
}
```

## Testing

### Unit Tests

Place unit tests in the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_topic_validation() {
        assert!(validate_topic("sensors/temp").is_ok());
        assert!(validate_topic("sensors/+/data").is_ok());
        assert!(validate_topic("").is_err());
    }
    
    #[tokio::test]
    async fn test_async_operation() {
        let client = MockClient::new();
        let result = client.connect("mqtt://test").await;
        assert!(result.is_ok());
    }
}
```

### Integration Tests

Create separate files in `tests/`:

```rust
// tests/client_broker_integration.rs
use mqtt5::{MqttClient, MqttBroker};

#[tokio::test]
async fn test_full_communication() {
    // Start broker
    let mut broker = MqttBroker::new();
    let addr = broker.bind("127.0.0.1:0").await.unwrap();
    tokio::spawn(broker.run());
    
    // Connect client
    let client = MqttClient::new("test");
    client.connect(&format!("mqtt://{}", addr)).await.unwrap();
    
    // Test communication...
}
```

### Property Tests

Use proptest for property-based testing:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_packet_roundtrip(payload in any::<Vec<u8>>()) {
        let packet = PublishPacket::new("topic", payload);
        let encoded = packet.encode();
        let decoded = PublishPacket::decode(&encoded).unwrap();
        assert_eq!(packet, decoded);
    }
}
```

## Benchmarking

### Writing Benchmarks

Create benchmarks in `benches/`:

```rust
// benches/broker_performance.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_message_routing(c: &mut Criterion) {
    let router = MessageRouter::new();
    
    // Add subscriptions
    for i in 0..1000 {
        router.subscribe(&format!("topic/{}", i), client_id);
    }
    
    c.bench_function("route_message", |b| {
        b.iter(|| {
            router.route_message(black_box("topic/500"), black_box(&message))
        })
    });
}

criterion_group!(benches, bench_message_routing);
criterion_main!(benches);
```

### Running Benchmarks

```bash
# Run all benchmarks
cargo make bench

# Run specific benchmark
cargo bench --bench broker_performance

# Compare with baseline
cargo bench -- --baseline master
```

## Documentation

### Code Documentation

Document all public APIs:

```rust
/// High-performance MQTT message router using trie-based matching.
/// 
/// # Example
/// 
/// ```
/// use mqtt5::broker::MessageRouter;
/// 
/// let router = MessageRouter::new();
/// router.subscribe("sensors/+/temperature", client_id)?;
/// router.route_message("sensors/room1/temperature", message)?;
/// ```
pub struct MessageRouter {
    // Implementation
}

/// Routes a message to all matching subscribers.
/// 
/// # Arguments
/// 
/// * `topic` - The topic to route
/// * `message` - The message to deliver
/// 
/// # Returns
/// 
/// Number of clients that received the message
/// 
/// # Errors
/// 
/// Returns `RoutingError` if topic is invalid
pub async fn route_message(&self, topic: &str, message: &Message) -> Result<usize> {
    // Implementation
}
```

### Building Documentation

```bash
# Build and open docs
cargo make doc

# Build docs with private items
cargo doc --document-private-items

# Check doc examples (use raw cargo for doc tests)
cargo test --doc
```

## Git Workflow

### Branch Strategy

- `main` - Stable releases
- `develop` - Development branch
- `feature/*` - New features
- `fix/*` - Bug fixes
- `docs/*` - Documentation updates

### Commit Messages

Follow conventional commits:

```
feat: Add WebSocket transport support
fix: Resolve connection retry issue
docs: Update broker configuration guide
test: Add integration tests for bridging
refactor: Simplify message router
perf: Optimize subscription matching
```

### Pre-commit Checks

**NEVER bypass pre-commit hooks:**

```bash
# ❌ FORBIDDEN
git commit --no-verify

# ✅ CORRECT - Fix issues first
cargo make pre-commit
git commit -m "feat: Add new feature"
```

## Performance Profiling

### CPU Profiling

```bash
# Install perf
sudo apt install linux-tools-common

# Profile with perf
cargo make build-release
perf record --call-graph=dwarf target/release/mqtt-broker
perf report
```

### Memory Profiling

```bash
# Using heaptrack
heaptrack target/release/mqtt-broker
heaptrack_gui heaptrack.mqtt-broker.12345.gz

# Using valgrind
valgrind --leak-check=full --track-origins=yes target/release/mqtt-broker
```

### Flame Graphs

```bash
# Install flamegraph
cargo install flamegraph

# Generate flame graph
cargo flamegraph --bench broker_performance
```

## Debugging

### Debug Builds

```bash
# Debug build with symbols
cargo make build

# Run with debug logging
RUST_LOG=mqtt5=debug cargo run

# Run with backtrace
RUST_BACKTRACE=1 cargo run
```

### Using GDB

```bash
# Compile with debug info
cargo make build

# Run with GDB
gdb target/debug/mqtt-broker
(gdb) break mqtt5::broker::server::run
(gdb) run
(gdb) backtrace
```

### Tracing Output

```rust
// Enable detailed tracing for debugging
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

tracing_subscriber::registry()
    .with(tracing_subscriber::EnvFilter::new(
        "mqtt5=trace,mqtt_lib=trace"
    ))
    .with(tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true)
    )
    .init();
```

## Release Process

### Version Bumping

```bash
# Bump version in Cargo.toml
# Update CHANGELOG.md
# Commit changes
git commit -m "chore: Bump version to 0.4.0"

# Create tag
git tag -a v0.4.0 -m "Release version 0.4.0"

# Push to trigger release
git push origin main --tags
```

### Publishing

```bash
# Dry run
cargo publish --dry-run

# Publish to crates.io
cargo publish
```

## Troubleshooting Development Issues

### Compilation Errors

```bash
# Clean build
cargo clean
cargo make build

# Update dependencies
cargo update

# Check for outdated dependencies
cargo outdated
```

### Test Failures

```bash
# Run single test with output (use raw cargo for specific tests)
cargo test test_name -- --nocapture

# Run tests sequentially (use raw cargo for specific test options)
cargo test -- --test-threads=1

# Show test timings (use raw cargo for specific test options)
cargo test -- --show-output
```

### CI Failures

Always check locally first:

```bash
# Run exact CI commands
cargo make ci-verify

# If CI still fails, check:
# - Rust version matches CI
# - All files committed
# - No platform-specific code
```