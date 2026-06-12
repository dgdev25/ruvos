//! `ruvos plugin install` — fetch + verify + install plugin tarballs.

use anyhow::{bail, Result};
use std::path::PathBuf;

pub struct Fetched {
    /// Local path of the tarball (original file or downloaded temp file).
    pub tarball: PathBuf,
    pub sha256: Option<String>,
    pub signature: Option<String>,
    /// Keeps downloaded temp files alive until install completes.
    pub _tmp: Option<tempfile::TempDir>,
}

/// Fetch a tarball plus optional `.sha256` / `.sig` sidecars from a local
/// path or an https URL.
pub async fn fetch(source: &str) -> Result<Fetched> {
    if source.starts_with("https://") || source.starts_with("http://") {
        if source.starts_with("http://") {
            bail!("refusing plaintext http; use https");
        }
        let tmp = tempfile::tempdir()?;
        let tarball = tmp.path().join("plugin.tar.gz");
        let bytes = reqwest::get(source)
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        std::fs::write(&tarball, &bytes)?;
        let sha256 = fetch_sidecar(&format!("{source}.sha256")).await;
        let signature = fetch_sidecar(&format!("{source}.sig")).await;
        Ok(Fetched {
            tarball,
            sha256,
            signature,
            _tmp: Some(tmp),
        })
    } else {
        let tarball = PathBuf::from(source);
        if !tarball.exists() {
            bail!("no such file: {source}");
        }
        let sha256 = read_sidecar(&format!("{source}.sha256"));
        let signature = read_sidecar(&format!("{source}.sig"));
        Ok(Fetched {
            tarball,
            sha256,
            signature,
            _tmp: None,
        })
    }
}

async fn fetch_sidecar(url: &str) -> Option<String> {
    let resp = reqwest::get(url).await.ok()?.error_for_status().ok()?;
    Some(resp.text().await.ok()?.trim().to_string())
}

fn read_sidecar(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fetch_local_reads_tarball_and_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let tarball = dir.path().join("demo.tar.gz");
        std::fs::write(&tarball, b"bytes").unwrap();
        std::fs::write(dir.path().join("demo.tar.gz.sha256"), "abc123\n").unwrap();

        let fetched = fetch(&tarball.to_string_lossy()).await.unwrap();
        assert_eq!(std::fs::read(&fetched.tarball).unwrap(), b"bytes");
        assert_eq!(fetched.sha256.as_deref(), Some("abc123"));
        assert!(fetched.signature.is_none());
    }
}
