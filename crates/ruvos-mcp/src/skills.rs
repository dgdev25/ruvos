//! Runtime skill selection for task prompts.

use crate::paths;
use anyhow::Context;
use ruvos_skills::{SkillRecord, SkillSearchHit, SkillStore};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectedSkill {
    pub skill_id: String,
    pub name: String,
    pub score: u32,
    pub reason: String,
    pub purpose: String,
    pub tags: Vec<String>,
    pub validation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillBundle {
    pub pack_path: PathBuf,
    pub query: String,
    pub selections: Vec<SelectedSkill>,
}

impl SkillBundle {
    pub fn is_empty(&self) -> bool {
        self.selections.is_empty()
    }

    pub fn selected_skill_ids(&self) -> Vec<String> {
        self.selections
            .iter()
            .map(|selection| selection.skill_id.clone())
            .collect()
    }

    pub fn render_prompt_section(&self) -> String {
        let mut out = String::new();
        out.push_str("## Skill guidance\n");
        out.push_str("Use the selected skills as operating constraints and validation anchors.\n");
        for (index, skill) in self.selections.iter().enumerate() {
            out.push_str(&format!(
                "{}. `{}` — `{}` (score `{}`, {})\n",
                index + 1,
                skill.skill_id,
                skill.name,
                skill.score,
                skill.reason
            ));
            out.push_str(&format!("   - purpose: {}\n", skill.purpose));
            if !skill.tags.is_empty() {
                out.push_str(&format!("   - tags: {}\n", skill.tags.join(", ")));
            }
            if !skill.validation.is_empty() {
                out.push_str(&format!(
                    "   - validation: {}\n",
                    skill.validation.join(" | ")
                ));
            }
        }
        out
    }

    pub fn persist_to_disk(&self, orchestration_id: &str) -> anyhow::Result<PathBuf> {
        let out_dir = std::env::current_dir()
            .context("reading current directory")?
            .join("generated")
            .join(orchestration_id);
        std::fs::create_dir_all(&out_dir)
            .with_context(|| format!("creating {}", out_dir.display()))?;
        let out_path = out_dir.join("selected-skills.json");
        let rendered = serde_json::to_string_pretty(self)?;
        std::fs::write(&out_path, rendered)
            .with_context(|| format!("writing {}", out_path.display()))?;
        Ok(out_path)
    }
}

pub fn select_skill_bundle(
    archetype: &str,
    prompt: &str,
    limit: usize,
) -> anyhow::Result<Option<SkillBundle>> {
    let pack_path = paths::skills_pack_file();
    if !pack_path.exists() {
        return Ok(None);
    }

    let store = SkillStore::open(&pack_path)
        .with_context(|| format!("opening skills pack {}", pack_path.display()))?;
    let query = build_query(archetype, prompt);
    let hits = store.search(&query, limit)?;
    if hits.is_empty() {
        return Ok(None);
    }

    let mut selections = Vec::new();
    for hit in hits {
        if let Some(skill) = store.get_skill(&hit.skill_id)? {
            selections.push(selection_from_hit(skill, hit));
        }
    }

    if selections.is_empty() {
        return Ok(None);
    }

    Ok(Some(SkillBundle {
        pack_path,
        query,
        selections,
    }))
}

/// Select one bundle for an entire orchestration/task run.
pub fn select_task_skill_bundle(task: &str, limit: usize) -> anyhow::Result<Option<SkillBundle>> {
    select_skill_bundle("coordinator", task, limit)
}

/// Select one bundle using the whole orchestration plan as context.
pub fn select_orchestration_skill_bundle(
    template: &str,
    task: &str,
    pipeline: &[String],
    extra_context: &[String],
    limit: usize,
) -> anyhow::Result<Option<SkillBundle>> {
    let mut query = format!("{template} {task}");
    if !pipeline.is_empty() {
        query.push(' ');
        query.push_str(&pipeline.join(" "));
    }
    if !extra_context.is_empty() {
        query.push(' ');
        query.push_str(&extra_context.join(" "));
    }
    select_skill_bundle("coordinator", &query, limit)
}

pub fn record_skill_bundle_feedback(
    bundle: &SkillBundle,
    success: bool,
    outcome: &str,
    note: Option<String>,
) -> anyhow::Result<()> {
    let store = SkillStore::open(&bundle.pack_path)
        .with_context(|| format!("opening skills pack {}", bundle.pack_path.display()))?;
    for skill_id in bundle.selected_skill_ids() {
        store.record_feedback(&skill_id, success, outcome.to_string(), note.clone())?;
    }
    Ok(())
}

pub(crate) fn build_query(archetype: &str, prompt: &str) -> String {
    let hints = match archetype {
        "coder" => "implementation rust code tests api",
        "tester" => "validation tests failure cases coverage",
        "reviewer" => "review correctness safety style risk",
        "planner" => "decompose plan sequencing goals",
        "researcher" => "research sources synthesis analysis",
        "architect" => "interfaces boundaries design",
        "security" => "threat model vulnerabilities mitigation",
        "perf" => "profiling hotspots optimization",
        "devops" => "deployment ci cd infrastructure",
        "data" => "schema query migration analysis",
        "docs" => "documentation examples usage",
        "coordinator" => "routing delegation handoff",
        _ => "task execution",
    };
    format!("{archetype} {hints} {prompt}")
}

fn selection_from_hit(skill: SkillRecord, hit: SkillSearchHit) -> SelectedSkill {
    SelectedSkill {
        skill_id: skill.id,
        name: skill.name,
        score: hit.score,
        reason: hit.reason,
        purpose: skill.purpose,
        tags: skill.tags,
        validation: skill.validation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruvos_skills::{CompressionCodec, SkillChunkLink, SkillPackMeta, SkillSource};

    fn sample_skill(id: &str, name: &str, purpose: &str, tags: &[&str]) -> SkillRecord {
        SkillRecord {
            id: id.to_string(),
            name: name.to_string(),
            version: "1.0.0".to_string(),
            purpose: purpose.to_string(),
            tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
            aliases: vec![id.to_string()],
            prerequisites: vec!["read the task".to_string()],
            safety_level: "advisory".to_string(),
            validation: vec!["check the result".to_string()],
            summary: Some(purpose.to_string()),
            source: SkillSource {
                source_root: "/skillbase".to_string(),
                source_path: format!("{id}.md"),
                corpus_hash: "abc123".to_string(),
            },
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn render_prompt_section_includes_skill_details() {
        let bundle = SkillBundle {
            pack_path: PathBuf::from("/tmp/skills.redb"),
            query: "coder rust".to_string(),
            selections: vec![SelectedSkill {
                skill_id: "safe-rust".to_string(),
                name: "Safe Rust".to_string(),
                score: 4,
                reason: "tag match".to_string(),
                purpose: "Write safe Rust".to_string(),
                tags: vec!["rust".to_string(), "safety".to_string()],
                validation: vec!["compile".to_string()],
            }],
        };

        let rendered = bundle.render_prompt_section();
        assert!(rendered.contains("safe-rust"));
        assert!(rendered.contains("Write safe Rust"));
    }

    #[test]
    fn select_skill_bundle_uses_pack_search() {
        let dir = tempfile::tempdir().unwrap();
        paths::set_test_root(dir.path().to_path_buf());
        let pack_path = paths::skills_pack_file();
        let store = SkillStore::open(&pack_path).unwrap();
        let skill = sample_skill(
            "safe-rust",
            "Safe Rust",
            "Write safe Rust modules",
            &["rust", "coder"],
        );
        store.put_skill(&skill).unwrap();
        let chunk = store
            .encode_and_put_chunk(b"skill body", CompressionCodec::None)
            .unwrap();
        store.put_chunk(&chunk).unwrap();
        store
            .put_skill_chunks(
                &skill.id,
                &[SkillChunkLink {
                    ordinal: 0,
                    chunk_hash: chunk.hash,
                }],
            )
            .unwrap();
        store
            .put_pack_meta(&SkillPackMeta::new(
                "corpus",
                "/skillbase",
                CompressionCodec::None,
                1,
                1,
            ))
            .unwrap();
        drop(store);

        let bundle = select_skill_bundle("coder", "write a safe rust module", 3)
            .unwrap()
            .expect("expected a skill bundle");
        assert_eq!(bundle.selections[0].skill_id, "safe-rust");
    }

    #[test]
    fn select_task_skill_bundle_uses_coordinator_query() {
        let dir = tempfile::tempdir().unwrap();
        paths::set_test_root(dir.path().to_path_buf());
        let pack_path = paths::skills_pack_file();
        let store = SkillStore::open(&pack_path).unwrap();
        let skill = sample_skill(
            "safe-rust",
            "Safe Rust",
            "Write safe Rust modules",
            &["coordinator", "task"],
        );
        store.put_skill(&skill).unwrap();
        let chunk = store
            .encode_and_put_chunk(b"task body", CompressionCodec::None)
            .unwrap();
        store.put_chunk(&chunk).unwrap();
        store
            .put_skill_chunks(
                &skill.id,
                &[SkillChunkLink {
                    ordinal: 0,
                    chunk_hash: chunk.hash,
                }],
            )
            .unwrap();
        store
            .put_pack_meta(&SkillPackMeta::new(
                "corpus",
                "/skillbase",
                CompressionCodec::None,
                1,
                1,
            ))
            .unwrap();
        drop(store);

        let bundle = select_task_skill_bundle("write a safe rust module", 3)
            .unwrap()
            .expect("expected a task skill bundle");
        assert_eq!(bundle.selections[0].skill_id, "safe-rust");
    }

    #[test]
    fn select_orchestration_skill_bundle_uses_plan_terms() {
        let dir = tempfile::tempdir().unwrap();
        paths::set_test_root(dir.path().to_path_buf());
        let pack_path = paths::skills_pack_file();
        let store = SkillStore::open(&pack_path).unwrap();
        let skill = sample_skill(
            "plan-security",
            "Plan Security",
            "Harden a planned workflow",
            &["security", "reviewer"],
        );
        store.put_skill(&skill).unwrap();
        let chunk = store
            .encode_and_put_chunk(b"plan body", CompressionCodec::None)
            .unwrap();
        store.put_chunk(&chunk).unwrap();
        store
            .put_skill_chunks(
                &skill.id,
                &[SkillChunkLink {
                    ordinal: 0,
                    chunk_hash: chunk.hash,
                }],
            )
            .unwrap();
        store
            .put_pack_meta(&SkillPackMeta::new(
                "corpus",
                "/skillbase",
                CompressionCodec::None,
                1,
                1,
            ))
            .unwrap();
        drop(store);

        let bundle = select_orchestration_skill_bundle(
            "feature",
            "build a secure feature",
            &[
                "planner".to_string(),
                "security".to_string(),
                "reviewer".to_string(),
            ],
            &["security".to_string(), "plan".to_string()],
            3,
        )
        .unwrap()
        .expect("expected an orchestration skill bundle");
        assert_eq!(bundle.selections[0].skill_id, "plan-security");
    }

    #[test]
    fn bundle_persists_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = SkillBundle {
            pack_path: PathBuf::from("/tmp/skills.redb"),
            query: "coder rust".to_string(),
            selections: vec![SelectedSkill {
                skill_id: "safe-rust".to_string(),
                name: "Safe Rust".to_string(),
                score: 4,
                reason: "tag match".to_string(),
                purpose: "Write safe Rust".to_string(),
                tags: vec!["rust".to_string(), "safety".to_string()],
                validation: vec!["compile".to_string()],
            }],
        };

        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let path = bundle.persist_to_disk("orch-1").unwrap();
        std::env::set_current_dir(previous).unwrap();

        assert!(path.exists());
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("safe-rust"));
    }
}
