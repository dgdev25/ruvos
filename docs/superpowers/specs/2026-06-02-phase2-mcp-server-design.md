# Phase 2: MCP Server + Echo Tool — Design Specification

> **For agentic workers:** Use superpowers:subagent-driven-development to implement this plan task-by-task once the implementation plan is written.

**Goal:** Implement a working `ruflo mcp serve` command that ships a minimal MCP server with a hello-world echo tool and validates end-to-end integration with Claude Code CLI.

**Architecture:** Phase 2 builds the MCP protocol foundation and tool handler framework. The echo tool serves as proof that the architecture works. The other 19 tools are stubbed, ready for Phase 3+ implementation. Automated integration test validates the round-trip from Claude Code CLI → Ruflo MCP server → response back to Claude Code.

**Tech Stack:** Rust 1.77+, Tokio (async), serde_json (JSON-RPC), UUID for request tracking.

---

## 1. Core Problem Phase 2 Solves

**Current state (end of Phase 1):** Ruflo has 6 crates scaffolded (cli, mcp, host, plugin-host, hooks, session) with module stubs but no actual implementation.

**Phase 2 deliverable:** A working MCP server that Claude Code CLI can connect to, call a tool, and receive a response. Proves the architecture is sound before building real tool logic in Phase 3+.

**Why this matters:** If the MCP round-trip fails, the entire v1 rewrite fails. Validating this early (week 2 of the project) prevents discovering architectural issues in Phase 5.

---

## 2. MCP Server Architecture

### 2.1 JSON-RPC Protocol Over Stdio

**Design decision:** Implement JSON-RPC 2.0 server manually over `tokio::io::stdin/stdout` rather than use a third-party crate.

**Rationale:** MCP protocol is small enough (~500 LOC for full server skeleton). Rolling our own gives clarity, zero external dependencies, and full control over error handling and multi-tool dispatch.

**Structure:**
```rust
// crates/ruflo-mcp/src/server.rs
pub struct JsonRpcServer {
    stdin: BufReader<Stdin>,
    stdout: BufWriter<Stdout>,
    registry: ToolRegistry,
}

impl JsonRpcServer {
    async fn run(&mut self) -> Result<()> {
        loop {
            let line = self.read_line().await?;
            let req: JsonRpcRequest = serde_json::from_str(&line)?;
            let res = self.dispatch_tool(&req).await;
            self.write_response(&res).await?;
        }
    }

    async fn dispatch_tool(&self, req: &JsonRpcRequest) -> JsonRpcResponse {
        // Lookup tool in registry by req.method
        // Call handler.execute(req)
        // Wrap response as JSON-RPC
    }
}

#[derive(Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,  // "2.0"
    method: String,   // "echo.test", "memory.search", etc.
    params: serde_json::Value,
    id: String,       // request ID
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
    id: String,
}
```

**Error handling:**
- Malformed JSON: return JSON-RPC error (code: -32700)
- Unknown method: return "method not found" error (code: -32601)
- Handler panic: catch, return error response (code: -32000)
- Handler returns error: wrap in JSON-RPC error field

---

### 2.2 Tool Handler Framework

**Design:** Trait-based registry for extensibility.

```rust
// crates/ruflo-mcp/src/tools/handler.rs

pub trait ToolHandler: Send + Sync {
    fn name(&self) -> &'static str;  // "echo"
    fn domain(&self) -> &'static str;  // "test"
    fn validate(&self, params: &serde_json::Value) -> Result<()>;
    async fn execute(&self, params: serde_json::Value) -> Result<serde_json::Value>;
}

pub struct ToolRegistry {
    handlers: HashMap<String, Box<dyn ToolHandler>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = ToolRegistry {
            handlers: HashMap::new(),
        };
        
        // Register echo tool (real implementation)
        registry.register(Box::new(EchoHandler));
        
        // Register 19 stub tools
        registry.register(Box::new(MemorySearchStub));
        registry.register(Box::new(SessionCreateStub));
        // ... etc
        
        registry
    }

    pub fn register(&mut self, handler: Box<dyn ToolHandler>) {
        let key = format!("{}.{}", handler.domain(), handler.name());
        self.handlers.insert(key, handler);
    }

    pub async fn execute(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let handler = self.handlers.get(method)
            .ok_or_else(|| Error::MethodNotFound)?;
        
        handler.validate(&params)?;
        handler.execute(params).await
    }
}
```

