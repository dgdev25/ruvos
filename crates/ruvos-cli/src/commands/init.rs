//! `ruvos init` — bootstrap a project's CLAUDE.md with the ruvos managed block.

use std::path::{Path, PathBuf};

const SENTINEL_START: &str =
    "<!-- ruvos-managed: do not edit this block manually, run `ruvos init` to update -->";
const SENTINEL_END: &str = "<!-- end ruvos-managed -->";

const MANAGED_BLOCK: &str = r#"<!-- ruvos-managed: do not edit this block manually, run `ruvos init` to update -->

## rUvOS (globally registered — no setup needed)

**Before every non-trivial task**, call hooks_pre to trigger auto-swarm,
routing, and safety checks:

```
ruvos_hooks_pre  kind=task  payload={"prompt": "<your task description>"}
```

Use the returned `swarm_id` for all subsequent `ruvos_swarm_assign` calls.
Pass `auto_swarm: false` for single-file fixes.

| Situation | Tool |
|-----------|------|
| Save a decision or pattern for future sessions | `ruvos_memory_store` / `ruvos_memory_search` |
| Fork before a risky change | `ruvos_session_fork` |
| Resume interrupted work | `ruvos_session_resume` |
| Multi-step task with ordered stages | `ruvos_swarm_create` + `ruvos_swarm_assign` |
| Log a significant operation async | `ruvos_hooks_post` |
| Sprint retrospective | `ruvos_gov_sprint_summary` |
| Track a bug or task | `ruvos_gov_issue_create` |

<!-- end ruvos-managed -->"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    Rust,
    Node,
    Python,
    Go,
    Unknown,
}

impl ProjectKind {
    fn detect(dir: &Path) -> Self {
        if dir.join("Cargo.toml").exists() {
            return Self::Rust;
        }
        if dir.join("package.json").exists() {
            return Self::Node;
        }
        if dir.join("pyproject.toml").exists() || dir.join("setup.py").exists() {
            return Self::Python;
        }
        if dir.join("go.mod").exists() {
            return Self::Go;
        }
        Self::Unknown
    }

    fn hint(&self) -> Option<&'static str> {
        match self {
            Self::Rust => Some(
                "Rust project detected. Consider adding to your CLAUDE.md:\n  \
                 - Zero-warnings policy: `cargo clippy --all-targets -- -D warnings`\n  \
                 - File size limit: keep .rs files ≤ 500 lines",
            ),
            Self::Node => Some(
                "Node/TypeScript project detected. Consider adding to your CLAUDE.md:\n  \
                 - All DB calls must be async/await — never block the event loop\n  \
                 - Run `npm run typecheck` before committing",
            ),
            Self::Python => Some(
                "Python project detected. Consider adding to your CLAUDE.md:\n  \
                 - Use ruff for linting: `ruff check .`\n  \
                 - Type-check with mypy or pyright",
            ),
            Self::Go => Some(
                "Go project detected. Consider adding to your CLAUDE.md:\n  \
                 - `go vet ./...` must pass clean\n  \
                 - `gofmt -l .` must produce no output",
            ),
            Self::Unknown => None,
        }
    }
}

/// Result of the init operation.
pub struct InitResult {
    pub claude_md_path: PathBuf,
    pub action: &'static str, // "created" | "updated" | "unchanged"
    pub data_dir_created: bool,
    pub project_kind: ProjectKind,
}

pub async fn init(
    name: Option<String>,
    dry_run: bool,
    force: bool,
    no_data_dir: bool,
) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let project_name = name.unwrap_or_else(|| {
        cwd.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string()
    });

    let kind = ProjectKind::detect(&cwd);
    let claude_md = cwd.join("CLAUDE.md");
    let data_dir = cwd.join(".ruvos");

    // --- CLAUDE.md handling ---
    let (new_content, action) = prepare_claude_md(&claude_md, &project_name, force)?;

    if action != "unchanged" {
        if dry_run {
            println!("[dry-run] Would write CLAUDE.md ({action})");
            if let Some(content) = &new_content {
                println!("--- preview (first 20 lines) ---");
                for line in content.lines().take(20) {
                    println!("{line}");
                }
                println!("...");
            }
        } else if let Some(content) = new_content {
            std::fs::write(&claude_md, content)?;
            println!("✓ CLAUDE.md {action}: {}", claude_md.display());
        }
    } else {
        println!("✓ CLAUDE.md already up to date");
    }

    // --- .ruvos/ data dir ---
    let data_dir_created = if !no_data_dir && !data_dir.exists() {
        if dry_run {
            println!("[dry-run] Would create {}", data_dir.display());
            false
        } else {
            std::fs::create_dir_all(&data_dir)?;
            println!("✓ Created {}", data_dir.display());
            true
        }
    } else {
        false
    };

    // --- language hint ---
    if let Some(hint) = kind.hint() {
        println!("\nHint: {hint}");
    }

    // --- summary ---
    println!("\nProject '{}' is ready to use ruvos.", project_name);
    println!("Run `claude mcp list` to confirm ruvos shows Connected.");

    let _ = InitResult {
        claude_md_path: claude_md,
        action,
        data_dir_created,
        project_kind: kind,
    };

    Ok(())
}

