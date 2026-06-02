//! ruflo-hooks: 8 hooks (pre/post task, edit, command, session) + SONA learning integration.
//!
//! All hooks dispatch through hooks.pre / hooks.post MCP tools with a kind discriminator.
//!
//! Hook kinds:
//! 1. task — Before/after Claude Code task
//! 2. edit — Before/after file write
//! 3. command — Before/after shell exec
//! 4. session — Boot/shutdown (restore/persist .rvf)

pub mod hooks;
pub mod post;
pub mod pre;
pub mod route;

pub use hooks::{HookKind, HookPayload};
pub use post::post_hook;
pub use pre::pre_hook;
pub use route::{route_task, RouteRecommendation};
