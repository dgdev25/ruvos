use serde_json::Value;

#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// Ordered list of provider names to probe (first available wins).
    pub priority: Vec<String>,
    pub claude_model: String,
    pub claude_extra_args: Vec<String>,
    pub gemini_extra_args: Vec<String>,
    pub codex_extra_args: Vec<String>,
    pub openrouter_model: String,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            priority: vec![
                "claude".into(),
                "gemini".into(),
                "codex".into(),
                "openrouter".into(),
            ],
            claude_model: "sonnet".into(),
            claude_extra_args: vec![],
            gemini_extra_args: vec![],
            codex_extra_args: vec![],
            openrouter_model: "anthropic/claude-sonnet-4-6".into(),
        }
    }
}

/// Args that, if present in user-supplied `extra_args`, would bypass CLI safety
/// gates or override security-sensitive settings. Filtered from `llm.json` so
/// a world-writable config file cannot escalate privileges.
const BLOCKED_EXTRA_ARGS: &[&str] = &[
    "--dangerously-bypass-approvals-and-sandbox",
    "--yolo",
    "--permission-mode",
    "--model",
    "--api-key",
    "--output-format",
    "--append-system-prompt",
    "-p",
];

fn filter_extra_args(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .filter(|a| {
            let blocked = BLOCKED_EXTRA_ARGS
                .iter()
                .any(|b| a == b || a.starts_with(&format!("{b}=")));
            if blocked {
                tracing::warn!("blocked extra_arg from llm.json: {a}");
            }
            !blocked
        })
        .collect()
}

impl RouterConfig {
    /// Load from `.ruvos/llm.json` if present; fall back to defaults.
    pub fn load() -> Self {
        let path = crate::paths::data_root().join("llm.json");
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                return Self::from_json(&v);
            }
        }
        Self::default()
    }

    pub(super) fn from_json(v: &Value) -> Self {
        let mut cfg = Self::default();
        if let Some(arr) = v["routing"]["priority"].as_array() {
            cfg.priority = arr.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect();
        }
        if let Some(m) = v["claude"]["model"].as_str() {
            cfg.claude_model = m.to_string();
        }
        if let Some(arr) = v["claude"]["extra_args"].as_array() {
            cfg.claude_extra_args = filter_extra_args(
                arr.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            );
        }
        if let Some(arr) = v["gemini"]["extra_args"].as_array() {
            cfg.gemini_extra_args = filter_extra_args(
                arr.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            );
        }
        if let Some(arr) = v["codex"]["extra_args"].as_array() {
            cfg.codex_extra_args = filter_extra_args(
                arr.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            );
        }
        if let Some(m) = v["openrouter"]["default_model"].as_str() {
            cfg.openrouter_model = m.to_string();
        }
        cfg
    }
}
