use crate::constants::{COMPRESSION_SIGNAL_KIND, COMPRESSION_SIGNAL_NAMESPACE};
use crate::runtime::{publish_event, RuntimeEvent};
use crate::tools::{intel, memory};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

/// Summary of a compression outcome to feed into existing learning stores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionLearningSignal {
    pub origin: String,
    pub source: String,
    pub kind: String,
    pub changed: bool,
    pub original_bytes: usize,
    pub compressed_bytes: usize,
    pub bytes_saved: usize,
    pub compression_ratio: f64,
    pub tokens_before: usize,
    pub tokens_after: usize,
}

impl CompressionLearningSignal {
    pub fn from_result(
        origin: impl Into<String>,
        source: impl Into<String>,
        result: &compress::CompressionResult,
    ) -> Self {
        Self {
            origin: origin.into(),
            source: source.into(),
            kind: format!("{:?}", result.kind).to_lowercase(),
            changed: result.changed,
            original_bytes: result.original_bytes,
            compressed_bytes: result.compressed_bytes,
            bytes_saved: result.bytes_saved,
            compression_ratio: result.compression_ratio,
            tokens_before: result.tokens_before,
            tokens_after: result.tokens_after,
        }
    }
}

fn signal_tags(signal: &CompressionLearningSignal) -> Vec<String> {
    vec![
        format!("origin:{}", signal.origin),
        format!("source:{}", signal.source),
        format!("kind:{}", signal.kind),
        format!("changed:{}", signal.changed),
        if signal.changed {
            "signal:useful".to_string()
        } else {
            "signal:neutral".to_string()
        },
    ]
}

fn signal_summary(signal: &CompressionLearningSignal) -> String {
    format!(
        "{}|{}|kind={} changed={} saved={} ratio={:.4} tokens={}→{}",
        signal.origin,
        signal.source,
        signal.kind,
        signal.changed,
        signal.bytes_saved,
        signal.compression_ratio,
        signal.tokens_before,
        signal.tokens_after,
    )
}

fn signal_confidence(signal: &CompressionLearningSignal) -> f64 {
    if !signal.changed {
        return 0.1;
    }
    if signal.original_bytes == 0 {
        return 0.5;
    }
    (signal.bytes_saved as f64 / signal.original_bytes as f64).clamp(0.1, 1.0)
}

/// Persist a compression learning signal into the existing memory and intel
/// stores, then emit a runtime event so it can be traced like any other tool
/// outcome.
pub fn record_compression_learning(signal: &CompressionLearningSignal) -> Result<()> {
    let tags = signal_tags(signal);
    let summary = signal_summary(signal);
    let key = format!("{}:{}:{}", signal.source, signal.kind, Uuid::new_v4());

    memory::record_memory_signal(COMPRESSION_SIGNAL_NAMESPACE, &key, &summary, &tags)?;
    intel::record_intent_signal(
        COMPRESSION_SIGNAL_KIND,
        &summary,
        &tags,
        &signal.origin,
        signal_confidence(signal),
    )?;

    publish_event(RuntimeEvent {
        kind: "compress.learning.recorded".to_string(),
        payload: json!({
            "origin": signal.origin,
            "source": signal.source,
            "kind": signal.kind,
            "changed": signal.changed,
            "bytes_saved": signal.bytes_saved,
            "compression_ratio": signal.compression_ratio,
            "tokens_before": signal.tokens_before,
            "tokens_after": signal.tokens_after,
        }),
        agent_id: None,
        task_id: None,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;
    use compress::{CompressionConfig, ContentKind};

    #[test]
    fn records_learning_signal_into_memory_and_intel() {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        paths::ensure_root().unwrap();

        let result = compress::compress_content(
            &"line\n".repeat(128),
            Some(ContentKind::Text),
            CompressionConfig {
                min_bytes: 1,
                keep_head_lines: 2,
                keep_tail_lines: 2,
                max_array_items: 12,
            },
        );
        let signal =
            CompressionLearningSignal::from_result("mcp.tools.call", "memory.search", &result);
        record_compression_learning(&signal).unwrap();

        let memory_bytes = std::fs::read_to_string(paths::memory_file()).unwrap();
        assert!(memory_bytes.contains(COMPRESSION_SIGNAL_NAMESPACE));
        assert!(memory_bytes.contains("memory.search"));

        let intel_bytes = std::fs::read_to_string(paths::intent_file()).unwrap();
        assert!(intel_bytes.contains(COMPRESSION_SIGNAL_KIND));
        assert!(intel_bytes.contains("mcp.tools.call"));
    }
}
