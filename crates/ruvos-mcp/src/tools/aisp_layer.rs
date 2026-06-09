//! ADR-034: AISP (AI Symbolic Protocol) prompt-precision layer.
//!
//! Converts free-form natural-language task specs to AISP symbolic notation
//! (`rosetta-aisp`), then validates the result and computes a quality tier by
//! semantic density δ (`aisp`). Natural language carries a 40–65% ambiguity
//! rate; AISP-validated specs drop that to <2%.
//!
//! Wired into `hooks_pre` for `kind == "task"`: when enabled it attaches an
//! `aisp` assessment to the response and, if a `min_tier` gate is configured
//! and `warn_only = false`, can mark the task `blocked` when the converted
//! spec scores below threshold.
//!
//! ## Dependencies (constraint-checked)
//! - `aisp` 0.1 — zero deps.
//! - `rosetta-aisp` 0.2 — deps already in the workspace (chrono/serde/regex/…).
//! - `rosetta-aisp-llm` is deliberately **NOT** used: it pulls `claude-agent-sdk-rs`
//!   (wraps the claude CLI), which would duplicate the ADR-032 `CliRouter` and
//!   bypass its claude→gemini→codex→openrouter priority. The LLM fallback, when
//!   added, routes through `CliRouter` instead.
//!
//! Config lives in `~/.ruvos/hooks.json` (JSON, consistent with `llm.json` — a
//! deliberate deviation from the ADR's `hooks.toml` to avoid a new `toml` dep).

use aisp::Tier;
use rosetta_aisp::AispConverter;
use serde_json::{json, Value};

/// AISP layer configuration, loaded from `~/.ruvos/hooks.json` `[aisp]`.
#[derive(Debug, Clone)]
pub struct AispConfig {
    /// Master switch. When false the layer is a no-op (default).
    pub enabled: bool,
    /// Minimum acceptable quality tier. `None` = no gate (assess only).
    pub min_tier: Option<Tier>,
    /// When true, a below-threshold tier is reported but never blocks.
    pub warn_only: bool,
    /// When true, prose is auto-converted to AISP; when false, validate only.
    pub auto_convert: bool,
}

impl Default for AispConfig {
    fn default() -> Self {
        // Off by default: existing workflows are unchanged until opted in.
        Self {
            enabled: false,
            min_tier: None,
            warn_only: true,
            auto_convert: true,
        }
    }
}

impl AispConfig {
    /// Load from `~/.ruvos/hooks.json` if present; fall back to defaults.
    pub fn load() -> Self {
        let path = crate::paths::data_root().join("hooks.json");
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                return Self::from_json(&v);
            }
        }
        Self::default()
    }

    fn from_json(v: &Value) -> Self {
        let mut c = Self::default();
        let a = &v["aisp"];
        if let Some(b) = a["enabled"].as_bool() {
            c.enabled = b;
        }
        if let Some(b) = a["warn_only"].as_bool() {
            c.warn_only = b;
        }
        if let Some(b) = a["auto_convert"].as_bool() {
            c.auto_convert = b;
        }
        if let Some(s) = a["min_tier"].as_str() {
            c.min_tier = parse_tier(s);
        }
        c
    }
}

/// Parse a tier name (case-insensitive) into an `aisp::Tier`.
pub fn parse_tier(s: &str) -> Option<Tier> {
    match s.trim().to_lowercase().as_str() {
        "platinum" => Some(Tier::Platinum),
        "gold" => Some(Tier::Gold),
        "silver" => Some(Tier::Silver),
        "bronze" => Some(Tier::Bronze),
        "reject" => Some(Tier::Reject),
        _ => None,
    }
}

/// Outcome of converting + validating a prose spec.
#[derive(Debug, Clone)]
pub struct AispAssessment {
    pub original: String,
    pub aisp_spec: String,
    pub tier: Tier,
    pub delta: f32,
    pub ambiguity: f32,
    pub confidence: f64,
    pub valid: bool,
    pub unmapped: Vec<String>,
    /// True only when a gate is configured, not warn_only, and tier < min_tier.
    pub blocked: bool,
}

