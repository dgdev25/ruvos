//! Writes ruvos hook bindings into .claude/settings.json (merge, idempotent).
//!
//! Bindings make the Claude Code harness invoke `ruvos hook <kind>` on
//! PreToolUse(Edit|Write), PostToolUse(Edit|Write), PreToolUse(Bash),
//! PostToolUse(Bash), SessionStart, and Stop — so the learning loop fires
//! mechanically. User-defined entries are preserved; re-running is a no-op.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

/// (event, matcher, ruvos hook command) — matcher None for lifecycle events.
fn bindings(binary: &str) -> Vec<(&'static str, Option<&'static str>, String)> {
    vec![
        (
            "PreToolUse",
            Some("Edit|Write"),
            format!("{binary} hook edit --phase pre"),
        ),
        (
            "PostToolUse",
            Some("Edit|Write"),
            format!("{binary} hook edit --phase post"),
        ),
        (
            "PreToolUse",
            Some("Bash"),
            format!("{binary} hook command --phase pre"),
        ),
        (
            "PostToolUse",
            Some("Bash"),
            format!("{binary} hook command --phase post"),
        ),
        (
            "SessionStart",
            None,
            format!("{binary} hook session --phase pre"),
        ),
        ("Stop", None, format!("{binary} hook session --phase post")),
    ]
}

pub fn write_hook_bindings(settings_path: &Path, binary: &str) -> Result<()> {
    let mut root: Value = match std::fs::read_to_string(settings_path) {
        Ok(raw) => serde_json::from_str(&raw)
            .with_context(|| format!("{} is not valid JSON", settings_path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(e).with_context(|| format!("reading {}", settings_path.display())),
    };

    let hooks = root
        .as_object_mut()
        .context("settings.json root must be an object")?
        .entry("hooks")
        .or_insert(json!({}));

    for (event, matcher, command) in bindings(binary) {
        let entries = hooks
            .as_object_mut()
            .context("hooks must be an object")?
            .entry(event)
            .or_insert(json!([]));
        let arr = entries
            .as_array_mut()
            .context("hook event must be an array")?;
        let already = arr.iter().any(|e| {
            e["hooks"]
                .as_array()
                .map(|hs| hs.iter().any(|h| h["command"] == json!(command)))
                .unwrap_or(false)
        });
        if already {
            continue;
        }
        let mut entry = json!({
            "hooks": [{ "type": "command", "command": command }]
        });
        if let Some(m) = matcher {
            entry["matcher"] = json!(m);
        }
        arr.push(entry);
    }

    ruvos_mcp::paths::atomic_write(
        settings_path,
        serde_json::to_string_pretty(&root)?.as_bytes(),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_settings_with_hook_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".claude/settings.json");
        write_hook_bindings(&path, "ruvos").unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let pre = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert!(pre.iter().any(|e| e["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("ruvos hook")));
        assert!(v["hooks"]["SessionStart"].is_array());
    }

    #[test]
    fn merge_preserves_existing_user_hooks_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".claude/settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"my-own-thing"}]}]},"env":{"FOO":"1"}}"#,
        )
        .unwrap();

        write_hook_bindings(&path, "ruvos").unwrap();
        write_hook_bindings(&path, "ruvos").unwrap(); // second run: no duplicates

        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["env"]["FOO"], "1");
        let pre = v["hooks"]["PreToolUse"].as_array().unwrap();
        let ruvos_entries = pre
            .iter()
            .filter(|e| {
                e["hooks"][0]["command"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ruvos hook")
            })
            .count();
        let user_entries = pre
            .iter()
            .filter(|e| e["hooks"][0]["command"] == "my-own-thing")
            .count();
        assert_eq!(user_entries, 1);
        assert!(ruvos_entries >= 1);
        // idempotent: exactly one copy of each ruvos entry
        assert_eq!(pre.len(), 1 + ruvos_entries);
    }

    #[test]
    fn invalid_json_errors_and_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".claude/settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();

        let err = write_hook_bindings(&path, "ruvos");
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("not valid JSON"));
        // the broken file must be left byte-identical
        assert_eq!(std::fs::read(&path).unwrap(), b"{ not json");
    }

    #[test]
    fn wrong_type_hooks_field_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".claude/settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = r#"{"hooks": "oops"}"#;
        std::fs::write(&path, original).unwrap();

        let err = write_hook_bindings(&path, "ruvos");
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("hooks must be an object"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }
}
