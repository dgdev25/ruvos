//! # ruvos-graphflow — lean typed DAG executor with conditional edges
//!
//! A small, pure-`std` workflow graph modeled on the conditional-edge design of
//! rUvnet's **rs-graph-llm / graph-flow** (© Reuven Cohen / @ruvnet, MIT) — but
//! without its PostgreSQL/Rig/async/Context machinery, which rUvOS does not need.
//!
//! A [`FlowGraph`] is a set of nodes (string ids) joined by edges that carry a
//! [`EdgeCond`] (`Always` / `OnSuccess` / `OnFailure`). Given the current node and
//! whether its step succeeded, [`FlowGraph::next`] returns the next node — so a
//! failed step can branch (e.g. loop back to an earlier step for rework) instead
//! of always advancing. [`run`] is a synchronous reference driver with per-node
//! visit caps and an overall step budget; async callers (e.g. `orchestrate`) reuse
//! [`FlowGraph::next`] in their own loop.

use std::collections::HashMap;

/// When an edge is eligible, based on the source step's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeCond {
    /// Eligible regardless of outcome.
    Always,
    /// Eligible only when the source step succeeded.
    OnSuccess,
    /// Eligible only when the source step failed.
    OnFailure,
}

impl EdgeCond {
    fn matches(self, success: bool) -> bool {
        match self {
            EdgeCond::Always => true,
            EdgeCond::OnSuccess => success,
            EdgeCond::OnFailure => !success,
        }
    }
}

#[derive(Debug, Clone)]
struct Edge {
    from: String,
    to: String,
    cond: EdgeCond,
}

/// A directed graph of workflow steps with conditional edges.
#[derive(Debug, Clone, Default)]
pub struct FlowGraph {
    start: String,
    edges: Vec<Edge>,
}

impl FlowGraph {
    pub fn new(start: impl Into<String>) -> Self {
        Self {
            start: start.into(),
            edges: Vec::new(),
        }
    }

    /// Add an edge `from → to` taken when `cond` matches the source outcome.
    /// Edges are evaluated in insertion order, so add specific (`OnSuccess` /
    /// `OnFailure`) edges before any `Always` fallback from the same node.
    pub fn edge(mut self, from: impl Into<String>, to: impl Into<String>, cond: EdgeCond) -> Self {
        self.edges.push(Edge {
            from: from.into(),
            to: to.into(),
            cond,
        });
        self
    }

    pub fn start(&self) -> &str {
        &self.start
    }

    /// The next node from `current` given whether its step `success`-ed: the first
    /// edge (in insertion order) whose condition matches. `None` ⇒ terminal node.
    pub fn next(&self, current: &str, success: bool) -> Option<&str> {
        self.edges
            .iter()
            .find(|e| e.from == current && e.cond.matches(success))
            .map(|e| e.to.as_str())
    }
}

/// The outcome of a [`run`]: the executed node path and whether it ended cleanly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub path: Vec<String>,
    pub success: bool,
}

/// Runtime events emitted by [`run_with_observer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEvent {
    Started {
        start: String,
        max_visits: usize,
        max_steps: usize,
    },
    Step {
        node: String,
        success: bool,
        next: Option<String>,
    },
    Terminal {
        node: String,
        success: bool,
    },
    VisitCapExceeded {
        node: String,
        max_visits: usize,
    },
    StepBudgetExceeded {
        max_steps: usize,
    },
}

/// Synchronous reference driver. Walks from the start node, calling `step(node)`
/// (returns success) and following [`FlowGraph::next`]. A node may be revisited up
/// to `max_visits` times (bounding retry/rework loops); `max_steps` caps total
/// work. `success` is `true` only when execution reaches a terminal node after a
/// successful step.
pub fn run<F>(graph: &FlowGraph, max_visits: usize, max_steps: usize, step: F) -> RunReport
where
    F: FnMut(&str) -> bool,
{
    run_with_observer(graph, max_visits, max_steps, step, |_| {})
}