impl AispAssessment {
    pub fn to_json(&self) -> Value {
        json!({
            "original": self.original,
            "aisp_spec": self.aisp_spec,
            "tier": self.tier.name(),
            "tier_symbol": self.tier.symbol(),
            "delta": self.delta,
            "ambiguity": self.ambiguity,
            "confidence": self.confidence,
            "valid": self.valid,
            "unmapped": self.unmapped,
            "blocked": self.blocked,
        })
    }
}

/// Convert `prose` to AISP, validate the result, and apply the configured gate.
///
/// When `cfg.auto_convert` is false, the prose is treated as already-AISP and
/// validated as-is (no conversion attempted).
pub fn assess(prose: &str, cfg: &AispConfig) -> AispAssessment {
    let (aisp_spec, confidence, unmapped) = if cfg.auto_convert {
        let r = AispConverter::convert(prose, None);
        (r.output, r.confidence, r.unmapped)
    } else {
        (prose.to_string(), 1.0, Vec::new())
    };

    let validation = aisp::validate(&aisp_spec);
    let tier = validation.tier;

    // Block only when a gate exists, it is not warn-only, and we are below it.
    let blocked = match cfg.min_tier {
        Some(min) => !cfg.warn_only && tier < min,
        None => false,
    };

    AispAssessment {
        original: prose.to_string(),
        aisp_spec,
        tier,
        delta: validation.delta,
        ambiguity: validation.ambiguity,
        confidence,
        valid: validation.valid,
        unmapped,
        blocked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_by_default() {
        let cfg = AispConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.min_tier.is_none());
        assert!(cfg.warn_only);
    }

    #[test]
    fn parse_tier_is_case_insensitive() {
        assert_eq!(parse_tier("Silver"), Some(Tier::Silver));
        assert_eq!(parse_tier("  PLATINUM "), Some(Tier::Platinum));
        assert_eq!(parse_tier("reject"), Some(Tier::Reject));
        assert_eq!(parse_tier("nonsense"), None);
    }

    #[test]
    fn from_json_reads_aisp_section() {
        let v = json!({
            "aisp": {
                "enabled": true,
                "min_tier": "gold",
                "warn_only": false,
                "auto_convert": true
            }
        });
        let c = AispConfig::from_json(&v);
        assert!(c.enabled);
        assert_eq!(c.min_tier, Some(Tier::Gold));
        assert!(!c.warn_only);
        assert!(c.auto_convert);
    }

    #[test]
    fn assess_produces_tier_and_spec() {
        let cfg = AispConfig {
            enabled: true,
            min_tier: None,
            warn_only: true,
            auto_convert: true,
        };
        let a = assess("For all users, if admin then allow access", &cfg);
        // No gate → never blocked, regardless of tier.
        assert!(!a.blocked);
        // Tier name is always one of the known tiers.
        assert!(["Platinum", "Gold", "Silver", "Bronze", "Reject"].contains(&a.tier.name()));
        // Round-trips through JSON cleanly.
        let j = a.to_json();
        assert_eq!(j["original"], "For all users, if admin then allow access");
        assert!(j["tier"].is_string());
    }

    #[test]
    fn gate_blocks_below_min_tier_when_not_warn_only() {
        // Force a hard gate at Platinum (almost nothing reaches it), warn_only off.
        let cfg = AispConfig {
            enabled: true,
            min_tier: Some(Tier::Platinum),
            warn_only: false,
            auto_convert: true,
        };
        let a = assess("do the thing somehow", &cfg);
        // Low-density prose is below Platinum → blocked.
        assert!(a.tier < Tier::Platinum, "vague prose should not reach Platinum");
        assert!(a.blocked, "sub-threshold tier with warn_only=false must block");
    }

    #[test]
    fn warn_only_never_blocks() {
        let cfg = AispConfig {
            enabled: true,
            min_tier: Some(Tier::Platinum),
            warn_only: true,
            auto_convert: true,
        };
        let a = assess("do the thing somehow", &cfg);
        assert!(!a.blocked, "warn_only must never block even below threshold");
    }
}
