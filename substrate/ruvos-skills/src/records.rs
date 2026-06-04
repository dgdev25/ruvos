//! Records stored in the redb-backed skills pack.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// Current UNIX timestamp in whole seconds.
pub(crate) fn now_secs() -> i64 {
    Utc::now().timestamp()
}

/// Compression codec used to store a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionCodec {
    /// No compression.
    None,
    /// Gzip-compressed payload.
    Gzip,
}

impl Default for CompressionCodec {
    fn default() -> Self {
        Self::Gzip
    }
}

/// Provenance for a skill corpus entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSource {
    /// Root source path used to build the pack.
    pub source_root: String,
    /// Specific source file or corpus path.
    pub source_path: String,
    /// Corpus hash used for deterministic rebuilds.
    pub corpus_hash: String,
}

/// Canonical skill metadata stored in the pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRecord {
    /// Stable skill id.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Semantic version for the normalized skill.
    pub version: String,
    /// Short purpose statement.
    pub purpose: String,
    /// Tags for fast lookup.
    pub tags: Vec<String>,
    /// Alternative names and aliases.
    pub aliases: Vec<String>,
    /// Preconditions the caller should satisfy.
    pub prerequisites: Vec<String>,
    /// Safety classification.
    pub safety_level: String,
    /// How to verify the skill was applied correctly.
    pub validation: Vec<String>,
    /// Optional summary or abstract.
    pub summary: Option<String>,
    /// Source provenance.
    pub source: SkillSource,
    /// Creation timestamp.
    pub created_at: i64,
    /// Update timestamp.
    pub updated_at: i64,
}

impl SkillRecord {
    /// Build the indexable text tokens for this skill.
    pub fn index_terms(&self) -> BTreeSet<String> {
        let mut terms = BTreeSet::new();
        self.extend_terms(&mut terms, &self.name);
        self.extend_terms(&mut terms, &self.purpose);
        self.extend_terms(&mut terms, &self.safety_level);
        if let Some(summary) = &self.summary {
            self.extend_terms(&mut terms, summary);
        }
        for tag in &self.tags {
            self.extend_terms(&mut terms, tag);
        }
        for alias in &self.aliases {
            self.extend_terms(&mut terms, alias);
        }
        for item in &self.prerequisites {
            self.extend_terms(&mut terms, item);
        }
        for item in &self.validation {
            self.extend_terms(&mut terms, item);
        }
        self.extend_terms(&mut terms, &self.source.source_path);
        terms
    }

    fn extend_terms(&self, out: &mut BTreeSet<String>, text: &str) {
        for token in tokenize(text) {
            out.insert(token);
        }
    }
}

/// Deduplicated chunk payload stored by content hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillChunkRecord {
    /// Content hash of the uncompressed chunk payload.
    pub hash: String,
    /// Compression codec used for the stored bytes.
    pub codec: CompressionCodec,
    /// Original uncompressed byte length.
    pub original_len: usize,
    /// Stored compressed byte length.
    pub compressed_len: usize,
    /// Stored payload bytes.
    pub data: Vec<u8>,
}

impl SkillChunkRecord {
    /// Construct a new chunk record.
    pub fn new(
        hash: String,
        codec: CompressionCodec,
        original_len: usize,
        compressed_len: usize,
        data: Vec<u8>,
    ) -> Self {
        Self {
            hash,
            codec,
            original_len,
            compressed_len,
            data,
        }
    }
}

/// Ordered mapping from a skill to a chunk hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillChunkLink {
    /// Ordinal inside the skill definition.
    pub ordinal: u32,
    /// Hash of the referenced chunk.
    pub chunk_hash: String,
}

/// Feedback counters used to rank skills over time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillFeedbackRecord {
    /// Skill identifier.
    pub skill_id: String,
    /// Number of times selected.
    pub usage_count: u64,
    /// Number of successful outcomes.
    pub success_count: u64,
    /// Number of failed outcomes.
    pub failure_count: u64,
    /// Last time the skill was used.
    pub last_used_at: i64,
    /// Last recorded outcome label.
    pub last_outcome: Option<String>,
    /// Free-form notes.
    pub notes: Vec<String>,
}

impl SkillFeedbackRecord {
    /// Create an empty feedback record.
    pub fn new(skill_id: impl Into<String>) -> Self {
        Self {
            skill_id: skill_id.into(),
            usage_count: 0,
            success_count: 0,
            failure_count: 0,
            last_used_at: 0,
            last_outcome: None,
            notes: Vec::new(),
        }
    }
}

/// Pack-level metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillPackMeta {
    /// Schema version for the pack layout.
    pub schema_version: u32,
    /// Deterministic corpus hash.
    pub corpus_hash: String,
    /// Root corpus path used to build the pack.
    pub source_root: String,
    /// Compression codec used for stored chunks.
    pub codec: CompressionCodec,
    /// Number of normalized skills.
    pub skill_count: u64,
    /// Number of unique chunks.
    pub chunk_count: u64,
    /// Build time.
    pub built_at: i64,
}

impl SkillPackMeta {
    /// Create a new pack metadata record.
    pub fn new(
        corpus_hash: impl Into<String>,
        source_root: impl Into<String>,
        codec: CompressionCodec,
        skill_count: u64,
        chunk_count: u64,
    ) -> Self {
        Self {
            schema_version: 1,
            corpus_hash: corpus_hash.into(),
            source_root: source_root.into(),
            codec,
            skill_count,
            chunk_count,
            built_at: now_secs(),
        }
    }
}

/// Search hit returned by the skills pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSearchHit {
    /// Skill identifier.
    pub skill_id: String,
    /// Integer relevance score.
    pub score: u32,
    /// Matched tokens.
    pub matched_terms: Vec<String>,
    /// Reason the hit was surfaced.
    pub reason: String,
}

impl SkillSearchHit {
    /// Construct a search hit.
    pub fn new(
        skill_id: impl Into<String>,
        score: u32,
        matched_terms: Vec<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            skill_id: skill_id.into(),
            score,
            matched_terms,
            reason: reason.into(),
        }
    }
}

/// Tokenize a text string into lowercase alphanumeric terms.
pub(crate) fn tokenize(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .filter(|token| token.len() > 1)
}

/// Build a stable hash for a payload.
pub fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}
