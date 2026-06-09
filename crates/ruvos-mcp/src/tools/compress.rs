//! Compression tools.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::compress_learning::{record_compression_learning, CompressionLearningSignal};
use crate::paths;
use crate::{Result, RuvosError};
use compress::defaults::{KEEP_HEAD_LINES, KEEP_TAIL_LINES, MAX_ARRAY_ITEMS, MIN_BYTES};
use compress::{compress_content_into_session, CompressionConfig, ContentKind};
use serde_json::{json, Value};
use tracing::warn;

pub struct CompressRunHandler;

impl ToolHandler for CompressRunHandler {
    fn name(&self) -> &'static str {
        "run"
    }

    fn domain(&self) -> &'static str {
        "compress"
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "Text or JSON content to compress"
                },
                "kind": {
                    "type": "string",
                    "enum": ["json", "text", "code"],
                    "description": "Content type hint for compression strategy"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session UUID to attach the compressed output to"
                }
            },
            "required": ["content"]
        })
    }

    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("content").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'content' field (string)".to_string(),
            ));
        }
        if let Some(session_id) = params.get("session_id") {
            if !session_id.is_null() && session_id.as_str().is_none() {
                return Err(RuvosError::InvalidParams(
                    "'session_id' must be a string or null".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let content = params
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RuvosError::InvalidParams("content must be a string".to_string()))?;
            let session_path = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .map(|session_id| {
                    uuid::Uuid::parse_str(session_id).map_err(|_| {
                        RuvosError::InvalidParams("session_id must be a UUID".to_string())
                    })?;
                    Ok::<String, RuvosError>(
                        paths::sessions_dir()
                            .join(format!("{}.rvf", session_id))
                            .to_string_lossy()
                            .into_owned(),
                    )
                })
                .transpose()?;

            let kind = match params.get("kind").and_then(|v| v.as_str()) {
                Some("json") => Some(ContentKind::Json),
                Some("code") => Some(ContentKind::Code),
                Some("log") => Some(ContentKind::Log),
                Some("text") => Some(ContentKind::Text),
                Some("auto") | None => None,
                Some(other) => {
                    return Err(RuvosError::InvalidParams(format!(
                        "invalid kind '{other}' (expected auto|json|code|log|text)"
                    )));
                }
            };

            let config = CompressionConfig {
                min_bytes: params
                    .get("min_bytes")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(MIN_BYTES as u64) as usize,
                keep_head_lines: params
                    .get("keep_head_lines")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(KEEP_HEAD_LINES as u64) as usize,
                keep_tail_lines: params
                    .get("keep_tail_lines")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(KEEP_TAIL_LINES as u64) as usize,
                max_array_items: params
                    .get("max_array_items")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(MAX_ARRAY_ITEMS as u64) as usize,
            };

            let result = if let Some(session_path) = session_path.as_deref() {
                compress_content_into_session(content, kind, config, Some(session_path))
                    .await
                    .map_err(|e| {
                        RuvosError::InternalError(format!("session compression failed: {e}"))
                    })?
            } else {
                compress_content_into_session(content, kind, config, None)
                    .await
                    .map_err(|e| RuvosError::InternalError(format!("compression failed: {e}")))?
            };

            if let Err(error) = record_compression_learning(
                &CompressionLearningSignal::from_result("mcp.tools.call", "compress.run", &result),
            ) {
                warn!("compression learning recording failed for compress.run: {error:?}");
            }

            Ok(json!({
                "kind": result.kind,
                "changed": result.changed,
                "original_bytes": result.original_bytes,
                "compressed_bytes": result.compressed_bytes,
                "bytes_saved": result.bytes_saved,
                "compression_ratio": result.compression_ratio,
                "tokens_before": result.tokens_before,
                "tokens_after": result.tokens_after,
                "original_ref": result.original_ref,
                "compressed": result.compressed,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;
    use ruvos_session::{read_session, write_session, Session};

    #[tokio::test]
    async fn compress_roundtrip_persists_original_into_session() {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        paths::ensure_root().unwrap();

        let mut session = Session::new();
        session.name = "compress-roundtrip".into();
        let session_path = paths::sessions_dir().join(format!("{}.rvf", session.id));
        session.rvf_path = session_path.to_string_lossy().into_owned();
        write_session(&session, &session.rvf_path).await.unwrap();

        let handler = CompressRunHandler;
        let input = (0..80)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let output = handler
            .execute(json!({
                "content": input,
                "kind": "text",
                "session_id": session.id.to_string(),
                "min_bytes": 1,
                "keep_head_lines": 2,
                "keep_tail_lines": 2,
                "max_array_items": 12
            }))
            .await
            .unwrap();

        assert!(output["changed"].as_bool().unwrap());
        let loaded = read_session(&session.rvf_path).await.unwrap();
        let reference = output["original_ref"].as_str().unwrap();
        assert!(
            loaded
                .state
                .contains_key(&format!("compress.original.{reference}")),
            "compressed original must be persisted in the session"
        );
    }
}
