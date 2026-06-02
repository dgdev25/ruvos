# Phase 4: Hooks & SQLite-Backed Queue Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the 8-hook system (pre/post task, edit, command, session) with SQLite-backed durable queue, replacing the in-process daemon model from Ruflo v3.

**Architecture:** Phase 4 replaces Ruflo's in-process daemon (which caused Windows persistence bug #1766 and headless race #2251) with a stateless design: hooks emit events to a SQLite queue, hook handlers execute via tokio, and state is persisted to `.rvf` containers. All hooks use unified `hooks.pre` / `hooks.post` MCP tools with kind discriminators (task, edit, command, session).

**Tech Stack:** Rust 1.77+, tokio (async), rusqlite (SQLite), serde (serialization), chrono (timestamps).

**Total new LOC budget:** ~2,500 (within 3k `ruflo-hooks` budget)

---

## File Structure

### New Files

- `crates/ruflo-hooks/src/lib.rs` — Main library, public API exports
- `crates/ruflo-hooks/src/types.rs` — Hook event types (HookKind, HookEvent, PreHookRequest, PostHookRequest)
- `crates/ruflo-hooks/src/queue.rs` — SQLite queue: create/read/enqueue/dequeue (~200 LOC)
- `crates/ruflo-hooks/src/handlers.rs` — Hook handler dispatch (~150 LOC)
- `crates/ruflo-hooks/src/sona_bridge.rs` — Integration with SONA learning (~100 LOC)
- `crates/ruflo-mcp/src/tools/hooks.rs` — Update stubs to real implementations using hooks crate (~150 LOC)

### Modified Files

- `crates/ruflo-hooks/src/lib.rs` — Export modules and public API
- `crates/ruflo-mcp/src/tools/hooks.rs` — Replace 3 stub handlers with real implementations
- `Cargo.toml` — Add dependencies: `rusqlite`, `chrono`

---

## Task Breakdown

### Task 1: Define Hook Types and Queue Schema

**Files:**
- Create: `crates/ruflo-hooks/src/types.rs`
- Modify: `crates/ruflo-hooks/src/lib.rs`

**Steps:**

- [ ] **Step 1: Define hook types**

Create `crates/ruflo-hooks/src/types.rs`:

```rust
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HookKind {
    Task,
    Edit,
    Command,
    Session,
}

impl HookKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookKind::Task => "task",
            HookKind::Edit => "edit",
            HookKind::Command => "command",
            HookKind::Session => "session",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    pub id: String,
    pub kind: HookKind,
    pub phase: HookPhase,
    pub timestamp: DateTime<Utc>,
    pub payload: serde_json::Value,
    pub status: EventStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HookPhase {
    Pre,
    Post,
}

impl HookPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookPhase::Pre => "pre",
            HookPhase::Post => "post",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreHookRequest {
    pub kind: HookKind,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostHookRequest {
    pub kind: HookKind,
    pub payload: serde_json::Value,
    pub outcome: HookOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookOutcome {
    pub success: bool,
    pub message: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResponse {
    pub status: String,
    pub routing: Option<HookRouting>,
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRouting {
    pub model: Option<String>,
    pub archetype: Option<String>,
}
```

- [ ] **Step 2: Update lib.rs**

Modify `crates/ruflo-hooks/src/lib.rs`:

```rust
pub mod types;
pub mod queue;
pub mod handlers;
pub mod sona_bridge;

pub use types::*;
pub use queue::HookQueue;
pub use handlers::HookDispatcher;

pub fn create_queue(db_path: &str) -> anyhow::Result<HookQueue> {
    HookQueue::new(db_path)
}

pub fn create_dispatcher() -> HookDispatcher {
    HookDispatcher::new()
}
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check --lib ruflo-hooks
```

Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add crates/ruflo-hooks/src/types.rs crates/ruflo-hooks/src/lib.rs
git commit -m "feat: define hook types and core data structures"
```

---

### Task 2: Implement SQLite Hook Queue

**Files:**
- Create: `crates/ruflo-hooks/src/queue.rs`
- Test: Create inline tests in queue.rs

**Steps:**

- [ ] **Step 1: Implement SQLite queue**

Create `crates/ruflo-hooks/src/queue.rs` (~200 LOC):

```rust
use crate::types::*;
use anyhow::Result;
use chrono::Utc;
use rusqlite::{Connection, params};
use serde_json::json;
use uuid::Uuid;

