use crate::ccr::{original_reference, store_original};
use crate::defaults::{
    KEEP_HEAD_LINES, KEEP_TAIL_LINES, MAX_ARRAY_ITEMS, MAX_CODE_LINES, MAX_LOG_LINES,
    MAX_TEXT_LINES, MIN_BYTES,
};
use crate::detect::{detect_content_type, ContentKind};
use crate::json::compress_json;
use crate::text::{compress_code, compress_text};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CompressionConfig {
    pub min_bytes: usize,
    pub keep_head_lines: usize,
    pub keep_tail_lines: usize,
    pub max_array_items: usize,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            min_bytes: MIN_BYTES,
            keep_head_lines: KEEP_HEAD_LINES,
            keep_tail_lines: KEEP_TAIL_LINES,
            max_array_items: MAX_ARRAY_ITEMS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    pub original: String,
    pub compressed: String,
    pub kind: ContentKind,
    pub changed: bool,
    pub original_bytes: usize,
    pub compressed_bytes: usize,
    pub bytes_saved: usize,
    pub compression_ratio: f64,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub original_ref: Option<String>,
}

fn estimate_tokens(text: &str) -> usize {
    text.split_whitespace().count().max(1)
}

fn kind_label(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Json => "json",
        ContentKind::Code => "code",
        ContentKind::Log => "log",
        ContentKind::Text => "text",
    }
}

pub fn compress_content(
    content: &str,
    kind_hint: Option<ContentKind>,
    config: CompressionConfig,
) -> CompressionResult {
    let original = content.to_string();
    let original_bytes = original.len();
    let original_ref = Some(original_reference(content));
    let kind = kind_hint.unwrap_or_else(|| detect_content_type(content));

    let compressed = if original_bytes < config.min_bytes {
        original.clone()
    } else {
        match kind {
            ContentKind::Json => {
                compress_json(content, config.max_array_items).unwrap_or_else(|| original.clone())
            }
            ContentKind::Code => compress_code(content, MAX_CODE_LINES),
            ContentKind::Log => compress_text(
                content,
                config.keep_head_lines,
                config.keep_tail_lines,
                MAX_LOG_LINES,
            ),
            ContentKind::Text => compress_text(
                content,
                config.keep_head_lines,
                config.keep_tail_lines,
                MAX_TEXT_LINES,
            ),
        }
    };

    let changed = compressed != original;
    let compressed_bytes = compressed.len();
    let bytes_saved = original_bytes.saturating_sub(compressed_bytes);
    let compression_ratio = if original_bytes == 0 {
        1.0
    } else {
        compressed_bytes as f64 / original_bytes as f64
    };
    let tokens_before = estimate_tokens(&original);
    let tokens_after = estimate_tokens(&compressed);

    if changed {
        let _ = store_original(&original);
    }

    CompressionResult {
        original,
        compressed,
        kind,
        changed,
        original_bytes,
        compressed_bytes,
        bytes_saved,
        compression_ratio,
        tokens_before,
        tokens_after,
        original_ref: if changed { original_ref } else { None },
    }
}

pub async fn compress_content_into_session(
    content: &str,
    kind_hint: Option<ContentKind>,
    config: CompressionConfig,
    session_path: Option<&str>,
) -> anyhow::Result<CompressionResult> {
    let result = compress_content(content, kind_hint, config);
    if result.changed {
        if let Some(path) = session_path {
            if let Some(reference) = &result.original_ref {
                ruvos_session::compress::persist_original_to_session(
                    path,
                    reference,
                    &result.original,
                    kind_label(result.kind),
                    result.original_bytes,
                    result.compressed_bytes,
                )
                .await?;
            }
        }
    }
    Ok(result)
}
