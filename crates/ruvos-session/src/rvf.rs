//! `.rvf` container write/read operations — real signed files on disk.

use crate::verify::{sign_payload, verify_container};
use crate::Session;
use serde::{Deserialize, Serialize};

/// On-disk `.rvf` container: a signed envelope around a session payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RvfContainer {
    pub version: String,
    pub payload: Session,
    pub signature: String,
}

/// Write a session to an `.rvf` container at `path`, signing the payload.
/// Creates parent directories as needed and writes atomically (temp + rename).
pub async fn write_session(session: &Session, path: &str) -> anyhow::Result<()> {
    let container = RvfContainer {
        version: "rvf-1".to_string(),
        payload: session.clone(),
        signature: sign_payload(session),
    };
    let bytes = serde_json::to_vec_pretty(&container)?;

    if let Some(parent) = std::path::Path::new(path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = format!("{}.tmp", path);
    tokio::fs::write(&tmp, &bytes).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

/// Read a session from an `.rvf` container, verifying its signature first.
/// Returns an error if the file is missing, malformed, or tampered with.
pub async fn read_session(path: &str) -> anyhow::Result<Session> {
    let bytes = tokio::fs::read(path).await?;
    let container: RvfContainer = serde_json::from_slice(&bytes)?;
    if !verify_container(&container) {
        anyhow::bail!("signature verification failed for {}", path);
    }
    Ok(container.payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_then_read_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.rvf");
        let path_str = path.to_str().unwrap();

        let mut s = Session::new();
        s.name = "roundtrip".into();
        s.rvf_path = path_str.to_string();
        s.state.insert("hello".into(), "\"world\"".into());

        write_session(&s, path_str).await.unwrap();
        assert!(path.exists(), "the .rvf file must actually be created");

        let loaded = read_session(path_str).await.unwrap();
        assert_eq!(loaded, s, "loaded session must equal what was written");
    }

    #[tokio::test]
    async fn tampered_file_fails_to_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.rvf");
        let path_str = path.to_str().unwrap();

        let s = Session::new();
        write_session(&s, path_str).await.unwrap();

        // Corrupt the payload on disk without re-signing.
        let raw = tokio::fs::read_to_string(path_str).await.unwrap();
        let corrupted = raw.replace(&s.id.to_string(), &uuid::Uuid::new_v4().to_string());
        tokio::fs::write(path_str, corrupted).await.unwrap();

        assert!(
            read_session(path_str).await.is_err(),
            "reading a tampered container must error"
        );
    }
}