pub struct HookQueue {
    conn: Connection,
}

impl HookQueue {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        Self::init_schema(&conn)?;
        Ok(HookQueue { conn })
    }

    fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS hook_events (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                phase TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                payload TEXT NOT NULL,
                status TEXT NOT NULL
            )",
        )?;
        Ok(())
    }

    pub fn enqueue(&self, kind: HookKind, phase: HookPhase, payload: serde_json::Value) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let timestamp = Utc::now().to_rfc3339();
        let payload_str = serde_json::to_string(&payload)?;

        self.conn.execute(
            "INSERT INTO hook_events (id, kind, phase, timestamp, payload, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &id,
                kind.as_str(),
                phase.as_str(),
                &timestamp,
                &payload_str,
                "pending"
            ],
        )?;

        Ok(id)
    }

    pub fn dequeue(&self) -> Result<Option<HookEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, phase, timestamp, payload, status FROM hook_events WHERE status = 'pending' ORDER BY timestamp LIMIT 1"
        )?;

        let event = stmt.query_row([], |row| {
            Ok(HookEvent {
                id: row.get(0)?,
                kind: match row.get::<_, String>(1)?.as_str() {
                    "task" => HookKind::Task,
                    "edit" => HookKind::Edit,
                    "command" => HookKind::Command,
                    "session" => HookKind::Session,
                    _ => HookKind::Task,
                },
                phase: match row.get::<_, String>(2)?.as_str() {
                    "pre" => HookPhase::Pre,
                    "post" => HookPhase::Post,
                    _ => HookPhase::Pre,
                },
                timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)?
                    .with_timezone(&Utc),
                payload: serde_json::from_str(&row.get::<_, String>(4)?)?,
                status: match row.get::<_, String>(5)?.as_str() {
                    "pending" => EventStatus::Pending,
                    "processing" => EventStatus::Processing,
                    "completed" => EventStatus::Completed,
                    "failed" => EventStatus::Failed,
                    _ => EventStatus::Pending,
                },
            })
        }).optional()?;

        Ok(event)
    }

    pub fn mark_completed(&self, event_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE hook_events SET status = 'completed' WHERE id = ?1",
            params![event_id],
        )?;
        Ok(())
    }

    pub fn mark_failed(&self, event_id: &str, reason: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE hook_events SET status = 'failed' WHERE id = ?1",
            params![event_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enqueue_and_dequeue() {
        let queue = HookQueue::new(":memory:").expect("create queue");
        let payload = json!({"task": "test"});
        
        let id = queue.enqueue(HookKind::Task, HookPhase::Pre, payload.clone())
            .expect("enqueue event");
        
        assert!(!id.is_empty());
        
        let event = queue.dequeue().expect("dequeue event").expect("event exists");
        assert_eq!(event.id, id);
        assert_eq!(event.kind, HookKind::Task);
        assert_eq!(event.phase, HookPhase::Pre);
    }
}
```

- [ ] **Step 2: Verify tests**

```bash
cargo test --lib ruflo_hooks::queue
```

Expected: Tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/ruflo-hooks/src/queue.rs
git commit -m "feat: implement SQLite-backed hook event queue"
```

---

### Task 3: Implement Hook Handler Dispatcher

**Files:**
- Create: `crates/ruflo-hooks/src/handlers.rs`

**Steps:**

- [ ] **Step 1: Implement dispatcher**

Create `crates/ruflo-hooks/src/handlers.rs` (~150 LOC):

```rust
use crate::types::*;
use anyhow::Result;
use serde_json::json;

pub struct HookDispatcher;

impl HookDispatcher {
    pub fn new() -> Self {
        HookDispatcher
    }

    pub async fn dispatch_pre(&self, kind: HookKind, payload: serde_json::Value) -> Result<HookResponse> {
        // Route based on kind
        match kind {
            HookKind::Task => self.handle_pre_task(&payload).await,
            HookKind::Edit => self.handle_pre_edit(&payload).await,
            HookKind::Command => self.handle_pre_command(&payload).await,
            HookKind::Session => self.handle_pre_session(&payload).await,
        }
    }

    pub async fn dispatch_post(&self, kind: HookKind, payload: serde_json::Value, outcome: HookOutcome) -> Result<HookResponse> {
        // Route based on kind
        match kind {
            HookKind::Task => self.handle_post_task(&payload, outcome).await,
            HookKind::Edit => self.handle_post_edit(&payload, outcome).await,
            HookKind::Command => self.handle_post_command(&payload, outcome).await,
            HookKind::Session => self.handle_post_session(&payload, outcome).await,
        }
    }

    async fn handle_pre_task(&self, _payload: &serde_json::Value) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_post_task(&self, _payload: &serde_json::Value, _outcome: HookOutcome) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_pre_edit(&self, _payload: &serde_json::Value) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_post_edit(&self, _payload: &serde_json::Value, _outcome: HookOutcome) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_pre_command(&self, _payload: &serde_json::Value) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_post_command(&self, _payload: &serde_json::Value, _outcome: HookOutcome) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_pre_session(&self, _payload: &serde_json::Value) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_post_session(&self, _payload: &serde_json::Value, _outcome: HookOutcome) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }
}

impl Default for HookDispatcher {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check --lib ruflo_hooks
```

Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add crates/ruflo-hooks/src/handlers.rs
git commit -m "feat: implement hook handler dispatcher with 8 hook kinds"
```

---

### Task 4: SONA Learning Bridge (Stub)

**Files:**
- Create: `crates/ruflo-hooks/src/sona_bridge.rs`

**Steps:**

- [ ] **Step 1: Create stub**

Create `crates/ruflo-hooks/src/sona_bridge.rs`:

```rust
use crate::types::*;
use anyhow::Result;

pub struct SonaLearningBridge;

impl SonaLearningBridge {
    pub fn new() -> Self {
        SonaLearningBridge
    }

    pub fn record_outcome(&self, kind: HookKind, outcome: &HookOutcome) -> Result<()> {
        // Phase 5: Integrate with SONA for learning
        // For now, just log the outcome
        println!("Outcome recorded for {:?}: {:?}", kind, outcome);
        Ok(())
    }
}

impl Default for SonaLearningBridge {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/ruflo-hooks/src/sona_bridge.rs
git commit -m "feat: add SONA learning bridge stub (Phase 5 integration)"
```

---

### Task 5: MCP Tool Handlers (hooks.pre and hooks.post)

**Files:**
- Modify: `crates/ruflo-mcp/src/tools/hooks.rs`
- Modify: `crates/ruflo-mcp/Cargo.toml`

**Steps:**

- [ ] **Step 1: Add dependency**

Modify `crates/ruflo-mcp/Cargo.toml`:

```toml
ruflo-hooks = { path = "../ruflo-hooks" }
```

- [ ] **Step 2: Implement real handlers**

Replace stub handlers in `crates/ruflo-mcp/src/tools/hooks.rs`:

```rust
use crate::tools::handler::ToolHandler;
use anyhow::Result as AnyhowResult;
use ruflo_hooks::{HookDispatcher, HookKind, HookOutcome};
use serde_json::{json, Value};

pub struct HooksPreHandler {
    dispatcher: HookDispatcher,
}

impl HooksPreHandler {
    pub fn new() -> Self {
        HooksPreHandler {
            dispatcher: HookDispatcher::new(),
        }
    }
}

impl ToolHandler for HooksPreHandler {
    fn name(&self) -> &'static str {
        "pre"
    }

    fn domain(&self) -> &'static str {
        "hooks"
    }

    fn validate(&self, params: &Value) -> AnyhowResult<()> {
        if !params.is_object() {
            return Err(anyhow::anyhow!("expected object"));
        }
        
        if params.get("kind").and_then(|v| v.as_str()).is_none() {
            return Err(anyhow::anyhow!("missing 'kind' field"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> AnyhowResult<Value> {
        let kind_str = params
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap();

        let kind = match kind_str {
            "task" => ruflo_hooks::HookKind::Task,
            "edit" => ruflo_hooks::HookKind::Edit,
            "command" => ruflo_hooks::HookKind::Command,
            "session" => ruflo_hooks::HookKind::Session,
            _ => return Err(anyhow::anyhow!("invalid kind: {}", kind_str)),
        };

        let payload = params.get("payload").cloned().unwrap_or(json!({}));

        match self.dispatcher.dispatch_pre(kind, payload).await {
            Ok(response) => Ok(json!({
                "status": response.status,
                "routing": response.routing,
                "context": response.context,
            })),
            Err(e) => Ok(json!({
                "error": e.to_string(),
                "status": "error",
            })),
        }
    }
}

pub struct HooksPostHandler {
    dispatcher: HookDispatcher,
}

impl HooksPostHandler {
    pub fn new() -> Self {
        HooksPostHandler {
            dispatcher: HookDispatcher::new(),
        }
    }
}

impl ToolHandler for HooksPostHandler {
    fn name(&self) -> &'static str {
        "post"
    }

    fn domain(&self) -> &'static str {
        "hooks"
    }

    fn validate(&self, params: &Value) -> AnyhowResult<()> {
        if !params.is_object() {
            return Err(anyhow::anyhow!("expected object"));
        }
        
        if params.get("kind").and_then(|v| v.as_str()).is_none() {
            return Err(anyhow::anyhow!("missing 'kind' field"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> AnyhowResult<Value> {
        let kind_str = params
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap();

        let kind = match kind_str {
            "task" => ruflo_hooks::HookKind::Task,
            "edit" => ruflo_hooks::HookKind::Edit,
            "command" => ruflo_hooks::HookKind::Command,
            "session" => ruflo_hooks::HookKind::Session,
            _ => return Err(anyhow::anyhow!("invalid kind: {}", kind_str)),
        };

        let payload = params.get("payload").cloned().unwrap_or(json!({}));
        let outcome = HookOutcome {
            success: params
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            message: params
                .get("message")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            metadata: params.get("metadata").cloned().unwrap_or(json!({})),
        };

        match self.dispatcher.dispatch_post(kind, payload, outcome).await {
            Ok(response) => Ok(json!({
                "status": response.status,
                "routing": response.routing,
                "context": response.context,
            })),
            Err(e) => Ok(json!({
                "error": e.to_string(),
                "status": "error",
            })),
        }
    }
}
```

- [ ] **Step 3: Update tool registry**

Modify `crates/ruflo-mcp/src/tools/mod.rs` to use real handlers instead of stubs.

- [ ] **Step 4: Verify compilation**

```bash
cargo check --all-features
```

Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add crates/ruflo-mcp/src/tools/hooks.rs crates/ruflo-mcp/Cargo.toml crates/ruflo-mcp/src/tools/mod.rs
git commit -m "feat: implement hooks.pre and hooks.post MCP tools"
```

---

### Task 6: Workspace Build & Test Validation

**Files:**
- None (validation only)

**Steps:**

- [ ] **Step 1-6: Follow Phase 3 Task 8 pattern**

Run the same build/test/lint checks as Phase 3 Task 8.

Expected: All pass.

- [ ] **Step 7: Commit if needed**

If auto-formatting: `git add -A && git commit -m "Phase 4: Auto-format code"`

---

### Task 7: Documentation Update

**Files:**
- Modify: `CLAUDE.md`

**Steps:**

- [ ] Append Phase 4 completion section to CLAUDE.md (mirrors Phase 3 format)
- [ ] Commit: "docs: Phase 4 completion documented"

---

## Success Criteria

**Phase 4 is complete when:**

1. ✅ All hook types defined (8 kinds: task, edit, command, session × pre, post)
2. ✅ SQLite queue working (enqueue, dequeue, mark_completed, mark_failed)
3. ✅ Hook dispatcher routes all 8 hooks to handlers
4. ✅ `hooks.pre` and `hooks.post` MCP tools work end-to-end
5. ✅ Full workspace builds with zero warnings
6. ✅ All tests pass
7. ✅ CLAUDE.md updated

---

## Handoff to Phase 5

Once Phase 4 validates the hooks system:

**Phase 5:** Implement real tool logic for memory, session, and agent tools. Hook integration provides the learning feedback loop.
