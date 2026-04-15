# ACP (Agent Client Protocol) Integration Implementation Plan

> **Plan mode:** `full`
> **For agentic workers:**
>
> - **ticket-to-pr pipeline:** hand this plan to the `coordinator` agent; it delegates execution to backend, frontend, and test specialists.
> - **Interactive execution:** Use `subagent-driven-development` (recommended) or execute the plan inline in the current session. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add ACP (Agent Client Protocol) support to fabro so that workflows can delegate work to any ACP-compatible coding agent (GitHub Copilot, Claude, Codex, Gemini, etc.) via a standardized JSON-RPC protocol, enabling multi-agent orchestration alongside existing LLM API providers.

**Plan Depth Rationale:** This is a System Track change — it introduces a new dependency (the `agent-client-protocol` Rust crate), a new protocol (ACP), a new crate (`fabro-acp`), cross-cutting integration points (workflow engine, agent crate, types, settings), and API contract changes (new handler type, new settings fields).

## Planning Controls

Track: System Track
Implementation Readiness: PASS
Track Rationale: New protocol, new crate, new dependency, cross-cutting integration points — requires full plan depth.
Readiness Rationale: All files, schemas, and integration points are identified. The official Rust SDK (`agent-client-protocol` v0.10.4) exists on crates.io. No unknowns remain.

**Architecture:** Create a new `fabro-acp` crate that wraps the official `agent-client-protocol` Rust SDK to manage ACP agent connections (spawn, initialize, session lifecycle, prompt turns). This mirrors the `fabro-mcp` pattern exactly — a `AcpClient` manages a single agent subprocess, an `AcpConnectionManager` aggregates multiple agents, and `make_acp_tools()` bridges ACP agents into the fabro-agent tool registry. Workflow nodes can use ACP agents via a new `acp_agent` handler or by invoking ACP tools from an `agent` node.

**Tech Stack:** Rust, `agent-client-protocol` crate v0.10.4, `tokio`, `serde`, `serde_json`, `tracing`, existing fabro workspace crates.

---

## Key Protocol Details (ACP)

ACP is JSON-RPC 2.0 over stdio (newline-delimited). The flow is:

1. **Initialize**: Client sends `initialize` with protocol version + client capabilities → Agent responds with capabilities + auth methods
2. **Session Setup**: `session/new` (with `cwd`, `mcpServers`) → Agent returns `sessionId`
3. **Prompt Turn**: `session/prompt` with content blocks → Agent sends `session/update` notifications (text, tool calls, diffs) → Turn ends with `StopReason`
4. **Teardown**: Drop connection, agent subprocess terminates

Agent can request file system access (`fs/read_text_file`, `fs/write_text_file`) and terminal access (`terminal/create`, etc.) — the client handles these. Agent can also request permissions (`session/request_permission`) for tool calls.

---

## Technical Constraints

1. **ACP Rust SDK maturity**: The `agent-client-protocol` crate is at v0.10.4 (pre-1.0). The API may change between minor versions. We pin to a specific version and update deliberately.
2. **Stdio transport only for v1**: The ACP spec defines stdio as the primary transport. HTTP/streamable transport is draft. We implement stdio only initially.
3. **Agent subprocess lifecycle**: ACP agents are spawned as child processes. We must handle crashes, timeouts, and graceful shutdown. The `agent-client-protocol` crate's `Client` trait handles transport; we add process management on top.
4. **Permission handling**: ACP agents may request permission for tool calls. In workflow context, we auto-approve (similar to how `--allow-all` works) since the workflow orchestrator decides what runs.
5. **Concurrent sessions**: Multiple ACP agents may run concurrently. Each gets its own subprocess. The `AcpConnectionManager` must manage these independently.

---

## File Map

### New crate: `lib/crates/fabro-acp/`

- Create: `lib/crates/fabro-acp/Cargo.toml`
- Create: `lib/crates/fabro-acp/src/lib.rs`
- Create: `lib/crates/fabro-acp/src/client.rs` — `AcpClient` (spawn agent, initialize, session lifecycle, prompt)
- Create: `lib/crates/fabro-acp/src/connection_manager.rs` — `AcpConnectionManager` (aggregate multiple agents)
- Create: `lib/crates/fabro-acp/src/config.rs` — Re-export ACP settings from `fabro-types`
- Create: `lib/crates/fabro-acp/src/protocol.rs` — Thin wrappers around `agent-client-protocol` types for fabro-internal use
- Create: `lib/crates/fabro-acp/src/error.rs` — ACP-specific error types
- Create: `lib/crates/fabro-acp/tests/stdio_integration.rs` — Integration test against a test ACP agent

