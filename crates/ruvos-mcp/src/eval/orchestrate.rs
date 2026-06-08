//! Orchestration handoff eval suite.
//!
//! Verifies that GOAP planning produces the correct archetype pipeline for each
//! known template, and that the conditional-edge graph routes correctly on
//! success and failure.

use crate::tools::orchestrate_plan::plan_archetypes;
use ruvos_graphflow::{EdgeCond, FlowGraph};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffCaseResult {
    pub name: String,
    pub pipeline: Vec<String>,
    pub expected: Vec<String>,
    pub matches: bool,
    pub step_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRoutingResult {
    pub success_routing_correct: bool,
    pub failure_loops_back_to_coder: bool,
    pub self_loop_without_prior_coder: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffSummary {
    pub case_count: usize,
    pub correct_count: usize,
    pub all_correct: bool,
    pub routing_correct: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffReport {
    pub suite: String,
    pub pipeline_cases: Vec<HandoffCaseResult>,
    pub graph_routing: GraphRoutingResult,
    pub summary: HandoffSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffComparison {
    pub suite_matches: bool,
    pub case_count_matches: bool,
    pub matching_case_names: bool,
    pub all_correct_baseline: bool,
    pub all_correct_current: bool,
}

fn expected_pipeline(template: &str) -> Vec<String> {
    let steps: &[&str] = match template {
        "feature" => &["planner", "coder", "tester", "reviewer"],
        "bugfix" => &["researcher", "coder", "tester"],
        "refactor" => &["architect", "coder", "tester", "reviewer"],
        "security" => &["security", "coder", "tester"],
        // Minimum required set; GOAP may include planner too.
        "sparc" => &["researcher", "architect", "coder", "tester", "reviewer"],
        _ => &[],
    };
    steps.iter().map(|s| s.to_string()).collect()
}

fn pipeline_matches(template: &str, pipeline: &[String]) -> bool {
    if template == "sparc" {
        // Check ordering constraints rather than exact equality.
        let pos = |n: &str| pipeline.iter().position(|x| x == n);
        let coder = match pos("coder") { Some(p) => p, None => return false };
        let tester = match pos("tester") { Some(p) => p, None => return false };
        let reviewer = match pos("reviewer") { Some(p) => p, None => return false };
        let has_design = pos("researcher").is_some() || pos("architect").is_some();
        has_design && coder < tester && tester < reviewer
    } else {
        pipeline == expected_pipeline(template)
    }
}

fn build_eval_graph(pipeline: &[String]) -> FlowGraph {
    let mut g = FlowGraph::new(pipeline[0].clone());
    for i in 0..pipeline.len() {
        if i + 1 < pipeline.len() {
            g = g.edge(
                pipeline[i].clone(),
                pipeline[i + 1].clone(),
                EdgeCond::OnSuccess,
            );
        }
        let rework = pipeline[..i]
            .iter()
            .rposition(|a| a == "coder")
            .map(|p| pipeline[p].clone())
            .unwrap_or_else(|| pipeline[i].clone());
        g = g.edge(pipeline[i].clone(), rework, EdgeCond::OnFailure);
    }
    g
}

fn check_graph_routing() -> GraphRoutingResult {
    // Canonical 4-step pipeline: planner → coder → tester → reviewer.
    let pipeline: Vec<String> = ["planner", "coder", "tester", "reviewer"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let g = build_eval_graph(&pipeline);

    let success_correct = g.next("planner", true) == Some("coder")
        && g.next("coder", true) == Some("tester")
        && g.next("tester", true) == Some("reviewer")
        && g.next("reviewer", true).is_none();

    // tester and reviewer both loop back to the nearest prior coder step.
    let failure_loops_to_coder = g.next("tester", false) == Some("coder")
        && g.next("reviewer", false) == Some("coder");

    // planner has no prior coder, so failure → self. coder's only prior is planner,
    // so coder failure also loops to itself.
    let self_loop = g.next("planner", false) == Some("planner")
        && g.next("coder", false) == Some("coder");

    GraphRoutingResult {
        success_routing_correct: success_correct,
        failure_loops_back_to_coder: failure_loops_to_coder,
        self_loop_without_prior_coder: self_loop,
    }
}

pub fn run_orchestrate_handoff_suite() -> HandoffReport {
    let templates = ["feature", "bugfix", "refactor", "security", "sparc"];
    let mut pipeline_cases = Vec::new();

    for &t in &templates {
        let pipeline = plan_archetypes(t, &[])
            .map(|(steps, _)| steps)
            .unwrap_or_default();
        let expected = expected_pipeline(t);
        let matches = pipeline_matches(t, &pipeline);
        let step_count = pipeline.len();
        pipeline_cases.push(HandoffCaseResult {
            name: t.to_string(),
            pipeline,
            expected,
            matches,
            step_count,
        });
    }

    let graph_routing = check_graph_routing();
    let correct_count = pipeline_cases.iter().filter(|c| c.matches).count();
    let routing_correct = graph_routing.success_routing_correct
        && graph_routing.failure_loops_back_to_coder
        && graph_routing.self_loop_without_prior_coder;

    HandoffReport {
        suite: "orchestrate-handoff".to_string(),
        summary: HandoffSummary {
            case_count: pipeline_cases.len(),
            correct_count,
            all_correct: correct_count == pipeline_cases.len(),
            routing_correct,
        },
        pipeline_cases,
        graph_routing,
    }
}

pub fn load_handoff_report(path: impl AsRef<Path>) -> anyhow::Result<HandoffReport> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

pub fn compare_handoff_reports(
    current: &HandoffReport,
    baseline: &HandoffReport,
) -> HandoffComparison {
    let matching_names = current
        .pipeline_cases
        .iter()
        .map(|c| &c.name)
        .eq(baseline.pipeline_cases.iter().map(|c| &c.name));
    HandoffComparison {
        suite_matches: current.suite == baseline.suite,
        case_count_matches: current.summary.case_count == baseline.summary.case_count,
        matching_case_names: matching_names,
        all_correct_baseline: baseline.summary.all_correct,
        all_correct_current: current.summary.all_correct,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suite_all_templates_correct() {
        let report = run_orchestrate_handoff_suite();
        assert_eq!(report.suite, "orchestrate-handoff");
        assert_eq!(report.summary.case_count, 5);
        assert!(
            report.summary.all_correct,
            "failed cases: {:?}",
            report
                .pipeline_cases
                .iter()
                .filter(|c| !c.matches)
                .map(|c| (&c.name, &c.pipeline))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn graph_routing_correct() {
        let report = run_orchestrate_handoff_suite();
        assert!(report.graph_routing.success_routing_correct);
        assert!(report.graph_routing.failure_loops_back_to_coder);
        assert!(report.graph_routing.self_loop_without_prior_coder);
        assert!(report.summary.routing_correct);
    }

    #[test]
    fn compare_identical_reports_matches() {
        let report = run_orchestrate_handoff_suite();
        let cmp = compare_handoff_reports(&report, &report);
        assert!(cmp.suite_matches);
        assert!(cmp.case_count_matches);
        assert!(cmp.matching_case_names);
    }
}
