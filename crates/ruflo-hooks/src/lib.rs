//! ruflo-hooks: 8 hooks (pre/post task, edit, command, session) + SONA learning integration.
//!
//! All hooks dispatch through hooks.pre / hooks.post MCP tools with a kind discriminator.
//!
//! Hook kinds:
//! 1. task — Before/after Claude Code task
//! 2. edit — Before/after file write
//! 3. command — Before/after shell exec
//! 4. session — Boot/shutdown (restore/persist .rvf)

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
