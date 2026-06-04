//! `ruvos-skills`: a portable, redb-backed skills pack for rUvOS.
//!
//! The pack is intentionally separate from live swarm state. It stores
//! normalized skill metadata, deduplicated chunks, retrieval indexes, and
//! feedback signals in a single portable `skills.redb` file.

mod records;
mod store;

pub use records::{
    hash_bytes, CompressionCodec, SkillChunkLink, SkillChunkRecord, SkillFeedbackRecord,
    SkillPackMeta, SkillRecord, SkillSearchHit, SkillSource,
};
pub use store::SkillStore;
