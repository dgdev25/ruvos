//! Compression artifact helpers stored inside `.rvf` session state.

use crate::{read_session, write_session, Session};

const PREFIX: &str = "compress.original.";
const METADATA_PREFIX: &str = "compress.meta.";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CompressionArtifact {
    pub reference: String,
    pub content_type: String,
    pub bytes_before: usize,
    pub bytes_after: usize,
    pub original: String,
}

fn payload_key(reference: &str) -> String {
    format!("{PREFIX}{reference}")
}

fn meta_key(reference: &str) -> String {
    format!("{METADATA_PREFIX}{reference}")
}

fn strip_prefix<'a>(prefix: &str, key: &'a str) -> Option<&'a str> {
    key.strip_prefix(prefix)
}

pub fn store_original_in_session(session: &mut Session, reference: &str, original: &str) {
    session.state.insert(
        payload_key(reference),
        serde_json::to_string(original).unwrap_or_default(),
    );
}

pub fn retrieve_original_from_session(session: &Session, reference: &str) -> Option<String> {
    session
        .state
        .get(&payload_key(reference))
        .and_then(|raw| serde_json::from_str::<String>(raw).ok())
}

pub fn record_metadata_in_session(
    session: &mut Session,
    reference: &str,
    content_type: &str,
    bytes_before: usize,
    bytes_after: usize,
) {
    let meta = serde_json::json!({
        "content_type": content_type,
        "bytes_before": bytes_before,
        "bytes_after": bytes_after,
    });
    session.state.insert(meta_key(reference), meta.to_string());
}

pub async fn persist_original_to_session(
    session_path: &str,
    reference: &str,
    original: &str,
    content_type: &str,
    bytes_before: usize,
    bytes_after: usize,
) -> anyhow::Result<()> {
    let mut session = read_session(session_path).await?;
    store_original_in_session(&mut session, reference, original);
    record_metadata_in_session(
        &mut session,
        reference,
        content_type,
        bytes_before,
        bytes_after,
    );
    session.updated_at = chrono::Utc::now().to_rfc3339();
    write_session(&session, session_path).await?;
    Ok(())
}

pub async fn load_original_from_session(
    session_path: &str,
    reference: &str,
) -> anyhow::Result<Option<String>> {
    let session = read_session(session_path).await?;
    Ok(retrieve_original_from_session(&session, reference))
}

pub async fn load_compression_artifact_from_session(
    session_path: &str,
    reference: &str,
) -> anyhow::Result<Option<CompressionArtifact>> {
    let session = read_session(session_path).await?;
    let original = match retrieve_original_from_session(&session, reference) {
        Some(original) => original,
        None => return Ok(None),
    };
    let metadata = match session.state.get(&meta_key(reference)) {
        Some(raw) => serde_json::from_str::<serde_json::Value>(raw).unwrap_or_default(),
        None => serde_json::Value::Object(Default::default()),
    };
    Ok(Some(CompressionArtifact {
        reference: reference.to_string(),
        content_type: metadata
            .get("content_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        bytes_before: metadata
            .get("bytes_before")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        bytes_after: metadata
            .get("bytes_after")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        original,
    }))
}

pub async fn list_compression_artifacts_in_session(
    session_path: &str,
) -> anyhow::Result<Vec<CompressionArtifact>> {
    let session = read_session(session_path).await?;
    let mut artifacts = Vec::new();

    for key in session.state.keys() {
        let Some(reference) = strip_prefix(PREFIX, key) else {
            continue;
        };
        if let Some(artifact) =
            load_compression_artifact_from_session(session_path, reference).await?
        {
            artifacts.push(artifact);
        }
    }

    artifacts.sort_by(|a, b| a.reference.cmp(&b.reference));
    Ok(artifacts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{write_session, Session};

    #[tokio::test]
    async fn lists_and_loads_compression_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let session_path = dir.path().join("artifact.rvf");
        let mut session = Session::new();
        session.name = "compress-artifact-test".into();
        session.rvf_path = session_path.to_string_lossy().into_owned();
        store_original_in_session(&mut session, "abc123", "original payload");
        record_metadata_in_session(&mut session, "abc123", "text", 100, 42);
        write_session(&session, &session.rvf_path).await.unwrap();

        let loaded = load_compression_artifact_from_session(&session.rvf_path, "abc123")
            .await
            .unwrap()
            .expect("artifact");
        assert_eq!(loaded.reference, "abc123");
        assert_eq!(loaded.original, "original payload");
        assert_eq!(loaded.content_type, "text");
        assert_eq!(loaded.bytes_before, 100);
        assert_eq!(loaded.bytes_after, 42);

        let list = list_compression_artifacts_in_session(&session.rvf_path)
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].reference, "abc123");
    }
}
