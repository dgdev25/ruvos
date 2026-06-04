//! Canonical contract manifest generation and verification.
//!
//! This keeps the live tool registry, archetype vocabulary, and hook model in
//! one machine-checkable artifact so docs and CI can fail fast when they drift.

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use ruvos_mcp::tool_registry;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum ContractFormat {
    Json,
    Markdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractManifest {
    pub version: String,
    pub tool_count: usize,
    pub tools: Vec<ContractTool>,
    pub archetypes: Vec<ContractArchetype>,
    pub hooks: Vec<ContractHook>,
    pub workflow_templates: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractTool {
    pub name: String,
    pub domain: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractArchetype {
    pub name: String,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractHook {
    pub name: String,
    pub kind: String,
    pub fires_on: String,
}

pub fn manifest() -> ContractManifest {
    let tools = tool_registry()
        .into_iter()
        .map(|tool| ContractTool {
            name: tool.name,
            domain: tool.domain,
            description: tool.description,
        })
        .collect::<Vec<_>>();

    ContractManifest {
        version: env!("CARGO_PKG_VERSION").to_string(),
        tool_count: tools.len(),
        tools,
        archetypes: vec![
            ContractArchetype {
                name: "coder".to_string(),
                purpose: "Implementation".to_string(),
            },
            ContractArchetype {
                name: "reviewer".to_string(),
                purpose: "Code review, quality, style".to_string(),
            },
            ContractArchetype {
                name: "tester".to_string(),
                purpose: "Test design + execution".to_string(),
            },
            ContractArchetype {
                name: "researcher".to_string(),
                purpose: "Investigation, codebase discovery".to_string(),
            },
            ContractArchetype {
                name: "architect".to_string(),
                purpose: "System / API design".to_string(),
            },
            ContractArchetype {
                name: "planner".to_string(),
                purpose: "Task decomposition, GOAP".to_string(),
            },
            ContractArchetype {
                name: "security".to_string(),
                purpose: "Threat modeling, vuln review".to_string(),
            },
            ContractArchetype {
                name: "perf".to_string(),
                purpose: "Benchmark, profiling, optimization".to_string(),
            },
            ContractArchetype {
                name: "devops".to_string(),
                purpose: "CI/CD, infra, deployment".to_string(),
            },
            ContractArchetype {
                name: "data".to_string(),
                purpose: "Schemas, migrations, queries".to_string(),
            },
            ContractArchetype {
                name: "docs".to_string(),
                purpose: "Documentation, API specs".to_string(),
            },
            ContractArchetype {
                name: "coordinator".to_string(),
                purpose: "Swarm queen / pipeline driver".to_string(),
            },
        ],
        hooks: vec![
            ContractHook {
                name: "pre-task".to_string(),
                kind: "task".to_string(),
                fires_on: "Before any Claude Code task start".to_string(),
            },
            ContractHook {
                name: "post-task".to_string(),
                kind: "task".to_string(),
                fires_on: "After completion (success/fail outcome -> SONA)".to_string(),
            },
            ContractHook {
                name: "pre-edit".to_string(),
                kind: "edit".to_string(),
                fires_on: "Before file write/edit".to_string(),
            },
            ContractHook {
                name: "post-edit".to_string(),
                kind: "edit".to_string(),
                fires_on: "After file write/edit (codemod tier + learning signal)".to_string(),
            },
            ContractHook {
                name: "pre-command".to_string(),
                kind: "command".to_string(),
                fires_on: "Before shell exec (risk assessment)".to_string(),
            },
            ContractHook {
                name: "post-command".to_string(),
                kind: "command".to_string(),
                fires_on: "After shell exec (outcome capture)".to_string(),
            },
            ContractHook {
                name: "session-start".to_string(),
                kind: "session".to_string(),
                fires_on: "Boot - restore session, prime memory".to_string(),
            },
            ContractHook {
                name: "session-end".to_string(),
                kind: "session".to_string(),
                fires_on: "Persist .rvf snapshot, consolidate".to_string(),
            },
        ],
        workflow_templates: vec![
            "feature".to_string(),
            "bugfix".to_string(),
            "refactor".to_string(),
            "security".to_string(),
            "sparc".to_string(),
        ],
    }
}

pub fn render_markdown(manifest: &ContractManifest) -> String {
    let mut output = String::new();
    output.push_str("# rUvOS Contract Manifest\n\n");
    output.push_str(&format!("- Version: `{}`\n", manifest.version));
    output.push_str(&format!("- Tool count: `{}`\n", manifest.tool_count));
    output.push_str("\n## Tools\n\n");
    output.push_str("| Name | Domain | Description |\n|---|---|---|\n");
    for tool in &manifest.tools {
        output.push_str(&format!(
            "| `{}` | `{}` | {} |\n",
            tool.name, tool.domain, tool.description
        ));
    }
    output.push_str("\n## Archetypes\n\n");
    for archetype in &manifest.archetypes {
        output.push_str(&format!("- `{}` — {}\n", archetype.name, archetype.purpose));
    }
    output.push_str("\n## Hooks\n\n");
    for hook in &manifest.hooks {
        output.push_str(&format!(
            "- `{}` (`{}`) — {}\n",
            hook.name, hook.kind, hook.fires_on
        ));
    }
    output.push_str("\n## Templates\n\n");
    for template in &manifest.workflow_templates {
        output.push_str(&format!("- `{}`\n", template));
    }
    output
}

pub fn serialize_manifest(
    format: ContractFormat,
    manifest: &ContractManifest,
) -> anyhow::Result<String> {
    Ok(match format {
        ContractFormat::Json => serde_json::to_string_pretty(manifest)?,
        ContractFormat::Markdown => render_markdown(manifest),
    })
}

pub fn write_manifest(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn generate(format: ContractFormat, write: Option<PathBuf>) -> anyhow::Result<String> {
    let manifest = manifest();
    let rendered = serialize_manifest(format, &manifest)?;
    if let Some(path) = write {
        write_manifest(&path, &rendered)?;
    } else {
        println!("{rendered}");
    }
    Ok(rendered)
}

pub fn check(path: PathBuf) -> anyhow::Result<()> {
    let expected = manifest();
    let rendered = serialize_manifest(ContractFormat::Json, &expected)?;
    let actual =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    if normalize_json(&rendered)? != normalize_json(&actual)? {
        anyhow::bail!(
            "contract manifest drift detected: {} does not match the live registry",
            path.display()
        );
    }
    Ok(())
}

fn normalize_json(raw: &str) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(raw)?;
    Ok(serde_json::to_string_pretty(&value)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_tool_count_matches_registry() {
        let manifest = manifest();
        assert_eq!(manifest.tool_count, manifest.tools.len());
        assert!(manifest
            .tools
            .iter()
            .any(|tool| tool.name == "orchestrate.run"));
        assert!(manifest.tools.iter().any(|tool| tool.name == "relay.send"));
    }

    #[test]
    fn markdown_mentions_tool_count() {
        let rendered = render_markdown(&manifest());
        assert!(rendered.contains("Tool count"));
        assert!(rendered.contains("orchestrate.run"));
    }
}
