use crate::types::{HookKind, HookOutcome};
use anyhow::Result;

/// SONA learning integration bridge (Phase 5 implementation).
///
/// Records hook outcomes for learning and pattern recognition.
/// Phase 4 stub logs outcomes; Phase 5 integrates with real SONA embeddings.
#[derive(Debug, Clone)]
pub struct SonaLearningBridge;

impl SonaLearningBridge {
    /// Create a new SONA learning bridge.
    pub fn new() -> Self {
        SonaLearningBridge
    }

    /// Record a hook outcome for learning.
    ///
    /// In Phase 4, this logs the outcome to tracing.
    /// In Phase 5, this will forward to SONA for embedding and pattern storage.
    pub fn record_outcome(&self, kind: HookKind, outcome: &HookOutcome) -> Result<()> {
        tracing::info!(
            hook_kind = %kind.as_str(),
            success = outcome.success,
            message = ?outcome.message,
            "SONA learning: recording hook outcome (Phase 5 integration pending)"
        );
        Ok(())
    }
}

impl Default for SonaLearningBridge {
    fn default() -> Self {
        Self::new()
    }
}
