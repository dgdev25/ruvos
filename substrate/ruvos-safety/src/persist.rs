use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};

/// Write `value` to `path` atomically: serialize to a sibling `.tmp` file,
/// then rename over the target.  Rename is atomic on POSIX; on Windows it is
/// as atomic as the OS allows (replaces the destination file in a single
/// system call since Rust 1.65 on NTFS).
pub fn atomic_write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create dirs for {}", parent.display()))?;

    let tmp: PathBuf = path.with_extension("tmp");
    let json = serde_json::to_string_pretty(value).context("serialize to JSON")?;
    std::fs::write(&tmp, json).with_context(|| format!("write temp file {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} → {}", tmp.display(), path.display()))?;
    Ok(())
}

/// Load and deserialize `T` from `path`.  Returns `Ok(None)` when the file
/// does not exist (first run / fresh state).
pub fn load_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let value: T = serde_json::from_slice(&bytes)
        .with_context(|| format!("deserialize {}", path.display()))?;
    Ok(Some(value))
}
