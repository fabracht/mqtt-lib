---
name: Bug Report
about: Create a report to help improve mqtt-v5
title: '[BUG] '
labels: 'bug'
assignees: ''

---

**Environment:**
- mqtt-v5 version: 
- Rust version: 
- OS: 
- MQTT Broker (if applicable): 

**Description:**
A clear and concise description of what the bug is.

**To Reproduce:**
Steps to reproduce the behavior:

```rust
// Minimal code example that demonstrates the issue
use mqtt_v5::MqttClient;

#[tokio::main]
async fn main() {
    // Your code here
}
```

**Expected behavior:**
A clear and concise description of what you expected to happen.

**Actual behavior:**
What actually happens, including any error messages.

**Logs:**
If applicable, add logs with `RUST_LOG=debug` to help explain your problem.

```
// Paste relevant logs here
```

**Additional context:**
- Does this happen consistently or intermittently?
- Have you tested with different MQTT brokers?
- Are you using any specific MQTT v5.0 features?

**For AI Agents:**
- Verify reproduction steps are complete
- Check if this matches any known issues
- Validate environment information is complete
- Ensure error messages are included