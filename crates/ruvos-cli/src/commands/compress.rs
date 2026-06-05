//! `ruvos compress` command.

use compress::{compress_content, CompressionConfig, ContentKind};
use serde::Serialize;
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct CompressCommand {
    pub file: Option<PathBuf>,
    pub kind: Option<String>,
    pub min_bytes: usize,
    pub keep_head_lines: usize,
    pub keep_tail_lines: usize,
    pub max_array_items: usize,
    pub session_id: Option<String>,
    pub raw: bool,
}

#[derive(Debug, Serialize)]
struct Output {
    kind: ContentKind,
    changed: bool,
    original_bytes: usize,
    compressed_bytes: usize,
    bytes_saved: usize,
    compression_ratio: f64,
    tokens_before: usize,
    tokens_after: usize,
    original_ref: Option<String>,
    compressed: String,
}

pub async fn run(command: CompressCommand) -> anyhow::Result<()> {
    let mut input = String::new();
    match command.file {
        Some(path) => {
            input = std::fs::read_to_string(&path)?;
        }
        None => {
            std::io::stdin().read_to_string(&mut input)?;
        }
    }

    let kind = match command.kind.as_deref() {
        Some("json") => Some(ContentKind::Json),
        Some("code") => Some(ContentKind::Code),
        Some("log") => Some(ContentKind::Log),
        Some("text") => Some(ContentKind::Text),
        Some("auto") | None => None,
        Some(other) => anyhow::bail!("invalid kind '{other}'"),
    };

    let session_path = match command.session_id.as_deref() {
        Some(session_id) => {
            uuid::Uuid::parse_str(session_id)?;
            Some(
                ruvos_mcp::paths::sessions_dir()
                    .join(format!("{}.rvf", session_id))
                    .to_string_lossy()
                    .into_owned(),
            )
        }
        None => None,
    };

    let result = if let Some(path) = session_path.as_deref() {
        compress::compress_content_into_session(
            &input,
            kind,
            CompressionConfig {
                min_bytes: command.min_bytes,
                keep_head_lines: command.keep_head_lines,
                keep_tail_lines: command.keep_tail_lines,
                max_array_items: command.max_array_items,
            },
            Some(path),
        )
        .await?
    } else {
        compress_content(
            &input,
            kind,
            CompressionConfig {
                min_bytes: command.min_bytes,
                keep_head_lines: command.keep_head_lines,
                keep_tail_lines: command.keep_tail_lines,
                max_array_items: command.max_array_items,
            },
        )
    };

    if command.raw {
        println!("{}", result.compressed);
    } else {
        let output = Output {
            kind: result.kind,
            changed: result.changed,
            original_bytes: result.original_bytes,
            compressed_bytes: result.compressed_bytes,
            bytes_saved: result.bytes_saved,
            compression_ratio: result.compression_ratio,
            tokens_before: result.tokens_before,
            tokens_after: result.tokens_after,
            original_ref: result.original_ref,
            compressed: result.compressed,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruvos_session::{read_session, write_session, Session};

    #[tokio::test]
    async fn cli_roundtrip_persists_original_into_session() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("RUVOS_HOME", dir.path());
        ruvos_mcp::paths::ensure_root().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let session_path = ruvos_mcp::paths::sessions_dir().join(format!("{}.rvf", session_id));

        let mut session = Session::new();
        session.id = session_id;
        session.rvf_path = session_path.to_string_lossy().into_owned();
        let session_path_str = session.rvf_path.clone();
        write_session(&session, session_path_str.as_str())
            .await
            .unwrap();

        let input_path = dir.path().join("input.txt");
        let input = (0..80)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&input_path, input).unwrap();

        run(CompressCommand {
            file: Some(input_path),
            kind: Some("text".into()),
            min_bytes: 1,
            keep_head_lines: 2,
            keep_tail_lines: 2,
            max_array_items: 12,
            session_id: Some(session_id.to_string()),
            raw: true,
        })
        .await
        .unwrap();

        let loaded: Session = read_session(session_path_str.as_str()).await.unwrap();
        assert!(loaded
            .state
            .keys()
            .any(|key: &String| key.starts_with("compress.original.")));
    }
}
