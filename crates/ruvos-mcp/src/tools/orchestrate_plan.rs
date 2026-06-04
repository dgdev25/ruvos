//! GOAP planning for `orchestrate`: model agent archetypes as actions over a
//! software-lifecycle world state, and named templates (or caller goals) as
//! [`GoapGoal`]s, so the pipeline is *computed* (A*) rather than hardcoded.
//!
//! Lifecycle world-state keys: `spec`, `plan`, `architecture`, `secured`,
//! `design` (any planning/spec/arch/security step satisfies this), `code`,
//! `tested`, `reviewed`. Each archetype declares the preconditions it needs and
//! the effects it produces; the planner derives the minimum-cost ordering.

use crate::runtime::{publish_event, RuntimeEvent};
use ruvos_goap::planner::PlanningEvent;
use ruvos_goap::{GoapAction, GoapGoal, GoapPlanner, StateValue, WorldState};

fn t() -> StateValue {
    StateValue::Bool(true)
}

/// The default archetype capability library (one [`GoapAction`] per archetype).
///
/// "Design" steps (researcher/planner/architect/security) each also set
/// `design=true`, which `coder` requires — so any one of them unblocks coding,
/// while the goal's specific keys (e.g. `plan`, `architecture`) decide *which*
/// design step is forced in.
pub fn default_actions() -> Vec<GoapAction> {
    vec![
        GoapAction::new("researcher")
            .with_effect("spec", t())
            .with_effect("design", t()),
        GoapAction::new("planner")
            .with_effect("plan", t())
            .with_effect("design", t()),
        GoapAction::new("architect")
            .with_effect("architecture", t())
            .with_effect("design", t()),
        GoapAction::new("security")
            .with_effect("secured", t())
            .with_effect("design", t()),
        GoapAction::new("coder")
            .with_precondition("design", t())
            .with_effect("code", t()),
        GoapAction::new("tester")
            .with_precondition("code", t())
            .with_effect("tested", t()),
        GoapAction::new("reviewer")
            .with_precondition("tested", t())
            .with_effect("reviewed", t()),
    ]
}

/// A named template → desired end-state goal. The desired keys are chosen so the
/// unique minimum-cost plan reproduces the intended archetype sequence.
pub fn goal_for_template(name: &str) -> Option<GoapGoal> {
    let g = GoapGoal::new(name, 1.0);
    Some(match name {
        // planner forced by `plan`; reviewer pulls tester→coder→planner.
        "feature" => g
            .with_condition("plan", t())
            .with_condition("reviewed", t()),
        // researcher forced by `spec`; tester pulls coder.
        "bugfix" => g.with_condition("spec", t()).with_condition("tested", t()),
        // architect forced by `architecture`; reviewer pulls tester→coder.
        "refactor" => g
            .with_condition("architecture", t())
            .with_condition("reviewed", t()),
        // security forced by `secured`; tester pulls coder.
        "security" => g
            .with_condition("secured", t())
            .with_condition("tested", t()),
        // SPARC: spec + architecture + full review chain.
        "sparc" => g
            .with_condition("spec", t())
            .with_condition("architecture", t())
            .with_condition("reviewed", t()),
        _ => return None,
    })
}

/// Build a planner seeded with `actions` and an empty initial world state.
pub fn build_planner(actions: &[GoapAction]) -> GoapPlanner {
    let planner = GoapPlanner::new();
    planner.add_actions(actions.to_vec());
    planner.set_world_state(WorldState::new());
    planner
}

/// Plan the archetype sequence for a named template, optionally with `extra`
/// caller-supplied actions. Returns `(ordered archetype names, total cost)`, or
/// `None` if the template is unknown or the goal is unreachable.
pub fn plan_archetypes(template: &str, extra: &[GoapAction]) -> Option<(Vec<String>, f64)> {
    let goal = goal_for_template(template)?;
    plan_for_goal(&goal, extra)
}

