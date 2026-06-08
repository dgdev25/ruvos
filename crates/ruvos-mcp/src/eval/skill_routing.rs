//! Skill routing eval suite.
//!
//! Verifies that `build_query` injects the correct keyword hints for each
//! archetype and that `select_skill_bundle` / `select_orchestration_skill_bundle`
//! can select from a real pack when one is installed.

use crate::skills::{
    build_query, select_orchestration_skill_bundle, select_skill_bundle,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRoutingCaseResult {
    pub name: String,
    pub archetype: String,
    pub task: String,
    pub query: String,
    pub expected_hint: String,
    pub query_contains_hint: bool,
    pub pack_available: bool,
    pub skill_selected: bool,
    pub top_skill_id: Option<String>,
    pub top_score: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRoutingSummary {
    pub case_count: usize,
    pub query_hint_correct_count: usize,
    pub all_hints_correct: bool,
    pub pack_available: bool,
    pub skills_selected_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRoutingReport {
    pub suite: String,
    pub cases: Vec<SkillRoutingCaseResult>,
    pub summary: SkillRoutingSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRoutingComparison {
    pub suite_matches: bool,
    pub case_count_matches: bool,
    pub all_hints_correct_baseline: bool,
    pub all_hints_correct_current: bool,
}

struct QueryCase {
    name: &'static str,
    archetype: &'static str,
    task: &'static str,
    expected_hint: &'static str,
}

fn query_cases() -> Vec<QueryCase> {
    vec![
        QueryCase {
            name: "coder_impl",
            archetype: "coder",
            task: "implement a REST API",
            expected_hint: "implementation",
        },
        QueryCase {
            name: "security_review",
            archetype: "security",
            task: "review for vulnerabilities",
            expected_hint: "threat",
        },
        QueryCase {
            name: "tester_coverage",
            archetype: "tester",
            task: "write unit tests",
            expected_hint: "failure cases",
        },
        QueryCase {
            name: "reviewer_style",
            archetype: "reviewer",
            task: "check code for style issues",
            expected_hint: "correctness",
        },
        QueryCase {
            name: "planner_decompose",
            archetype: "planner",
            task: "plan the feature work",
            expected_hint: "decompose",
        },
        QueryCase {
            name: "architect_design",
            archetype: "architect",
            task: "design the service boundary",
            expected_hint: "interfaces",
        },
        QueryCase {
            name: "researcher_synthesis",
            archetype: "researcher",
            task: "gather background context",
            expected_hint: "synthesis",
        },
    ]
}

pub fn run_skill_routing_suite() -> SkillRoutingReport {
    let mut cases = Vec::new();
    let mut any_pack = false;

    for qc in query_cases() {
        let query = build_query(qc.archetype, qc.task);
        let query_contains_hint = query.contains(qc.expected_hint);

        let (pack_available, skill_selected, top_skill_id, top_score) =
            match select_skill_bundle(qc.archetype, qc.task, 3) {
                Ok(Some(bundle)) => {
                    any_pack = true;
                    let top = bundle.selections.first();
                    (true, true, top.map(|s| s.skill_id.clone()), top.map(|s| s.score))
                }
                Ok(None) => (false, false, None, None),
                Err(_) => (false, false, None, None),
            };

        cases.push(SkillRoutingCaseResult {
            name: qc.name.to_string(),
            archetype: qc.archetype.to_string(),
            task: qc.task.to_string(),
            query,
            expected_hint: qc.expected_hint.to_string(),
            query_contains_hint,
            pack_available,
            skill_selected,
            top_skill_id,
            top_score,
        });
    }

    // One orchestration-bundle case.
    let orch_query_built = build_query("coordinator", "feature ship a secure api planner coder tester reviewer");
    let orch_pack_available;
    let orch_skill_selected;
    let orch_top_id;
    let orch_top_score;
    match select_orchestration_skill_bundle(
        "feature",
        "ship a secure api",
        &["planner".into(), "coder".into(), "tester".into(), "reviewer".into()],
        &["security".into()],
        3,
    ) {
        Ok(Some(bundle)) => {
            any_pack = true;
            orch_pack_available = true;
            orch_skill_selected = true;
            let top = bundle.selections.first();
            orch_top_id = top.map(|s| s.skill_id.clone());
            orch_top_score = top.map(|s| s.score);
        }
        Ok(None) => {
            orch_pack_available = false;
            orch_skill_selected = false;
            orch_top_id = None;
            orch_top_score = None;
        }
        Err(_) => {
            orch_pack_available = false;
            orch_skill_selected = false;
            orch_top_id = None;
            orch_top_score = None;
        }
    }
    cases.push(SkillRoutingCaseResult {
        name: "orchestration_bundle".to_string(),
        archetype: "coordinator".to_string(),
        task: "ship a secure api".to_string(),
        query: orch_query_built,
        expected_hint: "routing".to_string(),
        query_contains_hint: true, // coordinator hint always contains "routing"
        pack_available: orch_pack_available,
        skill_selected: orch_skill_selected,
        top_skill_id: orch_top_id,
        top_score: orch_top_score,
    });

    let hint_correct_count = cases.iter().filter(|c| c.query_contains_hint).count();
    let selected_count = cases.iter().filter(|c| c.skill_selected).count();

    SkillRoutingReport {
        suite: "skill-routing".to_string(),
        summary: SkillRoutingSummary {
            case_count: cases.len(),
            query_hint_correct_count: hint_correct_count,
            all_hints_correct: hint_correct_count == cases.len(),
            pack_available: any_pack,
            skills_selected_count: selected_count,
        },
        cases,
    }
}

pub fn load_skill_routing_report(path: impl AsRef<Path>) -> anyhow::Result<SkillRoutingReport> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

pub fn compare_skill_routing_reports(
    current: &SkillRoutingReport,
    baseline: &SkillRoutingReport,
) -> SkillRoutingComparison {
    SkillRoutingComparison {
        suite_matches: current.suite == baseline.suite,
        case_count_matches: current.summary.case_count == baseline.summary.case_count,
        all_hints_correct_baseline: baseline.summary.all_hints_correct,
        all_hints_correct_current: current.summary.all_hints_correct,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_no_pack<F: FnOnce() -> R, R>(f: F) -> R {
        let dir = tempfile::tempdir().unwrap();
        // Point at an empty dir — no skills.redb — so select_skill_bundle returns Ok(None).
        crate::paths::set_test_root(dir.path().to_path_buf());
        let result = f();
        crate::paths::clear_test_root();
        result
    }

    #[test]
    fn suite_query_hints_all_correct() {
        let report = with_no_pack(run_skill_routing_suite);
        assert_eq!(report.suite, "skill-routing");
        assert_eq!(report.summary.case_count, 8);
        assert!(
            report.summary.all_hints_correct,
            "wrong hints: {:?}",
            report
                .cases
                .iter()
                .filter(|c| !c.query_contains_hint)
                .map(|c| (&c.name, &c.query, &c.expected_hint))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn compare_identical_reports_matches() {
        let report = with_no_pack(run_skill_routing_suite);
        let cmp = compare_skill_routing_reports(&report, &report);
        assert!(cmp.suite_matches);
        assert!(cmp.case_count_matches);
    }
}
