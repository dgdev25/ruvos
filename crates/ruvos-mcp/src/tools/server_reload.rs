//! `ruvos_server_reload` — replace the running MCP server process in-place (ADR-033).
//!
//! Uses execve(2) to atomically replace the current process image with the newly
//! installed binary at the same path and with the same argv.  The MCP session
//! (stdin/stdout) is inherited by the new image so the client sees no disconnect.
//! The call never returns on success; on failure it returns an error JSON.
//!
//! ## Resolution robustness (the `(deleted)` problem)
//!
//! `install_binary` installs via atomic rename (`cp tmp && mv tmp dest`). That
//! replaces the inode at `dest`, so the still-running server's `/proc/self/exe`
//! — i.e. `std::env::current_exe()` — now reports `dest (deleted)`, a path that
//! does not resolve. A naive `exec(current_exe())` then fails with ENOENT.
//!
//! `resolve_reload_target` handles this: it strips the ` (deleted)` marker, and
//! if the resulting path is not a real file it falls back to an explicit caller
//! override, then to known install locations (`~/.local/bin`, `~/.cargo/bin`),
//! then to a PATH scan for the binary's own file name.

use crate::tools::handler::{ExecuteFuture, ToolHandler};
use crate::Result;
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct ServerReloadHandler;

impl ToolHandler for ServerReloadHandler {
    fn name(&self) -> &'static str {
        "reload"
    }

    fn domain(&self) -> &'static str {
        "server"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "binary_path": {
                    "type": "string",
                    "description": "Optional explicit path to the binary to exec. Use when current_exe() is stale (e.g. after an atomic-rename install marks /proc/self/exe as '(deleted)'). When omitted, ruvos resolves the target automatically."
                }
            },
            "required": []
        })
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move { reload_server(params) })
    }
}

/// Strip a trailing ` (deleted)` marker that Linux appends to `/proc/self/exe`
/// when the running binary's inode has been replaced (e.g. by an atomic rename).
fn strip_deleted_marker(path_str: &str) -> &str {
    path_str.strip_suffix(" (deleted)").unwrap_or(path_str)
}

/// `current_exe()` with the `(deleted)` marker removed. The returned path may or
/// may not exist — callers must check `is_file()`.
fn cleaned_current_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let s = exe.to_string_lossy();
    Some(PathBuf::from(strip_deleted_marker(&s)))
}

/// Best-effort file name of the running binary (defaults to "ruvos").
fn binary_file_name() -> String {
    cleaned_current_exe()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "ruvos".to_string())
}

/// Scan `$PATH` for an executable by name.
fn which_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// Resolve the binary to exec, in priority order:
/// 1. caller-supplied `explicit` path (if it is a real file),
/// 2. `current_exe()` with `(deleted)` stripped (if it is a real file),
/// 3. known install locations `~/.local/bin/<name>`, `~/.cargo/bin/<name>`,
/// 4. a `$PATH` scan for `<name>`.
///
/// Returns `None` only when nothing resolvable is found anywhere.
fn resolve_reload_target(explicit: Option<&str>) -> Option<PathBuf> {
    // 1. Explicit override.
    if let Some(p) = explicit {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }

    // 2. Cleaned current_exe.
    if let Some(exe) = cleaned_current_exe() {
        if exe.is_file() {
            return Some(exe);
        }
    }

    // 3. Known install locations.
    let name = binary_file_name();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        for rel in [".local/bin", ".cargo/bin"] {
            let cand = home.join(rel).join(&name);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }

    // 4. PATH scan.
    which_in_path(&name)
}

fn reload_server(params: Value) -> Result<Value> {
    use std::os::unix::process::CommandExt;

    let explicit = params.get("binary_path").and_then(|v| v.as_str());
    let exe = resolve_reload_target(explicit).ok_or_else(|| {
        crate::RuvosError::InternalError(format!(
            "server_reload: could not resolve a runnable binary. current_exe={:?} (may be marked deleted after an atomic install); \
             tried ~/.local/bin, ~/.cargo/bin, and $PATH for '{}'. Pass binary_path explicitly.",
            std::env::current_exe().ok(),
            binary_file_name(),
        ))
    })?;

    // Collect all argv[1..] so the new image inherits the same subcommands/flags.
    let args: Vec<String> = std::env::args().skip(1).collect();

    // .exec() replaces this process image; it only returns if execve fails.
    let err = std::process::Command::new(&exe).args(&args).exec();

    Err(crate::RuvosError::InternalError(format!(
        "execve failed for {}: {err}",
        exe.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_deleted_marker_removes_suffix() {
        assert_eq!(
            strip_deleted_marker("/home/lyle/.local/bin/ruvos (deleted)"),
            "/home/lyle/.local/bin/ruvos"
        );
    }

    #[test]
    fn strip_deleted_marker_leaves_clean_path_untouched() {
        assert_eq!(
            strip_deleted_marker("/usr/bin/ruvos"),
            "/usr/bin/ruvos"
        );
    }

    #[test]
    fn resolve_prefers_existing_explicit_path() {
        // A real file (this test binary's own dir has files; use a temp file).
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("fake-ruvos");
        std::fs::write(&f, b"#!/bin/sh\n").unwrap();
        let got = resolve_reload_target(Some(f.to_str().unwrap()));
        assert_eq!(got.as_deref(), Some(f.as_path()));
    }

    #[test]
    fn resolve_ignores_nonexistent_explicit_and_falls_through() {
        // A bogus explicit path must not be returned; resolution falls through
        // to current_exe() (the test runner binary, which is a real file).
        let bogus = "/nonexistent/path/to/ruvos (deleted)";
        let got = resolve_reload_target(Some(bogus));
        assert_ne!(
            got.as_deref(),
            Some(std::path::Path::new(bogus)),
            "must not return a non-existent explicit path"
        );
        // The fallback should find SOMETHING runnable (the test binary itself).
        assert!(got.is_some(), "expected fallback to current_exe of the test runner");
    }
}
