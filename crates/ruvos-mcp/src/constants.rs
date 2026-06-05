//! Shared policy constants for MCP tools and runtime helpers.

/// Default event query limit.
pub const DEFAULT_EVENT_LIMIT: usize = 50;

/// Maximum number of retries when opening the shared store.
pub const STORE_MAX_TRIES: u32 = 12;

/// Base backoff delay in milliseconds for store lock contention.
pub const STORE_BASE_DELAY_MS: u64 = 20;

/// BM25 term saturation constant.
pub const RETRIEVAL_K1: f32 = 1.2;

/// BM25 document-length normalization constant.
pub const RETRIEVAL_B: f32 = 0.75;

/// Reciprocal Rank Fusion constant.
pub const RETRIEVAL_RRF_K: f32 = 60.0;

/// Default number of SONA clusters for `intel` pattern clustering.
pub const INTEL_SONA_K_CLUSTERS: usize = 8;

/// Default minimum cluster size for `intel` pattern clustering.
pub const INTEL_SONA_MIN_CLUSTER_SIZE: usize = 1;

/// Default quality threshold for `intel` pattern clustering.
pub const INTEL_SONA_QUALITY_THRESHOLD: f32 = 0.05;

/// Heuristic boost when a pattern is already surfaced by SONA.
pub const INTEL_SONA_BOOST: f64 = 0.1;

/// Heuristic boost for tag overlap in intent search.
pub const INTENT_TAG_OVERLAP_BOOST: f64 = 0.1;

/// Heuristic boost when the query kind matches an intent kind.
pub const INTEL_KIND_BOOST: f64 = 0.2;

/// Confidence contribution scale for intent ranking.
pub const INTENT_CONFIDENCE_SCALE: f64 = 0.01;

/// Default `top_k` for intent search.
pub const DEFAULT_INTENT_TOP_K: usize = 5;

/// Default `top_k` for generic intel pattern search.
pub const DEFAULT_INTEL_TOP_K: usize = 5;

/// Default `top_k` for memory search.
pub const DEFAULT_MEMORY_TOP_K: usize = 5;

/// Default limit for replay history queries.
pub const GOV_REPLAY_LIMIT: usize = 200;

/// Default limit for swarm history queries.
pub const GOV_SWARM_HISTORY_LIMIT: usize = 25;

/// Default number of learner clusters for the swarm engine.
pub const SWARM_LEARNER_CLUSTERS: usize = 8;

/// Topology score used for mesh swarm embeddings.
pub const SWARM_TOPOLOGY_MESH_SCORE: f32 = 0.8;

/// Topology score used for hybrid swarm embeddings.
pub const SWARM_TOPOLOGY_HYBRID_SCORE: f32 = 0.6;

/// Topology score used for adaptive swarm embeddings.
pub const SWARM_TOPOLOGY_ADAPTIVE_SCORE: f32 = 0.9;

/// Topology score used when the topology is unknown.
pub const SWARM_TOPOLOGY_DEFAULT_SCORE: f32 = 0.3;

/// Default score used when finalizing a stored trajectory.
pub const SWARM_TRAJECTORY_FINALIZE_SCORE: f32 = 0.8;

/// Default `max_agents` for swarm creation.
pub const SWARM_CREATE_DEFAULT_MAX_AGENTS: u32 = 6;

/// Member-count threshold that nudges topology inference toward hybrid.
pub const SWARM_HYBRID_MEMBER_THRESHOLD: usize = 6;

/// `max_agents` threshold that nudges topology inference toward hybrid.
pub const SWARM_HYBRID_MAX_AGENTS_THRESHOLD: u32 = 8;

/// Namespace used to persist compression learning signals in the memory store.
pub const COMPRESSION_SIGNAL_NAMESPACE: &str = "compress";

/// Intent kind used when persisting compression learning signals.
pub const COMPRESSION_SIGNAL_KIND: &str = "compress.outcome";
