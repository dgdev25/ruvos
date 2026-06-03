//! `hooks.route` — keyword-heuristic task routing to archetype + model tier.
//!
//! Split out of `hooks.rs` to keep each source file under the 500-line CI
//! limit. Re-exported from `hooks` so the public path `hooks::HooksRouteHandler`
//! is unchanged.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::Result;
use serde_json::{json, Value};

/// Routes a task to the best archetype + model tier using real keyword
/// heuristics over the task text. Returns a confidence based on how strongly
/// the task matched a known archetype's signal words.
pub struct HooksRouteHandler;

impl HooksRouteHandler {
    /// (archetype, signal keywords, model tier).
    const RULES: &'static [(&'static str, &'static [&'static str], u32)] = &[
        ("tester", &["test", "spec", "tdd", "coverage", "assert"], 2),
        (
            "security",
            &["security", "vuln", "auth", "exploit", "threat", "injection"],
            3,
        ),
        (
            "perf",
            &[
                "perf",
                "performance",
                "optimize",
                "latency",
                "benchmark",
                "profil",
            ],
            3,
        ),
        (
            "reviewer",
            &["review", "lint", "quality", "style", "refactor"],
            3,
        ),
        (
            "architect",
            &["architect", "design", "interface", "boundary", "system"],
            3,
        ),
        (
            "planner",
            &["plan", "decompose", "roadmap", "milestone", "breakdown"],
            3,
        ),
        (
            "devops",
            &[
                "deploy",
                "ci",
                "cd",
                "pipeline",
                "docker",
                "kubernetes",
                "infra",
            ],
            2,
        ),
        (
            "data",
            &["schema", "migration", "database", "sql", "query", "table"],
            2,
        ),
        (
            "docs",
            &["document", "docs", "readme", "guide", "tutorial"],
            2,
        ),
        (
            "researcher",
            &["research", "investigate", "explore", "find", "discover"],
            2,
        ),
        (
            "coordinator",
            &[
                "coordinate",
                "orchestrate",
                "swarm",
                "multi-agent",
                "delegate",
            ],
            3,
        ),
        (
            "coder",
            &["implement", "build", "code", "endpoint", "function", "fix"],
            2,
        ),
    ];

    fn model_for_tier(tier: u32) -> &'static str {
        match tier {
            3 => "claude-opus-4-8",
            2 => "claude-haiku-4-5",
            _ => "claude-haiku-4-5",
        }
    }

    fn route(task: &str) -> (String, &'static str, u32, f64) {
        let lower = task.to_lowercase();
        let mut best = ("coder", 2u32, 0usize);
        for (archetype, keywords, tier) in Self::RULES {
            let hits = keywords.iter().filter(|k| lower.contains(**k)).count();
            if hits > best.2 {
                best = (archetype, *tier, hits);
            }
        }
        // Confidence scales with the number of matched signal words (capped).
        let confidence = if best.2 == 0 {
            0.3 // default fallback to coder, low confidence
        } else {
            (0.5 + 0.15 * best.2 as f64).min(0.95)
        };
        (
            best.0.to_string(),
            Self::model_for_tier(best.1),
            best.1,
            confidence,
        )
    }
}

impl ToolHandler for HooksRouteHandler {
    fn name(&self) -> &'static str {
        "route"
    }

    fn domain(&self) -> &'static str {
        "hooks"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("task").and_then(|v| v.as_str()).is_none() {
            return Err(crate::RuvosError::InvalidParams(
                "missing 'task' field (string)".to_string(),
            ));
        }
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let task = params["task"].as_str().unwrap_or_default().to_string();
            let (archetype, model, tier, confidence) = Self::route(&task);
            Ok(json!({
                "task": task,
                "archetype": archetype,
                "model": model,
                "tier": tier,
                "confidence": (confidence * 1000.0).round() / 1000.0
            }))
        })
    }
}

#[cfg(test)]
mod route_tests {
    use super::*;

    #[tokio::test]
    async fn routes_testing_task_to_tester() {
        let r = HooksRouteHandler
            .execute(json!({"task": "write tests for the users endpoint"}))
            .await
            .unwrap();
        assert_eq!(r["archetype"], "tester");
        assert!(r["confidence"].as_f64().unwrap() > 0.3);
    }

    #[tokio::test]
    async fn routes_security_task_to_opus_tier() {
        let r = HooksRouteHandler
            .execute(json!({"task": "audit auth flow for injection vulnerabilities"}))
            .await
            .unwrap();
        assert_eq!(r["archetype"], "security");
        assert_eq!(r["model"], "claude-opus-4-8");
        assert_eq!(r["tier"], 3);
    }

    #[tokio::test]
    async fn routes_build_task_to_coder() {
        let r = HooksRouteHandler
            .execute(json!({"task": "implement the POST /users endpoint"}))
            .await
            .unwrap();
        assert_eq!(r["archetype"], "coder");
    }

    #[tokio::test]
    async fn unknown_task_falls_back_low_confidence() {
        let r = HooksRouteHandler
            .execute(json!({"task": "asdf qwerty zxcv"}))
            .await
            .unwrap();
        assert_eq!(r["archetype"], "coder");
        assert!(r["confidence"].as_f64().unwrap() <= 0.3);
    }

    #[test]
    fn route_requires_task() {
        assert!(HooksRouteHandler.validate(&json!({})).is_err());
        assert!(HooksRouteHandler.validate(&json!({"task": "x"})).is_ok());
    }
}
