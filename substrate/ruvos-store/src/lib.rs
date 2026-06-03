//! `ruvos-store`: a pure-Rust, redb-backed store for rUvOS swarm state with
//! signed `.rvf` snapshots for provenance.
//!
//! This crate replaces the SQLite-based `ruv-swarm-persistence` with:
//! - **redb** ([`Store`]) as the live, transactional key-value store (no C
//!   dependency), holding agents, tasks, events, messages, and metrics; and
//! - **`.rvf` snapshots** ([`Store::snapshot_to_rvf`] /
//!   [`Store::restore_from_rvf`]) — a JSON payload of every record wrapped in a
//!   SHAKE-256 witness chain whose final entry is a keyed HMAC-SHA256
//!   attestation of the payload (via `rvf-crypto`), giving authenticity, not
//!   just tamper-evidence.
//!
//! All write paths use a single redb write transaction; in particular
//! [`Store::claim_task`] performs its read-check-write inside one transaction
//! so concurrent claimers cannot both succeed.

mod records;
mod snapshot;
mod store;

pub use records::{
    AgentRecord, EventRecord, MessageRecord, MetricRecord, StoreSnapshot, TaskRecord,
};
pub use snapshot::SnapshotContainer;
pub use store::Store;