/// Plan the archetype sequence to reach an explicit `goal`, optionally with
/// `extra` caller-supplied actions on top of the default library.
pub fn plan_for_goal(goal: &GoapGoal, extra: &[GoapAction]) -> Option<(Vec<String>, f64)> {
    let mut actions = default_actions();
    actions.extend_from_slice(extra);
    let planner = build_planner(&actions);
    let mut observed_goal = false;
    let plan = planner.plan_with_observer(goal, |event| {
        observed_goal = true;
        match event {
            PlanningEvent::Started {
                goal_name,
                goal_size,
            } => publish_event(RuntimeEvent {
                kind: "goap.plan.started".to_string(),
                payload: serde_json::json!({
                    "goal_name": goal_name,
                    "goal_size": goal_size,
                }),
                agent_id: None,
                task_id: None,
            }),
            PlanningEvent::NodeExpanded {
                iteration,
                cost,
                heuristic,
                action,
            } => publish_event(RuntimeEvent {
                kind: "goap.plan.node_expanded".to_string(),
                payload: serde_json::json!({
                    "iteration": iteration,
                    "cost": cost,
                    "heuristic": heuristic,
                    "action": action,
                }),
                agent_id: None,
                task_id: None,
            }),
            PlanningEvent::GoalSatisfied {
                goal_name,
                actions,
                total_cost,
                iterations,
            } => publish_event(RuntimeEvent {
                kind: "goap.plan.completed".to_string(),
                payload: serde_json::json!({
                    "goal_name": goal_name,
                    "actions": actions,
                    "total_cost": total_cost,
                    "iterations": iterations,
                }),
                agent_id: None,
                task_id: None,
            }),
            PlanningEvent::GoalUnreachable {
                goal_name,
                iterations,
            } => publish_event(RuntimeEvent {
                kind: "goap.plan.failed".to_string(),
                payload: serde_json::json!({
                    "goal_name": goal_name,
                    "iterations": iterations,
                }),
                agent_id: None,
                task_id: None,
            }),
            PlanningEvent::AbortedMaxIterations {
                goal_name,
                max_iterations,
            } => publish_event(RuntimeEvent {
                kind: "goap.plan.aborted".to_string(),
                payload: serde_json::json!({
                    "goal_name": goal_name,
                    "max_iterations": max_iterations,
                }),
                agent_id: None,
                task_id: None,
            }),
        }
    })?;
    debug_assert!(observed_goal);
    if plan.actions.is_empty() {
        return None;
    }
    let names = plan.actions.iter().map(|a| a.name.clone()).collect();
    Some((names, plan.total_cost))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(template: &str) -> Vec<String> {
        plan_archetypes(template, &[]).expect("a plan").0
    }

    #[test]
    fn feature_plans_canonical_order() {
        assert_eq!(names("feature"), ["planner", "coder", "tester", "reviewer"]);
    }

    #[test]
    fn bugfix_plans_research_code_test() {
        assert_eq!(names("bugfix"), ["researcher", "coder", "tester"]);
    }

    #[test]
    fn security_plans_security_code_test() {
        assert_eq!(names("security"), ["security", "coder", "tester"]);
    }

    #[test]
    fn refactor_plans_with_tester_before_review() {
        // Consistent lifecycle: a refactor is tested before it is reviewed
        // (legacy hardcoded order omitted the tester; this is the refinement).
        assert_eq!(
            names("refactor"),
            ["architect", "coder", "tester", "reviewer"]
        );
    }

    #[test]
    fn sparc_covers_all_phases_in_dependency_order() {
        let seq = names("sparc");
        for phase in ["researcher", "architect", "coder", "tester", "reviewer"] {
            assert!(seq.contains(&phase.to_string()), "sparc missing {phase}");
        }
        // The planner guarantees precondition order (not the order of independent
        // steps): a design step precedes coding, and code → test → review hold.
        let pos = |n: &str| seq.iter().position(|x| x == n).unwrap();
        assert!(pos("coder") < pos("tester") && pos("tester") < pos("reviewer"));
        assert!(
            pos("researcher").min(pos("architect")) < pos("coder"),
            "a design phase must precede coding"
        );
    }

    #[test]
    fn unknown_template_is_none() {
        assert!(plan_archetypes("magic", &[]).is_none());
    }

    #[test]
    fn custom_goal_composes_from_library() {
        let goal = GoapGoal::new("harden", 1.0)
            .with_condition("secured", t())
            .with_condition("tested", t());
        let (seq, _) = plan_for_goal(&goal, &[]).expect("a plan");
        assert!(seq.contains(&"security".to_string()));
        assert!(seq.contains(&"tester".to_string()));
        assert_eq!(seq.last().unwrap(), "tester");
    }
}