### Modified: `lib/crates/fabro-types/`

- Modify: `lib/crates/fabro-types/src/settings/run.rs` — Add `AcpServerSettings`, `AcpTransport`, `AcpEntryLayer`, and `acps: HashMap<String, AcpServerSettings>` field to `RunAgentSettings`

### Modified: `lib/crates/fabro-agent/`

- Modify: `lib/crates/fabro-agent/src/mcp_integration.rs` — Add `make_acp_tools()` function mirroring `make_mcp_tools()`
- Modify: `lib/crates/fabro-agent/src/session.rs` — Initialize ACP connections during `Session::initialize()`, register ACP tools
- Modify: `lib/crates/fabro-agent/Cargo.toml` — Add `fabro-acp` dependency

### Modified: `lib/crates/fabro-workflow/`

- Modify: `lib/crates/fabro-workflow/src/handler/llm/api.rs` — Support ACP agent config in `AgentApiBackend::create_session_for()`
- Modify: `lib/crates/fabro-workflow/src/handler/mod.rs` — Register `acp_agent` handler type (or route via config)

### Modified: workspace root

- Modify: `lib/crates/Cargo.toml` — Add `fabro-acp` dependency entries

---

## Execution Slices

### Slice 1: Core ACP Client Crate

**Files:**
- Create: `lib/crates/fabro-acp/Cargo.toml`
- Create: `lib/crates/fabro-acp/src/lib.rs`
- Create: `lib/crates/fabro-acp/src/error.rs`
- Create: `lib/crates/fabro-acp/src/protocol.rs`
- Create: `lib/crates/fabro-acp/src/client.rs`
- Create: `lib/crates/fabro-acp/src/connection_manager.rs`
- Create: `lib/crates/fabro-acp/src/config.rs`
- Test: `lib/crates/fabro-acp/tests/stdio_integration.rs`

#### Task 1.1: Error types and protocol wrappers

- [ ] **Step 1: Write failing tests for AcpError**

