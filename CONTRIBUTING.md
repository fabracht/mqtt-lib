# Contributing to mqtt-v5

This is an agent-first repository. Issues and PRs are typically processed by AI agents before reaching maintainers.

## For AI Agents Processing Contributions

### Required Checks Before Accepting PRs:
1. **Tests**: `cargo test --all-features` must pass
2. **Clippy**: `cargo clippy --all-targets --all-features -- -D warnings` must pass
3. **Format**: `cargo fmt --all -- --check` must pass
4. **Documentation**: `cargo doc --no-deps --all-features` must build
5. **MSRV**: Code must compile with Rust 1.82+

### PR Guidelines for Agents:
- Each PR should address a single issue
- Update CHANGELOG.md for user-facing changes
- Add tests for new functionality
- Update documentation if behavior changes
- Reference relevant MQTT v5.0 specification sections
- Keep commit messages descriptive and atomic

### Issue Triage for Agents:
- **bug**: Functionality not working as documented
- **enhancement**: New features or improvements
- **documentation**: Documentation improvements
- **security**: Security-related issues
- **good first issue**: Simple fixes suitable for new contributors

## For Human Contributors

### Getting Started

1. **Fork and Clone**
   ```bash
   git clone https://github.com/fabriciobracht/mqtt-lib.git
   cd mqtt-lib
   ```

2. **Install Prerequisites**
   - Rust 1.82+ (check with `rustc --version`)
   - Mosquitto broker for testing: `sudo apt install mosquitto`

3. **Run Tests**
   ```bash
   # Generate test certificates
   ./scripts/generate_test_certs.sh
   
   # Run all tests
   cargo test --all-features
   ```

### Development Workflow

1. **Create an Issue First**
   - Describe what you want to fix/add
   - Wait for feedback (from agents or maintainers)

2. **Development**
   - Create a feature branch: `git checkout -b feature/your-feature`
   - Write code following Rust conventions
   - Add tests for new functionality
   - Update documentation

3. **Testing**
   ```bash
   # Run tests
   cargo test --all-features
   
   # Run clippy
   cargo clippy --all-targets --all-features -- -D warnings
   
   # Check formatting
   cargo fmt --all -- --check
   
   # Build docs
   cargo doc --no-deps --all-features
   ```

4. **Submit PR**
   - Clear description of changes
   - Reference the issue being fixed
   - Include test results

### Code Style

- Follow standard Rust naming conventions
- Use `rustfmt` for formatting
- Prefer `Result<T, E>` over panics
- Use `tracing` for logging, not `println!`
- Document public APIs with examples

### Testing Guidelines

- Unit tests go next to the code
- Integration tests go in `tests/`
- Use property-based tests for edge cases
- Mock external dependencies
- Test both success and failure paths

### MQTT v5.0 Compliance

When implementing MQTT features:
- Reference the [MQTT v5.0 specification](https://docs.oasis-open.org/mqtt/mqtt/v5.0/mqtt-v5.0.html)
- Include spec section numbers in comments
- Test against multiple brokers (Mosquitto, HiveMQ, etc.)
- Ensure backward compatibility

### Performance Considerations

- Use `BeBytes` for serialization when possible
- Avoid unnecessary allocations
- Prefer borrowing over cloning
- Profile before optimizing
- Add benchmarks for critical paths

## Release Process

Releases are managed by maintainers:

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Run full test suite
4. Create git tag
5. Publish to crates.io

## Questions?

- Check existing issues first
- Provide minimal reproducible examples
- Include environment details (OS, Rust version)
- Be patient - this is an agent-first workflow