/// Returns (Option<new_content>, action) where action is "created"|"updated"|"unchanged".
fn prepare_claude_md(
    path: &Path,
    project_name: &str,
    force: bool,
) -> anyhow::Result<(Option<String>, &'static str)> {
    if !path.exists() {
        // Create a minimal CLAUDE.md with just the managed block.
        let content = format!("# {project_name}\n\n{MANAGED_BLOCK}\n");
        return Ok((Some(content), "created"));
    }

    let existing = std::fs::read_to_string(path)?;

    // Find existing managed block boundaries.
    let start_pos = existing.find(SENTINEL_START);
    let end_pos = existing.find(SENTINEL_END);

    match (start_pos, end_pos) {
        (Some(s), Some(e)) if e > s => {
            // Block exists — check if it matches current template.
            let block_end = e + SENTINEL_END.len();
            let current_block = &existing[s..block_end];
            if !force && current_block == MANAGED_BLOCK {
                return Ok((None, "unchanged"));
            }
            // Replace in place.
            let new_content = format!(
                "{}{}{}",
                &existing[..s],
                MANAGED_BLOCK,
                &existing[block_end..]
            );
            Ok((Some(new_content), "updated"))
        }
        _ => {
            // No block yet — append.
            let separator = if existing.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            let new_content = format!("{existing}{separator}{MANAGED_BLOCK}\n");
            Ok((Some(new_content), "updated"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn creates_new_file_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("CLAUDE.md");
        let (content, action) = prepare_claude_md(&path, "myproject", false).unwrap();
        assert_eq!(action, "created");
        let content = content.unwrap();
        assert!(content.contains("# myproject"));
        assert!(content.contains(SENTINEL_START));
        assert!(content.contains(SENTINEL_END));
    }

    #[test]
    fn returns_unchanged_when_block_already_current() {
        let existing = format!("# existing\n\n{MANAGED_BLOCK}\n");
        let f = write_temp(&existing);
        let (content, action) = prepare_claude_md(f.path(), "x", false).unwrap();
        assert_eq!(action, "unchanged");
        assert!(content.is_none());
    }

    #[test]
    fn appends_to_existing_file_without_block() {
        let f = write_temp("# MyProject\n\nSome existing content.\n");
        let (content, action) = prepare_claude_md(f.path(), "MyProject", false).unwrap();
        assert_eq!(action, "updated");
        let content = content.unwrap();
        assert!(content.contains("Some existing content."));
        assert!(content.contains(SENTINEL_START));
    }

    #[test]
    fn replaces_stale_block_in_place() {
        let stale = format!(
            "# proj\n\n{}\n\nsome stale content here\n\n{}\n\nMore content after.\n",
            SENTINEL_START, SENTINEL_END
        );
        let f = write_temp(&stale);
        let (content, action) = prepare_claude_md(f.path(), "proj", false).unwrap();
        assert_eq!(action, "updated");
        let content = content.unwrap();
        assert!(!content.contains("some stale content here"));
        assert!(content.contains("More content after."));
        assert!(content.contains(SENTINEL_START));
    }

    #[test]
    fn force_updates_even_when_current() {
        let existing = format!("# existing\n\n{MANAGED_BLOCK}\n");
        let f = write_temp(&existing);
        let (content, action) = prepare_claude_md(f.path(), "x", true).unwrap();
        assert_eq!(action, "updated");
        assert!(content.is_some());
    }

    #[test]
    fn detects_rust_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(ProjectKind::detect(dir.path()), ProjectKind::Rust);
    }

    #[test]
    fn detects_node_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(ProjectKind::detect(dir.path()), ProjectKind::Node);
    }
}