Create `lib/crates/fabro-acp/src/lib.rs` with module declarations.
Create `lib/crates/fabro-acp/src/error.rs` with `AcpError` enum covering: `Connection`, `Initialization`, `Session`, `Prompt`, `Timeout`, `Process`, `Protocol`, `Agent`.
Create `lib/crates/fabro-acp/src/protocol.rs` with fabro-facing types: `AcpSessionId(String)`, `AcpStopReason` enum (`EndTurn`, `ToolUse`, `Cancelled`, `Error`), `AcpContentBlock` (mirroring ACP's content blocks: text, tool calls, diffs), `AcpAgentInfo` (name, version, capabilities).

```rust
// error.rs
#[derive(Debug, thiserror::Error)]
pub enum AcpError {
    #[error("ACP connection error: {0}")]
    Connection(String),
    #[error("ACP initialization failed: {0}")]
    Initialization(String),
    #[error("ACP session error: {0}")]
    Session(String),
    #[error("ACP prompt error: {0}")]
    Prompt(String),
    #[error("ACP timeout: {0}")]
    Timeout(String),
    #[error("ACP process error: {0}")]
    Process(String),
    #[error("ACP protocol error: {0}")]
    Protocol(String),
    #[error("ACP agent error: {0}")]
    Agent(String),
}
```

- [ ] **Step 2: Run tests to verify they compile and pass**

Run: `cargo nextest run -p fabro-acp` — Expect: PASS (these are type definitions, not runtime tests)

- [ ] **Step 3: Create Cargo.toml**

```toml
[package]
name = "fabro-acp"
version.workspace = true
edition.workspace = true

[dependencies]
agent-client-protocol = "0.10"
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tracing.workspace = true
fabro-types.workspace = true
thiserror.workspace = true
```

- [ ] **Step 4: Add to workspace**

Add `fabro-acp` to `lib/crates/Cargo.toml` workspace dependencies and members array.

- [ ] **Step 5: Verify crate compiles**

Run: `cargo build -p fabro-acp` — Expect: BUILD SUCCESS

- [ ] **Step 6: Commit**

```bash
git add lib/crates/fabro-acp/ lib/crates/Cargo.toml
git commit -m "feat(acp): scaffold fabro-acp crate with error types and protocol wrappers"
```

#### Task 1.2: ACP Client — spawn, initialize, and session lifecycle

- [ ] **Step 1: Write failing test for AcpClient initialization**

Create `lib/crates/fabro-acp/tests/stdio_integration.rs`. Write a test that spawns a Python ACP agent (similar to the MCP test pattern), initializes it via `AcpClient`, and verifies the `initialize` response contains agent capabilities.

```rust
#[tokio::test]
async fn acp_client_initialize() {
    let config = AcpServerSettings {
        name: "test_acp".into(),
        transport: AcpTransport::Stdio {
            command: vec!["python3".into(), "tests/test_acp_agent.py".into()],
            env: HashMap::new(),
        },
        startup_timeout_secs: 10,
        prompt_timeout_secs: 60,
    };
    let client = AcpClient::new(config).await.expect("failed to create client");
    let info = client.initialize(Duration::from_secs(10)).await.expect("failed to initialize");
    assert_eq!(info.agent_info.name, "test-acp-agent");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p fabro-acp --test stdio_integration` — Expect: FAIL (AcpClient not implemented yet)

- [ ] **Step 3: Create test ACP agent**

Create `lib/crates/fabro-acp/tests/test_acp_agent.py` — a minimal Python script implementing ACP's JSON-RPC over stdio, similar to `tests/test_mcp_server.py` but speaking ACP. It handles: `initialize`, `session/new`, `session/prompt`, `session/update`, `session/cancel`. Returns a simple text response.

```python
#!/usr/bin/env python3
"""Minimal ACP agent for integration testing."""
import sys
import json

def read_message():
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    return json.loads(line)

def write_message(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()

while True:
    msg = read_message()
    method = msg.get("method")
    id_ = msg.get("id")
    if method == "initialize":
        write_message({"jsonrpc": "2.0", "id": id_, "result": {
            "protocolVersion": 1,
            "agentCapabilities": {"loadSession": False, "promptCapabilities": {"image": False, "audio": False, "embeddedContext": False}, "mcpCapabilities": {"http": False, "sse": False}, "sessionCapabilities": {}},
            "agentInfo": {"name": "test-acp-agent", "version": "0.1.0"},
            "authMethods": []
        }})
    elif method == "session/new":
        write_message({"jsonrpc": "2.0", "id": id_, "result": {
            "sessionId": "test-session-1",
            "configOptions": None,
            "modes": None
        }})
    elif method == "session/prompt":
        # Send a session/update notification then complete
        write_message({"jsonrpc": "2.0", "method": "session/update", "params": {
            "sessionId": "test-session-1",
            "update": {"type": "content", "content": [{"type": "text", "text": "Hello from ACP agent"}]}
        }})
        write_message({"jsonrpc": "2.0", "id": id_, "result": {"stopReason": "end_turn"}})
    elif method == "authenticate":
        write_message({"jsonrpc": "2.0", "id": id_, "result": {}})
    else:
        write_message({"jsonrpc": "2.0", "id": id_, "error": {"code": -32601, "message": f"Unknown method: {method}"}})
```

- [ ] **Step 4: Implement AcpClient**

Create `lib/crates/fabro-acp/src/client.rs`. The `AcpClient` wraps the `agent-client-protocol` crate's `Client` trait implementation. Key design:

```rust
pub struct AcpClient {
    server_name: String,
    state: Mutex<ClientState>,
}

enum ClientState {
    Connecting,
    Ready(AcpSession),
}

struct AcpSession {
    agent_info: AcpAgentInfo,
    // Internal state for managing the connection
}
```

Methods:
- `new(config: AcpServerSettings) -> Result<Self>` — spawn agent subprocess, create transport
- `initialize(timeout: Duration) -> Result<AcpAgentInfo>` — send `initialize`, store capabilities
- `new_session(cwd: &str, mcp_servers: Vec<McpServerConfig>) -> Result<AcpSessionId>` — send `session/new`
- `prompt(session_id: &AcpSessionId, content: Vec<AcpContentBlock>) -> Result<AcpPromptResult>` — send `session/prompt`, collect `session/update` notifications, return final result
- `cancel(session_id: &AcpSessionId)` — send `session/cancel`
- `close()` — terminate subprocess, clean up

The `AcpClient` uses the `agent-client-protocol` crate's `Client` trait implementation. The crate provides `serve_client` or similar API to create a client connection over a transport. We spawn the agent process via `tokio::process::Command` and pipe stdin/stdout, similar to how `fabro-mcp` spawns MCP servers.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo nextest run -p fabro-acp` — Expect: PASS

- [ ] **Step 6: Write failing test for AcpConnectionManager**

```rust
#[tokio::test]
async fn connection_manager_acp_roundtrip() {
    let configs = vec![AcpServerSettings {
        name: "test_acp".into(),
        transport: AcpTransport::Stdio {
            command: vec!["python3".into(), "tests/test_acp_agent.py".into()],
            env: HashMap::new(),
        },
        startup_timeout_secs: 10,
        prompt_timeout_secs: 60,
    }];
    let manager = AcpConnectionManager::start_servers(configs).await.expect("failed to start");
    let agent_names = manager.agent_names();
    assert!(agent_names.contains(&"test_acp".to_string()));
}
```

- [ ] **Step 7: Run test to verify it fails**

Run: `cargo nextest run -p fabro-acp --test stdio_integration` — Expect: FAIL (AcpConnectionManager not implemented)

- [ ] **Step 8: Implement AcpConnectionManager**

Create `lib/crates/fabro-acp/src/connection_manager.rs`:

```rust
pub struct AcpConnectionManager {
    clients: HashMap<String, Arc<AcpClient>>,
}

impl AcpConnectionManager {
    pub async fn start_servers(configs: Vec<AcpServerSettings>) -> Result<Self>;
    pub async fn prompt(&self, server_name: &str, content: Vec<AcpContentBlock>) -> Result<AcpPromptResult>;
    pub async fn close_all(&self);
    pub fn agent_names(&self) -> Vec<String>;
}
```

- [ ] **Step 9: Create config.rs re-export**

```rust
pub use fabro_types::settings::run::{AcpServerSettings, AcpTransport};
```

- [ ] **Step 10: Run all tests**

Run: `cargo nextest run -p fabro-acp` — Expect: ALL PASS

- [ ] **Step 11: Commit**

```bash
git add lib/crates/fabro-acp/
git commit -m "feat(acp): implement AcpClient and AcpConnectionManager"
```

### Slice 2: Settings and Type Definitions

**Files:**
- Modify: `lib/crates/fabro-types/src/settings/run.rs`
- Modify: `lib/crates/fabro-types/src/settings/mod.rs` (if needed)

#### Task 2.1: Add ACP settings to fabro-types

- [ ] **Step 1: Write failing test for ACP settings deserialization**

Add a test in `lib/crates/fabro-types/` (or in a test within the settings module) that verifies `AcpServerSettings` and `AcpTransport` deserialize correctly from TOML:

```rust
#[test]
fn acp_server_settings_deserialize_stdio() {
    let toml = r#"
    name = "copilot"
    startup_timeout_secs = 30
    prompt_timeout_secs = 120
    [transport]
    type = "stdio"
    command = ["copilot", "--acp"]
    env = {}
    "#;
    let settings: AcpServerSettings = toml::from_str(toml).unwrap();
    assert_eq!(settings.name, "copilot");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p fabro-types` — Expect: FAIL (types not defined)

- [ ] **Step 3: Implement AcpServerSettings and AcpTransport**

In `lib/crates/fabro-types/src/settings/run.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpServerSettings {
    pub name: String,
    pub transport: AcpTransport,
    #[serde(default = "default_startup_timeout")]
    pub startup_timeout_secs: u64,
    #[serde(default = "default_prompt_timeout")]
    pub prompt_timeout_secs: u64,
}

fn default_startup_timeout() -> u64 { 10 }
fn default_prompt_timeout() -> u64 { 120 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AcpTransport {
    Stdio {
        command: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
    Http {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    Sandbox {
        command: Vec<String>,
        port: u16,
        #[serde(default)]
        env: HashMap<String, String>,
    },
}
```

Also add to `RunAgentSettings`:
```rust
pub struct RunAgentSettings {
    // ... existing fields ...
    #[serde(default)]
    pub acps: HashMap<String, AcpServerSettings>,
}
```

And add `AcpEntryLayer` for config file representation (mirroring `McpEntryLayer`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpEntryLayer {
    pub name: String,
    pub transport: AcpTransport,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub startup_timeout_secs: Option<u64>,
    #[serde(default)]
    pub prompt_timeout_secs: Option<u64>,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run -p fabro-types` — Expect: PASS

- [ ] **Step 5: Commit**

```bash
git add lib/crates/fabro-types/
git commit -m "feat(acp): add AcpServerSettings and AcpTransport to fabro-types"
```

### Slice 3: Agent-ACP Integration (Tool Bridge)

**Files:**
- Modify: `lib/crates/fabro-agent/Cargo.toml`
- Modify: `lib/crates/fabro-agent/src/mcp_integration.rs` (add `make_acp_tools()`)
- Modify: `lib/crates/fabro-agent/src/session.rs` (initialize ACP connections)
- Modify: `lib/crates/fabro-agent/src/lib.rs` (re-export)

#### Task 3.1: Bridge ACP agents into the tool registry

- [ ] **Step 1: Write failing test for ACP tools in agent session**

Add a test in `lib/crates/fabro-agent/` that creates an `AcpConnectionManager` with a test agent, calls `make_acp_tools()`, and verifies the resulting tools have qualified names like `acp__test_acp__prompt` and the executor calls the agent correctly.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p fabro-agent` — Expect: FAIL (`make_acp_tools` not implemented)

- [ ] **Step 3: Implement `make_acp_tools()`**

In `lib/crates/fabro-agent/src/mcp_integration.rs` (or a new `acp_integration.rs` module):

```rust
pub fn make_acp_tools(manager: &Arc<AcpConnectionManager>) -> Vec<RegisteredTool> {
    manager.agent_names().iter().map(|name| {
        let qualified = format!("acp__{}", sanitize_name(name));
        let mgr = Arc::clone(manager);
        let agent_name = name.clone();
        let prompt_timeout = Duration::from_secs(120);
        RegisteredTool {
            definition: ToolDefinition {
                name: qualified.clone(),
                description: format!("Send a prompt to the ACP agent '{}'", name),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "prompt": {"type": "string", "description": "The prompt to send to the agent"}
                    },
                    "required": ["prompt"]
                }),
            },
            executor: Arc::new(move |args, _ctx| {
                let mgr = Arc::clone(&mgr);
                let agent_name = agent_name.clone();
                let timeout = prompt_timeout;
                Box::pin(async move {
                    let prompt_text = args.get("prompt")
                        .and_then(|v| v.as_str())
                        .ok_or("missing 'prompt' argument")?;
                    let content = vec![AcpContentBlock::Text { text: prompt_text.to_string() }];
                    let result = mgr.prompt(&agent_name, content, timeout).await
                        .map_err(|e| e.to_string())?;
                    Ok(result.text)
                })
            }),
        }
    }).collect()
}
```

- [ ] **Step 4: Update session.rs to initialize ACP connections**

In `Session::initialize()`, alongside the existing MCP initialization block (lines 147-181), add ACP initialization:

```rust
if !acp_servers.is_empty() {
    let acp_manager = AcpConnectionManager::start_servers(acp_servers).await?;
    let acp_tools = make_acp_tools(&acp_manager);
    for tool in acp_tools {
        profile.tool_registry_mut().register(tool);
    }
    self.acp_manager = Some(acp_manager);
}
```

The `Session` struct needs an `acp_manager: Option<Arc<AcpConnectionManager>>` field.

- [ ] **Step 5: Run tests**

Run: `cargo nextest run -p fabro-agent` — Expect: PASS
Run: `cargo nextest run -p fabro-acp` — Expect: PASS

- [ ] **Step 6: Commit**

```bash
git add lib/crates/fabro-agent/
git commit -m "feat(acp): bridge ACP agents into agent tool registry"
```

### Slice 4: Workflow Integration

**Files:**
- Modify: `lib/crates/fabro-workflow/Cargo.toml`
- Modify: `lib/crates/fabro-workflow/src/handler/mod.rs`
- Modify: `lib/crates/fabro-workflow/src/handler/llm/api.rs`
- Modify: `lib/crates/fabro-workflow/src/handler/llm/mod.rs`

#### Task 4.1: Pass ACP config through the workflow engine

- [ ] **Step 1: Write failing test for `acp_agent` node type**

Add a workflow integration test that creates a graph with a node of type `acp_agent` and verifies it routes to an ACP handler. The test should use a mock ACP backend (similar to how `MockCodergenBackend` is used in existing tests).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p fabro-workflow` — Expect: FAIL (unknown handler type)

- [ ] **Step 3: Add ACP settings to engine context**

In `EngineServices` (handler/mod.rs) and `Context`, thread `acps: HashMap<String, AcpServerSettings>` from the run configuration through to the agent session creation.

- [ ] **Step 4: Pass ACP server configs to Session in `AgentApiBackend::create_session_for()`**

In `api.rs`, when creating the session, pass the `acp_servers` from the node configuration or run settings to `Session::initialize()`.

- [ ] **Step 5: Run tests**

Run: `cargo nextest run -p fabro-workflow` — Expect: PASS
Run: `cargo build --workspace` — Expect: BUILD SUCCESS

- [ ] **Step 6: Commit**

```bash
git add lib/crates/fabro-workflow/
git commit -m "feat(acp): thread ACP config through workflow engine"
```

### Slice 5: Configuration and Documentation

**Files:**
- Modify: Workflow TOML examples and/or default settings
- Modify: `lib/crates/fabro-cli/src/shared/provider_auth.rs` (if needed for ACP key resolution)
- Create: `docs/api-reference/fabro-api.yaml` updates (if the API surface changes)
- Update: `AGENTS.md` or equivalent docs

#### Task 5.1: CLI and configuration support

- [ ] **Step 1: Write failing test for ACP config in run settings TOML**

Test that a workflow TOML can specify `acp_servers` and they deserialize into `RunAgentSettings`:

```toml
[agent]
permissions = "default"

[agent.acps.copilot]
name = "copilot"
startup_timeout_secs = 30
prompt_timeout_secs = 120

[agent.acps.copilot.transport]
type = "stdio"
command = ["copilot", "--acp"]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p fabro-types` — Expect: FAIL or CONFIG ERROR

- [ ] **Step 3: Wire up config deserialization**

Ensure `AcpEntryLayer` implements conversion to `AcpServerSettings`, following the pattern of `McpEntryLayer`. Add any necessary `Deserialize` adjustments.

- [ ] **Step 4: Run tests**

Run: `cargo nextest run -p fabro-types` — Expect: PASS
Run: `cargo build --workspace` — Expect: BUILD SUCCESS

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(acp): configuration support for ACP servers in workflow TOML"
```

### Slice 6: End-to-End Validation and Cleanup

#### Task 6.1: Full workspace build and test

- [ ] **Step 1: Run full workspace build**

Run: `cargo build --workspace` — Expect: BUILD SUCCESS

- [ ] **Step 2: Run full workspace tests**

Run: `cargo nextest run --workspace` — Expect: ALL PASS

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace -- -D warnings` — Expect: NO WARNINGS

- [ ] **Step 4: Run formatting check**

Run: `cargo +nightly fmt --check --all` — Expect: NO ISSUES

- [ ] **Step 5: Fix any issues found**

Address any clippy warnings or formatting issues.

#### Task 6.2: Review and finalize

- [ ] **Step 6: Verify all integration tests pass with ACP test agent**

Run: `cargo nextest run -p fabro-acp --test stdio_integration` — Expect: ALL PASS

- [ ] **Step 7: Final commit**

```bash
git add -A
git commit -m "chore(acp): workspace-wide cleanup and final validation"
```

---

## Acceptance + Validation Coverage

- **AC1: ACP client can spawn and initialize an agent subprocess** — Task 1.2 validates with `acp_client_initialize` test
- **AC2: ACP connection manager aggregates multiple agents** — Task 1.2 validates with `connection_manager_acp_roundtrip` test
- **AC3: ACP settings deserialize from workflow TOML** — Task 2.1 validates with settings deserialization test
- **AC4: ACP agents appear as registered tools in agent sessions** — Task 3.1 validates with `make_acp_tools` test
- **AC5: Workflow engine passes ACP config through to sessions** — Task 4.1 validates with `acp_agent` node type test
- **AC6: Full workspace builds and all tests pass** — Task 6.1 validates with `cargo build --workspace`, `cargo nextest run --workspace`, `cargo clippy`, `cargo fmt --check`

## Final Plan Conformance Check

- [ ] All planned files were created/modified, or deviations were documented
- [ ] All validation commands were run and passed
- [ ] ACP settings types are in `fabro-types` (following the MCP pattern)
- [ ] ACP client mirrors `fabro-mcp` patterns (client, connection_manager, config re-export)
- [ ] ACP integration in agent follows `make_mcp_tools()` pattern
- [ ] Workflow threading follows existing patterns for passing config to sessions
- [ ] The official `agent-client-protocol` Rust crate v0.10.4 is used
- [ ] Test ACP agent script (Python) mirrors the MCP test server pattern
- [ ] No `Provider::AcAgent` added to the LLM provider enum (ACP is orthogonal to LLM providers)