**Tool organization:**
- `tools/mod.rs`: registry initialization, module declarations
- `tools/handler.rs`: ToolHandler trait, ToolRegistry struct
- `tools/echo.rs`: EchoHandler implementation (real tool)
- `tools/memory.rs`: MemorySearchStub, etc. (19 stubs)
- `tools/session.rs`, `tools/agent.rs`, etc.: Domain stubs

---

## 3. Echo Tool Implementation

**Purpose:** Minimal real tool that proves the handler framework works end-to-end.

**Specification:**

```rust
// crates/ruflo-mcp/src/tools/echo.rs

pub struct EchoHandler;

impl ToolHandler for EchoHandler {
    fn name(&self) -> &'static str { "test" }
    fn domain(&self) -> &'static str { "echo" }

    fn validate(&self, params: &serde_json::Value) -> Result<()> {
        if !params.is_object() {
            return Err(Error::InvalidParams("expected object"));
        }
        if params.get("message").and_then(|v| v.as_str()).is_none() {
            return Err(Error::InvalidParams("missing 'message' field"));
        }
        Ok(())
    }

    async fn execute(&self, params: serde_json::Value) -> Result<serde_json::Value> {
        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();

        Ok(json!({
            "echo": message,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "handler": "echo",
        }))
    }
}
```

**Request/Response example:**

Request (from Claude Code CLI):
```json
{"jsonrpc": "2.0", "method": "echo.test", "params": {"message": "hello from Claude Code"}, "id": "req-001"}
```

Response (from Ruflo):
```json
{"jsonrpc": "2.0", "result": {"echo": "hello from Claude Code", "timestamp": "2026-06-02T10:30:45Z", "handler": "echo"}, "id": "req-001"}
```

---

## 4. Stub Tools (19 domains)

All 19 other tools implement the handler trait with a "not implemented" response:

```rust
pub struct MemorySearchStub;

impl ToolHandler for MemorySearchStub {
    fn name(&self) -> &'static str { "search_similar" }
    fn domain(&self) -> &'static str { "memory" }

    fn validate(&self, _params: &serde_json::Value) -> Result<()> { Ok(()) }

    async fn execute(&self, _params: serde_json::Value) -> Result<serde_json::Value> {
        Ok(json!({
            "status": "not_implemented",
            "message": "memory.search_similar will be implemented in Phase 5",
        }))
    }
}
```

**19 stubs total:**
- memory (4 tools): search_similar, store, retrieve, list
- session (3): create, resume, fork
- agent (3): spawn, status, message
- hooks (3): pre, post, route
- intel (2): pattern_search, pattern_store
- plugin (2): list, invoke
- gov (2): witness_verify, health
- workflow (1): run

All stubs follow the same pattern: validate passes, execute returns "not_implemented" JSON.

---

## 5. CLI Integration: `ruflo mcp serve`

**Command:** User runs `ruflo mcp serve` to start the MCP server.

```rust
// crates/ruflo-cli/src/commands/mcp.rs

pub async fn mcp_serve() -> Result<()> {
    let mut server = JsonRpcServer::new();
    server.run().await
}

// crates/ruflo-cli/src/main.rs (in clap setup)
#[derive(Subcommand)]
enum Commands {
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
}

#[derive(Subcommand)]
enum McpAction {
    Serve,
}
```

**Usage:**
```bash
cargo run --release -- mcp serve
# Server starts, listens on stdin/stdout, ready for Claude Code CLI
```

**Claude Code CLI connection:**
```bash
claude mcp add ruflo -- ./target/release/ruflo mcp serve
# Claude Code now has access to all 20 Ruflo tools via MCP
```

---

## 6. Automated Integration Test

**File:** `crates/ruflo-mcp/tests/integration_test.rs`

**What it does:**
1. Spawns `./target/release/ruflo mcp serve` as a child process
2. Spawns Claude Code CLI connected to Ruflo's stdin/stdout
3. Calls `echo.test` with a test message
4. Verifies the echo response matches the input
5. Verifies JSON-RPC structure is correct
6. Shuts down both processes, checks for clean exit