/// Synchronous reference driver with lifecycle event observation.
pub fn run_with_observer<F, O>(
    graph: &FlowGraph,
    max_visits: usize,
    max_steps: usize,
    mut step: F,
    mut observer: O,
) -> RunReport
where
    F: FnMut(&str) -> bool,
    O: FnMut(&RunEvent),
{
    let mut visits: HashMap<String, usize> = HashMap::new();
    let mut path = Vec::new();
    let mut current = graph.start().to_string();

    observer(&RunEvent::Started {
        start: current.clone(),
        max_visits,
        max_steps,
    });

    for _ in 0..max_steps {
        *visits.entry(current.clone()).or_insert(0) += 1;
        path.push(current.clone());
        let ok = step(&current);
        let next = graph.next(&current, ok).map(|node| node.to_string());
        observer(&RunEvent::Step {
            node: current.clone(),
            success: ok,
            next: next.clone(),
        });

        match next {
            None => {
                observer(&RunEvent::Terminal {
                    node: current.clone(),
                    success: ok,
                });
                return RunReport { success: ok, path };
            }
            Some(next) => {
                if visits.get(&next).copied().unwrap_or(0) >= max_visits {
                    observer(&RunEvent::VisitCapExceeded {
                        node: next.clone(),
                        max_visits,
                    });
                    return RunReport {
                        success: false, // retry/rework budget exhausted
                        path,
                    };
                }
                current = next;
            }
        }
    }
    observer(&RunEvent::StepBudgetExceeded { max_steps });
    RunReport {
        success: false, // step budget exhausted
        path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// planner →(ok) coder →(ok) tester →(ok) reviewer[terminal];
    /// tester →(fail) coder  (rework loop).
    fn pipeline() -> FlowGraph {
        FlowGraph::new("planner")
            .edge("planner", "coder", EdgeCond::OnSuccess)
            .edge("coder", "tester", EdgeCond::OnSuccess)
            .edge("tester", "reviewer", EdgeCond::OnSuccess)
            .edge("tester", "coder", EdgeCond::OnFailure)
    }

    #[test]
    fn next_picks_edge_by_outcome() {
        let g = pipeline();
        assert_eq!(g.next("tester", true), Some("reviewer"));
        assert_eq!(g.next("tester", false), Some("coder"));
        assert_eq!(g.next("reviewer", true), None); // terminal
    }

    #[test]
    fn run_with_observer_emits_step_and_terminal_events() {
        let mut events = Vec::new();
        let report = run_with_observer(
            &pipeline(),
            3,
            20,
            |_| true,
            |event| {
                events.push(event.clone());
            },
        );
        assert!(report.success);
        assert!(matches!(events.first(), Some(RunEvent::Started { .. })));
        assert!(events.iter().any(|event| matches!(
            event,
            RunEvent::Step { node, success: true, .. } if node == "planner"
        )));
        assert!(events.iter().any(|event| matches!(event, RunEvent::Terminal { node, success: true } if node == "reviewer")));
    }

    #[test]
    fn all_success_runs_linear_path() {
        let report = run(&pipeline(), 3, 20, |_| true);
        assert_eq!(report.path, ["planner", "coder", "tester", "reviewer"]);
        assert!(report.success);
    }

    #[test]
    fn failed_tester_loops_back_to_coder_then_recovers() {
        // tester fails on its first visit, succeeds on its second.
        let mut tester_seen = 0;
        let report = run(&pipeline(), 3, 20, |node| {
            if node == "tester" {
                tester_seen += 1;
                tester_seen > 1 // fail first time, pass second
            } else {
                true
            }
        });
        assert!(report.success, "should recover after one rework loop");
        // planner, coder, tester(fail) → coder, tester(ok) → reviewer
        assert_eq!(
            report.path,
            ["planner", "coder", "tester", "coder", "tester", "reviewer"]
        );
    }

    #[test]
    fn persistent_failure_is_bounded_and_fails() {
        // tester always fails → loops to coder until the visit cap, then stops.
        let report = run(&pipeline(), 2, 50, |node| node != "tester");
        assert!(!report.success, "exhausted rework budget must fail");
        // bounded: coder visited at most max_visits (2) times.
        let coder_visits = report.path.iter().filter(|n| *n == "coder").count();
        assert!(coder_visits <= 2, "rework is bounded, got {coder_visits}");
    }

    #[test]
    fn step_budget_caps_runaway() {
        // A 1-node self-loop on success with a tiny step budget must still halt.
        let g = FlowGraph::new("a").edge("a", "a", EdgeCond::Always);
        let report = run(&g, 1000, 5, |_| true);
        assert!(!report.success);
        assert_eq!(report.path.len(), 5);
    }
}
