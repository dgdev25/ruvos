//! Plugin tarball installation: checksum + optional HMAC verification,
//! traversal-safe unpack, atomic move into the plugins directory.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

/// Verify and install `tarball` as plugin `name` under `dest_root`.
///
/// - `expected_sha256`: required hex digest of the tarball bytes.
/// - `signature`: optional `(hex_hmac_sha256, key_bytes)` over the tarball.
///
/// Returns the installed plugin directory.
pub fn install_tarball(
    tarball: &Path,
    expected_sha256: &str,
    signature: Option<(&str, &[u8])>,
    name: &str,
    dest_root: &Path,
) -> Result<PathBuf> {
    let dest = dest_root.join(name);
    if dest.exists() {
        bail!(
            "plugin '{name}' is already installed at {} (remove it first)",
            dest.display()
        );
    }

    let bytes = std::fs::read(tarball).with_context(|| format!("reading {}", tarball.display()))?;

    let actual = hex::encode(Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(expected_sha256.trim()) {
        bail!("checksum mismatch: expected {expected_sha256}, got {actual}");
    }

    if let Some((sig_hex, key)) = signature {
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<Sha256>::new_from_slice(key).context("invalid signing key")?;
        mac.update(&bytes);
        let sig = hex::decode(sig_hex.trim()).context("signature is not hex")?;
        mac.verify_slice(&sig).map_err(|_| {
            anyhow::anyhow!("signature verification failed for {}", tarball.display())
        })?;
    }

    // Unpack into a sibling temp dir, validating every entry path, then
    // atomically rename into place.
    std::fs::create_dir_all(dest_root)?;
    let staging = dest_root.join(format!(".{name}.staging.{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;

    let result = unpack_checked(&bytes, &staging);
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    if !staging.join("plugin.toml").exists() {
        let _ = std::fs::remove_dir_all(&staging);
        bail!("tarball does not contain plugin.toml at its root — not a plugin archive");
    }

    std::fs::rename(&staging, &dest).with_context(|| {
        let _ = std::fs::remove_dir_all(&staging);
        format!("installing into {}", dest.display())
    })?;
    Ok(dest)
}

fn unpack_checked(tarball_bytes: &[u8], staging: &Path) -> Result<()> {
    use tar::EntryType;
    let gz = flate2::read::GzDecoder::new(tarball_bytes);
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        // Reject symlink/hardlink entries outright — they can point outside
        // the staging directory or alias later entries to arbitrary targets.
        let kind = entry.header().entry_type();
        if matches!(kind, EntryType::Symlink | EntryType::Link) {
            bail!(
                "archive entry '{}' is a {:?} link — plugins may not contain links",
                path.display(),
                kind
            );
        }
        // Reject absolute paths and any `..` component.
        if path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
        {
            bail!(
                "archive entry '{}' escapes the plugin directory",
                path.display()
            );
        }
        // Defense-in-depth: unpack_in re-validates containment within staging.
        entry.unpack_in(staging)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a gzipped tar containing a minimal valid plugin.
    fn make_plugin_tarball(dir: &std::path::Path, evil: bool) -> std::path::PathBuf {
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
        if evil {
            // tar::Builder refuses `..` paths, so write the name bytes directly.
            let name = b"../escape/plugin.toml";
            header.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name);
        } else {
            header.set_path("plugin.toml").unwrap();
        }
        header.set_cksum();
        builder.append(&header, &manifest[..]).unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        tar_path
    }

    fn sha256_file(p: &std::path::Path) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(std::fs::read(p).unwrap()))
    }

    #[test]
    fn installs_valid_tarball() {
        let work = tempfile::tempdir().unwrap();
        let tarball = make_plugin_tarball(work.path(), false);
        let checksum = sha256_file(&tarball);
        let dest_root = work.path().join("plugins");

        let installed = install_tarball(&tarball, &checksum, None, "demo", &dest_root).unwrap();
        assert_eq!(installed, dest_root.join("demo"));
        assert!(installed.join("plugin.toml").exists());
    }

    #[test]
    fn rejects_bad_checksum() {
        let work = tempfile::tempdir().unwrap();
        let tarball = make_plugin_tarball(work.path(), false);
        let err = install_tarball(
            &tarball,
            &"0".repeat(64),
            None,
            "demo",
            &work.path().join("plugins"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("checksum"));
    }

    #[test]
    fn rejects_path_traversal_entries() {
        let work = tempfile::tempdir().unwrap();
        let tarball = make_plugin_tarball(work.path(), true);
        let checksum = sha256_file(&tarball);
        let err = install_tarball(
            &tarball,
            &checksum,
            None,
            "demo",
            &work.path().join("plugins"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("escapes"));
        assert!(!work.path().join("escape").exists());
    }

    /// Build a gzipped tar containing a symlink entry pointing outside the
    /// staging directory.
    fn make_symlink_tarball(dir: &std::path::Path) -> std::path::PathBuf {
        let tar_path = dir.join("link.tar.gz");
        let file = std::fs::File::create(&tar_path).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);

        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_path("evil-link").unwrap();
        // tar::Header link-name setters can refuse `..` targets, so write the
        // bytes directly (same raw-header technique as the evil-entry test).
        let link = b"../escape";
        header.as_gnu_mut().unwrap().linkname[..link.len()].copy_from_slice(link);
        header.set_cksum();
        builder.append(&header, std::io::empty()).unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        tar_path
    }

    #[test]
    fn rejects_symlink_entries() {
        let work = tempfile::tempdir().unwrap();
        let tarball = make_symlink_tarball(work.path());
        let checksum = sha256_file(&tarball);
        let dest_root = work.path().join("plugins");
        let err = install_tarball(&tarball, &checksum, None, "demo", &dest_root).unwrap_err();
        assert!(err.to_string().contains("link"), "got: {err}");
        // Nothing created outside staging, and staging itself was cleaned up.
        assert!(!work.path().join("escape").exists());
        assert!(!dest_root.join("demo").exists());
        assert!(!dest_root.join("demo/evil-link").exists());
    }

    #[test]
    fn verifies_hmac_signature_when_key_given() {
        use hmac::{Hmac, Mac};
        let work = tempfile::tempdir().unwrap();
        let tarball = make_plugin_tarball(work.path(), false);
        let checksum = sha256_file(&tarball);

        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(b"secret").unwrap();
        mac.update(&std::fs::read(&tarball).unwrap());
        let sig = hex::encode(mac.finalize().into_bytes());

        // good signature installs
        install_tarball(
            &tarball,
            &checksum,
            Some((&sig, b"secret")),
            "demo",
            &work.path().join("p1"),
        )
        .unwrap();
        // bad signature rejected
        let err = install_tarball(
            &tarball,
            &checksum,
            Some((&"00".repeat(32), b"secret")),
            "demo",
            &work.path().join("p2"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("signature"));
    }

    #[test]
    fn refuses_to_overwrite_existing_plugin_dir() {
        let work = tempfile::tempdir().unwrap();
        let tarball = make_plugin_tarball(work.path(), false);
        let checksum = sha256_file(&tarball);
        let dest_root = work.path().join("plugins");
        std::fs::create_dir_all(dest_root.join("demo")).unwrap();
        let err = install_tarball(&tarball, &checksum, None, "demo", &dest_root).unwrap_err();
        assert!(err.to_string().contains("already installed"));
    }
}
