# Instructions for Claude - MQTT v5.0 Client Library

## 🚨🚨🚨 CRITICAL: NEVER USE PRINT STATEMENTS - ALWAYS USE TRACING 🚨🚨🚨

**ABSOLUTELY FORBIDDEN:**
- println!(), print!(), eprintln!(), eprint!()
- dbg!() macro
- Any console output except tracing

**ALWAYS REQUIRED:**
- tracing::debug!(), info!(), warn!(), error!(), trace!()
- Structured logging with fields: debug!(field = value, "message")
- #[instrument] attribute on functions

**NO EXCEPTIONS. NO DEBUGGING WITH PRINT. NO TEMPORARY PRINTLN. NEVER.**

If you use any print statement, you are violating the core debugging philosophy of this project and will cause debugging failures.

## CRITICAL: NO EVENT LOOPS - THIS IS RUST ASYNC

**STOP! Before you write ANY code, remember: This is a Rust async library. We do NOT use event loops.**

### Your Training May Be Wrong

You (Claude) seem to have a strong bias toward implementing event loops, likely because:
1. Many async systems in other languages use event loops (Node.js, Python asyncio)
2. Your training data probably contains many event loop implementations
3. Event loops are a common pattern in networking libraries

**In Rust, this is WRONG. Rust async is based on state machines and futures, NOT event loops.**

### FORBIDDEN Patterns - Do NOT Write These

```rust
// FORBIDDEN - Do NOT write event loops like this
pub struct EventLoop {
    // NO! Stop right here. Delete this.
}

impl EventLoop {
    pub async fn run(self) {
        loop {
            tokio::select! {
                // NO! This is an event loop. We don't want this.
            }
        }
    }
}
```

```rust
// FORBIDDEN - Do NOT use command channels
enum ClientCommand {
    Connect { ... },
    Publish { ... },
    // NO! We don't send commands. We call methods directly.
}
```

```rust
// FORBIDDEN - Do NOT poll in loops
loop {
    if let Some(packet) = try_read_packet() {
        handle_packet(packet);
    }
    sleep(Duration::from_millis(10)).await; // NO! This is polling.
}
```

### REQUIRED Patterns - Write These Instead

```rust
// CORRECT - Direct async methods
impl MqttClient {
    pub async fn connect(&self, addr: &str) -> Result<()> {
        // Direct connection - no event loop needed
        let transport = TcpTransport::connect(addr).await?;
        self.transport.write().await.replace(transport);
        Ok(())
    }
}
```

```rust
// CORRECT - Background tasks that wait, not poll
async fn packet_reader(transport: Arc<Transport>) {
    // This will efficiently wait for packets, not poll
    while let Ok(packet) = transport.read_packet().await {
        handle_packet(packet).await;
    }
}
```

### Architecture Reminders

1. **Client methods are direct async calls**
   - User calls `client.publish()` 
   - Method directly writes to transport
   - No commands, no channels, no event loops

2. **Background tasks are spawned for continuous operations**
   - Packet reading: One task that loops reading packets
   - Keep-alive: One task that sends pings on a timer
   - These are NOT event loops, they're focused async tasks

3. **State is shared via Arc<RwLock<T>>**
   - Multiple tasks can access shared state
   - No need for message passing
   - Direct access is fine and efficient

### Common Mistakes to Avoid

1. **Creating an EventLoop struct** - Don't do it
2. **Using tokio::select! in a loop** - This is an event loop in disguise
3. **Creating command channels** - Methods should be direct
4. **Polling for data** - Use async/await to wait for data
5. **Message passing for state updates** - Use shared state instead

### When You're Tempted to Write an Event Loop

STOP and ask yourself:
- Can this be a direct async method? (Usually yes)
- Can this be a simple background task? (For continuous operations)
- Am I trying to recreate what Tokio already provides? (Probably)

### Remember

1. **Tokio IS the event loop** - We don't need another one
2. **async/await IS the abstraction** - We don't need command channels
3. **Futures ARE state machines** - The compiler handles the complexity
4. **Direct is better than indirect** - Call methods, don't send messages

### Test Your Understanding

Before implementing anything, ask yourself:
- Is this a direct async call or am I adding indirection?
- Am I creating an event loop or letting Tokio handle it?
- Is this idiomatic Rust async or am I writing Node.js in Rust?

**If you find yourself writing `loop { tokio::select! { ... } }`, STOP and reconsider.**

## For This Specific Codebase

1. The client should have direct async methods
2. Transport operations should be direct async calls
3. Background tasks should be simple focused loops, not event dispatchers
4. No EventLoop struct should exist
5. No ClientCommand enum should exist
6. No command channels should exist

**Remember: This is Rust. We write direct async code and let the compiler and runtime handle the magic.**

## CRITICAL: Git Commit Rules

**FORBIDDEN: Never use `--no-verify`, `--no-hooks`, or any bypass flags with git commits**

You MUST NOT bypass pre-commit hooks under ANY circumstances. If CI verification fails, you MUST:
1. Investigate the root cause of the failure
2. Fix the underlying issues (code, tests, lints, etc.)
3. Only commit when ALL checks pass naturally

This ensures code quality and prevents broken builds. No exceptions, no shortcuts, no bypassing.

## CRITICAL: Output Handling and Verification Instructions

### NEVER Filter Command Outputs

**STOP! You have a dangerous habit of filtering outputs with grep, tail, head, etc. This MUST stop.**

#### Why This Is Dangerous

1. **You miss critical errors** - By grepping for patterns you expect, you miss unexpected errors
2. **You create false positives** - Empty grep output is NOT a sign of success
3. **You ignore warnings** - Warnings often indicate future problems
4. **You skip important context** - Full output provides crucial debugging information