**Pseudocode:**
```rust
#[tokio::test]
async fn test_mcp_echo_integration() {
    // Start Ruflo server
    let mut ruflo = Command::new("./target/release/ruflo")
        .args(&["mcp", "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn ruflo");

    let mut stdin = ruflo.stdin.take().unwrap();
    let mut stdout = ruflo.stdout.take().unwrap();

    // Send echo request
    let request = json!({
        "jsonrpc": "2.0",
        "method": "echo.test",
        "params": {"message": "integration-test"},
        "id": "test-1"
    });
    stdin.write_all(request.to_string().as_bytes()).await.unwrap();
    stdin.write_all(b"\n").await.unwrap();

    // Read response
    let mut reader = BufReader::new(&mut stdout);
    let mut response_line = String::new();
    reader.read_line(&mut response_line).await.unwrap();

    let response: JsonRpcResponse = serde_json::from_str(&response_line).unwrap();

    // Assertions
    assert_eq!(response.jsonrpc, "2.0");
    assert!(response.error.is_none());
    assert_eq!(response.result["echo"].as_str().unwrap(), "integration-test");
    assert_eq!(response.id, "test-1");

    // Cleanup
    ruflo.kill().await.unwrap();
}
```

**CI integration:** Test runs as part of `cargo test` in the CI pipeline.

---

## 7. File Structure & Deliverables

### New files:
- `crates/ruflo-mcp/src/server.rs` — JSON-RPC server (500 LOC)
- `crates/ruflo-mcp/src/tools/handler.rs` — ToolHandler trait + registry (200 LOC)
- `crates/ruflo-mcp/src/tools/echo.rs` — Echo tool (50 LOC)
- `crates/ruflo-mcp/src/tools/memory.rs` through `workflow.rs` — 19 stubs (200 LOC total)
- `crates/ruflo-mcp/tests/integration_test.rs` — Integration test (150 LOC)

### Modified files:
- `crates/ruflo-mcp/src/lib.rs` — Export server module
- `crates/ruflo-mcp/src/tools/mod.rs` — Registry initialization
- `crates/ruflo-cli/src/main.rs` — Add `mcp serve` command
- `crates/ruflo-cli/src/commands/mcp.rs` — New file for mcp command implementation

### Total LOC for Phase 2:
- New Rust: ~1,100 LOC
- Within ruflo-mcp budget: 6k LOC (plenty of room)
- Aligns with 30k total budget: yes ✓

---

## 8. Success Criteria

**Phase 2 is complete when:**

1. ✅ `cargo build --release` succeeds with zero warnings
2. ✅ `./target/release/ruflo mcp serve` starts and listens for JSON-RPC requests
3. ✅ Echo tool responds correctly: input message → echoed back in response
4. ✅ JSON-RPC protocol is correct: proper jsonrpc, result/error, id fields
5. ✅ Integration test runs in CI and passes: Claude Code CLI → Ruflo → echo response verified
6. ✅ All 19 stub tools return "not_implemented" without panicking
7. ✅ Error handling works: malformed JSON, unknown method, missing params → proper error responses
8. ✅ Code follows Rust idioms (clippy clean, rustfmt compliant, <500 LOC per file)
9. ✅ Documentation: CLAUDE.md updated with Phase 2 completion notes

---

## 9. Handoff to Phase 3

Once Phase 2 validates the MCP server architecture:

**Phase 3:** Implement plugin host (markdown discovery, shell exec, skill compatibility). The tool handlers remain stubs; real tool logic arrives in Phase 5.

**Foundation for Phase 5:** The ToolHandler trait makes it trivial to add real tools later. Each Phase 5+ tool is just:
1. Define a new struct implementing ToolHandler
2. Register it in the registry
3. Done

---

## 10. Key Architectural Decisions

| Decision | Rationale |
|----------|-----------|
| Custom JSON-RPC over third-party | MCP is small; control + zero deps > convenience |
| Trait-based handlers | Extensible, testable, idiomatic Rust |
| Echo as the only real tool | Proves data flow works; other 19 are stubs |
| Automated integration test | Validates round-trip before Phase 5 real logic |
| Stubs return "not_implemented" | Clear signal that these are placeholders |
| No tool logic in Phase 2 | Focus on protocol + architecture; logic comes Phase 5+ |

---

## 11. Risks & Mitigation

| Risk | Probability | Mitigation |
|------|-------------|-----------|
| JSON-RPC protocol misses MCP spec edge case | Low | Test with real Claude Code CLI early; iterate |
| Handler panic crashes server | Medium | Add try-catch in dispatch; return error response |
| Integration test flakes (process timing) | Medium | Add 100ms delays, retry logic, proper process cleanup |
| Stub tools confuse users | Low | Clarify in error message: "not_implemented, coming Phase 5" |
| stdout/stdin buffering issues | Low | Use BufReader/BufWriter, explicit flush after each response |

