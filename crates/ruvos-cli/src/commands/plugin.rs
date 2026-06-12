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

/// Install a plugin: fetch, require checksum, optional HMAC via
/// RUVOS_PLUGIN_KEY, unpack into `<dest_root>/<name>`.
pub async fn run_install(name: &str, source: &str, dest_root: &std::path::Path) -> Result<()> {
    let fetched = fetch(source).await?;
    let Some(sha256) = fetched.sha256.as_deref() else {
        bail!(
            "no .sha256 sidecar found for {source} — a checksum is required \
             (publish <tarball>.sha256 next to the tarball)"
        );
    };
    let key = std::env::var("RUVOS_PLUGIN_KEY").ok();
    let signature = match (fetched.signature.as_deref(), key.as_deref()) {
        (Some(sig), Some(key)) => Some((sig, key.as_bytes())),
        (Some(_), None) => {
            eprintln!(
                "warning: tarball has a .sig but RUVOS_PLUGIN_KEY is not set — signature NOT verified"
            );
            None
        }
        _ => None,
    };
    let installed = ruvos_plugin_host::install::install_tarball(
        &fetched.tarball,
        sha256,
        signature,
        name,
        dest_root,
    )?;
    println!("✓ Installed plugin '{name}' at {}", installed.display());
    println!("  Discoverable via ruvos_plugin_list immediately.");
    Ok(())
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

    /// Build a gzipped tar containing a minimal valid plugin.
    fn make_plugin_tarball(dir: &std::path::Path) -> std::path::PathBuf {
        let tar_path = dir.join("demo.tar.gz");
        let file = std::fs::File::create(&tar_path).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);

        let manifest = br#"
[plugin]
name = "demo"
version = "1.0.0"
description = "demo plugin"
license = "MIT"
authors = ["t"]

[capabilities]
"#;
        let mut header = tar::Header::new_gnu();
        header.set_size(manifest.len() as u64);
        header.set_mode(0o644);
        header.set_path("plugin.toml").unwrap();
        header.set_cksum();
        builder.append(&header, &manifest[..]).unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        tar_path
    }

    fn sha256_file(p: &std::path::Path) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(std::fs::read(p).unwrap()))
    }

    #[tokio::test]
    async fn install_command_end_to_end_local() {
        let work = tempfile::tempdir().unwrap();
        let tarball = make_plugin_tarball(work.path());
        let digest = sha256_file(&tarball);
        std::fs::write(
            format!("{}.sha256", tarball.display()),
            format!("{digest}\n"),
        )
        .unwrap();

        let dest_root = work.path().join("plugins");
        run_install("demo", &tarball.to_string_lossy(), &dest_root)
            .await
            .unwrap();
        assert!(dest_root.join("demo/plugin.toml").exists());

        // missing checksum sidecar must be a hard error
        let bare = work.path().join("bare.tar.gz");
        std::fs::copy(&tarball, &bare).unwrap();
        let err = run_install("bare", &bare.to_string_lossy(), &dest_root)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("sha256"));
    }

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