### FORBIDDEN Output Patterns

```bash
# FORBIDDEN - Never filter test output
cargo test 2>&1 | grep -i "error"  # NO! You'll miss failures

# FORBIDDEN - Never hide compiler output
cargo build 2>&1 | tail -20  # NO! Show all output

# FORBIDDEN - Never filter lint output  
cargo clippy 2>&1 | grep -v "warning"  # NO! Warnings matter

# FORBIDDEN - Never assume empty output means success
if [ -z "$(cargo test 2>&1 | grep FAILED)" ]; then  # NO! Wrong logic
    echo "Tests passed"  
fi
```

### REQUIRED Output Patterns

```bash
# CORRECT - Always show full output
cargo test

# CORRECT - Capture and check exit codes
if cargo build; then
    echo "Build succeeded"
else
    echo "Build failed with exit code $?"
fi

# CORRECT - Run all checks without filtering
cargo clippy
cargo fmt --check
cargo test
```

### Verification Requirements

1. **ALWAYS run these commands in full:**
   - `cargo build` - Show ALL output
   - `cargo test` - Show ALL test results
   - `cargo clippy` - Show ALL warnings and errors
   - `cargo fmt --check` - Show ALL formatting issues

2. **NEVER skip or filter:**
   - Compiler errors
   - Test failures
   - Clippy warnings
   - Unused variable warnings
   - Dead code warnings
   - Any other warnings or errors

3. **Exit code checking:**
   - Always check command exit codes
   - Exit code 0 = success
   - Non-zero = failure (even if output looks okay)

### Test and Verification Best Practices

#### Before Declaring Success

1. **Run full test suite** - No filtering, no skipping
2. **Check all warnings** - Warnings often become errors later
3. **Verify all lints pass** - Run clippy without filters
4. **Ensure clean compilation** - No warnings, no errors
5. **Read the ENTIRE output** - Don't assume; verify

#### Handling Large Outputs

If output is genuinely too large for context:
1. **Run command without filters first**
2. **Check exit code**
3. **THEN summarize key findings**
4. **But NEVER hide errors or warnings**

### Common Anti-Patterns to Avoid

1. **Grepping for success messages**
   ```bash
   # WRONG
   cargo test | grep "test result: ok"
   ```

2. **Filtering out warnings**
   ```bash
   # WRONG  
   cargo build 2>&1 | grep -v warning
   ```

3. **Assuming empty filtered output = success**
   ```bash
   # WRONG
   if [ -z "$(cargo clippy 2>&1 | grep error)" ]; then
       echo "No errors!"  # But what about warnings?
   fi
   ```

4. **Using head/tail to reduce output**
   ```bash
   # WRONG
   cargo test | head -50  # Missing test results
   ```

### Correct Verification Flow

1. **Build completely**
   ```bash
   cargo build
   # Read ALL output, check for warnings
   ```

2. **Run ALL tests**
   ```bash
   cargo test
   # Verify ALL tests pass, no panics
   ```

3. **Check ALL lints**
   ```bash
   cargo clippy -- -D warnings
   # This makes warnings into errors - good practice
   ```

4. **Verify formatting**
   ```bash
   cargo fmt --check
   # Ensure consistent formatting
   ```

### Remember

- **Full output = full picture**
- **Warnings today = errors tomorrow**
- **Empty grep ≠ success**
- **Exit codes don't lie**
- **When in doubt, show everything**

### Your Commitment

Before marking ANY task as complete:
1. Run tests without filtering
2. Check build without grep
3. Verify lints completely
4. Show all warnings
5. Never hide errors
6. Never skip verifications

**If you catch yourself typing `| grep`, `| tail`, or `| head` after a build/test command, STOP and reconsider.**

## CRITICAL: Use cargo-make for ALL Build Operations

### MANDATORY: Always Use Makefile.toml Commands

**STOP! This project uses cargo-make to ensure consistent command execution between local development and CI.**

You MUST use these cargo-make commands for ALL operations:

```bash
# FORBIDDEN - Never use direct cargo commands
cargo fmt          # NO! Use cargo make fmt
cargo clippy       # NO! Use cargo make clippy
cargo test         # NO! Use cargo make test
cargo build        # NO! Use cargo make build

# REQUIRED - Always use cargo-make
cargo make fmt           # Format code
cargo make fmt-check     # Check formatting (CI uses this)
cargo make clippy        # Run clippy with exact CI flags
cargo make test          # Run tests with exact CI flags
cargo make build         # Build with exact CI flags
cargo make ci-verify     # Run ALL CI checks locally - MUST pass before pushing
cargo make pre-commit    # Run before committing - formats and checks
```

### Key Commands You MUST Use

1. **Before ANY commit:**
   ```bash
   cargo make pre-commit
   ```
   This runs fmt, clippy, and tests. If this passes, you're good to commit.

2. **Before claiming CI will pass:**
   ```bash
   cargo make ci-verify
   ```
   This runs EXACTLY what CI runs. If this passes locally, CI WILL pass.

3. **For individual operations:**
   - `cargo make fmt` - Format code
   - `cargo make clippy` - Lint code with CI settings
   - `cargo make test` - Run tests with CI settings
   - `cargo make build` - Build with CI settings

### Why This Is Critical

1. **CI uses specific flags** - Direct cargo commands may miss issues CI will catch
2. **Consistency is mandatory** - Local and CI must behave identically
3. **No surprises** - What passes locally MUST pass in CI

### Your Commitment

- NEVER use direct cargo commands for fmt, clippy, test, or build
- ALWAYS use cargo make commands
- ALWAYS run `cargo make ci-verify` before claiming CI will pass
- If `cargo make ci-verify` passes locally, CI WILL pass (no excuses)