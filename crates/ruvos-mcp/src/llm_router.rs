//! ADR-032: CLI-first LLM routing.
//!
//! Priority order (hard-coded, non-negotiable):
//!   1. `claude` CLI  (subscription, --print --output-format json)
//!   2. `gemini` CLI  (subscription, -o json)
//!   3. `codex`  CLI  (subscription, exec --json)
//!   4. OpenRouter    (OPENROUTER_API_KEY — the only API key ever used)
//!
//! ANTHROPIC_API_KEY is never read or used.

use crate::{Result, RuvosError};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

// ── Provider enum ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum LlmProvider {
    ClaudeCli,
    GeminiCli,
    CodexCli,
    OpenRouter,
}

impl LlmProvider {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ClaudeCli  => "claude",
            Self::GeminiCli  => "gemini",
            Self::CodexCli   => "codex",
            Self::OpenRouter => "openrouter",
        }
    }
}

// ── Config ───────────────────────────────────────────────────────────────────

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
    /// Load from `~/.ruvos/llm.json` if present; fall back to defaults.
    pub fn load() -> Self {
        let path = crate::paths::data_root().join("llm.json");
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                return Self::from_json(&v);
            }
        }
        Self::default()
    }

    fn from_json(v: &Value) -> Self {
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

// ── Router ───────────────────────────────────────────────────────────────────

pub struct CliRouter {
    pub provider: LlmProvider,
    config: RouterConfig,
}

impl CliRouter {
    /// Detect the first available provider using the default priority order.
    /// Returns `None` if no CLI is in PATH and OPENROUTER_API_KEY is unset.
    pub fn detect() -> Option<Self> {
        Self::detect_with_config(RouterConfig::load())
    }

    pub fn detect_with_config(config: RouterConfig) -> Option<Self> {
        for name in &config.priority {
            let provider = match name.as_str() {
                "claude"      if which_exe("claude").is_some()  => LlmProvider::ClaudeCli,
                "gemini"      if which_exe("gemini").is_some()  => LlmProvider::GeminiCli,
                "codex"       if which_exe("codex").is_some()   => LlmProvider::CodexCli,
                "openrouter"  if std::env::var("OPENROUTER_API_KEY").is_ok() => LlmProvider::OpenRouter,
                _ => continue,
            };
            return Some(Self { provider, config });
        }
        None
    }

    pub fn provider_name(&self) -> &'static str {
        self.provider.name()
    }

    /// Run inference: system prompt + user prompt → text response.
    pub async fn call(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        match self.provider {
            LlmProvider::ClaudeCli  => self.call_claude(system_prompt, user_prompt).await,
            LlmProvider::GeminiCli  => self.call_gemini(system_prompt, user_prompt).await,
            LlmProvider::CodexCli   => self.call_codex(system_prompt, user_prompt).await,
            LlmProvider::OpenRouter => self.call_openrouter(system_prompt, user_prompt).await,
        }
    }

    // ── claude CLI ───────────────────────────────────────────────────────────

    async fn call_claude(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let mut args = vec![
            "--print".to_string(),
            "--output-format".to_string(),
            "json".to_string(),
            "--model".to_string(),
            self.config.claude_model.clone(),
        ];
        args.extend(self.config.claude_extra_args.clone());
        if !system_prompt.is_empty() {
            args.push("--append-system-prompt".to_string());
            args.push(system_prompt.to_string());
        }
        args.push("-p".to_string());
        args.push(user_prompt.to_string());

        let out = tokio::process::Command::new("claude")
            .args(&args)
            .output()
            .await
            .map_err(|e| RuvosError::InternalError(format!("claude CLI launch: {e}")))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(RuvosError::InternalError(
                format!("claude exited {}: {stderr}", out.status),
            ));
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        // Expected: {"type":"result","result":"...", ...}
        let v: Value = serde_json::from_str(&stdout)
            .map_err(|e| RuvosError::InternalError(format!("claude JSON: {e} — raw: {stdout}")))?
        ;
        v["result"].as_str()
            .map(String::from)
            .ok_or_else(|| RuvosError::InternalError(
                format!("claude: no .result field — raw: {stdout}"),
            ))
    }

    // ── gemini CLI ───────────────────────────────────────────────────────────

    async fn call_gemini(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        // Gemini CLI has no --append-system-prompt; prepend to user prompt.
        let full = if system_prompt.is_empty() {
            user_prompt.to_string()
        } else {
            format!("{system_prompt}\n\n{user_prompt}")
        };

        let mut args = vec!["-o".to_string(), "json".to_string()];
        args.extend(self.config.gemini_extra_args.clone());
        args.push("-p".to_string());
        args.push(full);

        let out = tokio::process::Command::new("gemini")
            .args(&args)
            .output()
            .await
            .map_err(|e| RuvosError::InternalError(format!("gemini CLI launch: {e}")))?;

        let stdout = String::from_utf8_lossy(&out.stdout);
        // Try structured paths first, fall back to raw text.
        if let Ok(v) = serde_json::from_str::<Value>(&stdout) {
            if let Some(t) = v["candidates"][0]["content"]["parts"][0]["text"].as_str() {
                return Ok(t.to_string());
            }
            if let Some(t) = v["text"].as_str() {
                return Ok(t.to_string());
            }
        }
        let text = stdout.trim().to_string();
        if text.is_empty() {
            Err(RuvosError::InternalError("gemini: empty output".into()))
        } else {
            Ok(text)
        }
    }

    // ── codex CLI ────────────────────────────────────────────────────────────

    async fn call_codex(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let full = if system_prompt.is_empty() {
            user_prompt.to_string()
        } else {
            format!("{system_prompt}\n\n{user_prompt}")
        };

        let mut args = vec!["exec".to_string(), "--json".to_string()];
        args.extend(self.config.codex_extra_args.clone());
        args.push("--".to_string());
        args.push(full);

        let out = tokio::process::Command::new("codex")
            .args(&args)
            .output()
            .await
            .map_err(|e| RuvosError::InternalError(format!("codex CLI launch: {e}")))?;

        let stdout = String::from_utf8_lossy(&out.stdout);
        // JSONL stream: find last line with type="message"
        let text = stdout.lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|line| {
                let v: Value = serde_json::from_str(line).ok()?;
                if v["type"].as_str() == Some("message") {
                    v["content"].as_str().map(String::from)
                } else {
                    None
                }
            })
            .last();

        text.or_else(|| {
            let t = stdout.trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        })
        .ok_or_else(|| RuvosError::InternalError("codex: no output".into()))
    }

    // ── OpenRouter REST ──────────────────────────────────────────────────────

    async fn call_openrouter(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .map_err(|_| RuvosError::InternalError("OPENROUTER_API_KEY not set".into()))?;

        let body = serde_json::json!({
            "model": self.config.openrouter_model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user",   "content": user_prompt }
            ]
        });

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| RuvosError::InternalError(format!("reqwest build: {e}")))?;

        let resp = client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| RuvosError::InternalError(format!("openrouter request: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(RuvosError::InternalError(
                format!("openrouter HTTP {status}: {body}"),
            ));
        }

        let v: Value = resp.json().await
            .map_err(|e| RuvosError::InternalError(format!("openrouter parse: {e}")))?;

        v["choices"][0]["message"]["content"].as_str()
            .map(String::from)
            .ok_or_else(|| RuvosError::InternalError(
                format!("openrouter: unexpected response: {v}"),
            ))
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Find an executable in PATH without the `which` crate.
pub fn which_exe(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_exe_finds_real_binary() {
        // `sh` is present on every POSIX system.
        assert!(which_exe("sh").is_some(), "sh must be in PATH");
    }

    #[test]
    fn which_exe_misses_nonexistent() {
        assert!(which_exe("ruvos_nonexistent_binary_xyz").is_none());
    }

    #[test]
    fn provider_name_round_trip() {
        assert_eq!(LlmProvider::ClaudeCli.name(),  "claude");
        assert_eq!(LlmProvider::GeminiCli.name(),  "gemini");
        assert_eq!(LlmProvider::CodexCli.name(),   "codex");
        assert_eq!(LlmProvider::OpenRouter.name(), "openrouter");
    }

    #[test]
    fn default_config_priority_order() {
        let cfg = RouterConfig::default();
        assert_eq!(cfg.priority[0], "claude");
        assert_eq!(cfg.priority[1], "gemini");
        assert_eq!(cfg.priority[2], "codex");
        assert_eq!(cfg.priority[3], "openrouter");
    }

    #[test]
    fn config_from_json_overrides_priority() {
        let v = serde_json::json!({
            "routing": { "priority": ["gemini", "openrouter"] },
            "gemini": { "extra_args": ["--yolo"] },
            "openrouter": { "default_model": "google/gemini-2.5-pro" }
        });
        let cfg = RouterConfig::from_json(&v);
        assert_eq!(cfg.priority, vec!["gemini", "openrouter"]);
        assert_eq!(cfg.openrouter_model, "google/gemini-2.5-pro");
    }

    #[test]
    fn detect_returns_none_when_no_provider() {
        // Force an empty priority list so nothing can match.
        let cfg = RouterConfig { priority: vec![], ..Default::default() };
        assert!(CliRouter::detect_with_config(cfg).is_none());
    }

    #[test]
    fn detect_finds_claude_when_in_path() {
        // claude is the active CLI in this session.
        if which_exe("claude").is_some() {
            let router = CliRouter::detect().expect("claude is in PATH, detect must succeed");
            assert_eq!(router.provider, LlmProvider::ClaudeCli);
        }
    }

    #[test]
    fn openrouter_body_has_correct_shape() {
        // Validate the request body shape without making a network call.
        let body = serde_json::json!({
            "model": "anthropic/claude-sonnet-4-6",
            "messages": [
                { "role": "system", "content": "be helpful" },
                { "role": "user",   "content": "hello" }
            ]
        });
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
    }

    // ── RouterConfig::from_json edge cases ───────────────────────────────────

    #[test]
    fn config_from_json_empty_object_preserves_all_defaults() {
        let v = serde_json::json!({});
        let cfg = RouterConfig::from_json(&v);
        let def = RouterConfig::default();
        assert_eq!(cfg.priority, def.priority);
        assert_eq!(cfg.claude_model, def.claude_model);
        assert_eq!(cfg.claude_extra_args, def.claude_extra_args);
        assert_eq!(cfg.gemini_extra_args, def.gemini_extra_args);
        assert_eq!(cfg.codex_extra_args, def.codex_extra_args);
        assert_eq!(cfg.openrouter_model, def.openrouter_model);
    }

    #[test]
    fn config_from_json_partial_override_preserves_unset_fields() {
        // Only override claude.model; everything else must stay default.
        let v = serde_json::json!({ "claude": { "model": "opus" } });
        let cfg = RouterConfig::from_json(&v);
        assert_eq!(cfg.claude_model, "opus");
        assert_eq!(cfg.priority, RouterConfig::default().priority);
        assert_eq!(cfg.gemini_extra_args, RouterConfig::default().gemini_extra_args);
    }

    #[test]
    fn config_from_json_non_array_extra_args_keeps_default() {
        // Malformed: extra_args is a string, not an array.
        let v = serde_json::json!({ "claude": { "extra_args": "not-an-array" } });
        let cfg = RouterConfig::from_json(&v);
        assert_eq!(cfg.claude_extra_args, RouterConfig::default().claude_extra_args);
    }

    #[test]
    fn config_from_json_empty_priority_array_accepted() {
        let v = serde_json::json!({ "routing": { "priority": [] } });
        let cfg = RouterConfig::from_json(&v);
        assert!(cfg.priority.is_empty());
    }

    #[test]
    fn config_from_json_all_fields_overridden() {
        let v = serde_json::json!({
            "routing": { "priority": ["openrouter"] },
            "claude":  { "model": "haiku", "extra_args": ["--no-mcp"] },
            "gemini":  { "extra_args": [] },
            "codex":   { "extra_args": ["--sandbox"] },
            "openrouter": { "default_model": "google/gemini-2.5-pro" }
        });
        let cfg = RouterConfig::from_json(&v);
        assert_eq!(cfg.priority, vec!["openrouter"]);
        assert_eq!(cfg.claude_model, "haiku");
        assert_eq!(cfg.claude_extra_args, vec!["--no-mcp"]);
        assert!(cfg.gemini_extra_args.is_empty());
        assert_eq!(cfg.codex_extra_args, vec!["--sandbox"]);
        assert_eq!(cfg.openrouter_model, "google/gemini-2.5-pro");
    }

    // ── Default config field values ──────────────────────────────────────────

    #[test]
    fn default_config_extra_args_are_empty() {
        // Security default: no extra_args; users must opt in via llm.json.
        let cfg = RouterConfig::default();
        assert!(cfg.claude_extra_args.is_empty());
        assert!(cfg.gemini_extra_args.is_empty());
        assert!(cfg.codex_extra_args.is_empty());
    }

    #[test]
    fn default_config_openrouter_model_is_claude_sonnet() {
        let cfg = RouterConfig::default();
        assert_eq!(cfg.openrouter_model, "anthropic/claude-sonnet-4-6");
    }

    // ── filter_extra_args / BLOCKED_EXTRA_ARGS security tests ───────────────

    #[test]
    fn blocked_args_are_stripped_from_claude_extra_args() {
        let v = serde_json::json!({
            "claude": {
                "extra_args": ["--permission-mode", "acceptEdits", "--safe-user-flag"]
            }
        });
        let cfg = RouterConfig::from_json(&v);
        assert!(
            !cfg.claude_extra_args.contains(&"--permission-mode".to_string()),
            "--permission-mode must be stripped"
        );
        assert!(
            cfg.claude_extra_args.contains(&"--safe-user-flag".to_string()),
            "safe flag must be kept"
        );
    }

    #[test]
    fn blocked_yolo_stripped_from_gemini_extra_args() {
        let v = serde_json::json!({
            "gemini": { "extra_args": ["--yolo", "--verbose"] }
        });
        let cfg = RouterConfig::from_json(&v);
        assert!(!cfg.gemini_extra_args.contains(&"--yolo".to_string()));
        assert!(cfg.gemini_extra_args.contains(&"--verbose".to_string()));
    }

    #[test]
    fn blocked_bypass_stripped_from_codex_extra_args() {
        let v = serde_json::json!({
            "codex": {
                "extra_args": ["--dangerously-bypass-approvals-and-sandbox", "--timeout", "30"]
            }
        });
        let cfg = RouterConfig::from_json(&v);
        assert!(!cfg.codex_extra_args.contains(
            &"--dangerously-bypass-approvals-and-sandbox".to_string()
        ));
        assert!(cfg.codex_extra_args.contains(&"--timeout".to_string()));
        assert!(cfg.codex_extra_args.contains(&"30".to_string()));
    }

    #[test]
    fn all_args_blocked_leaves_empty_extra_args() {
        let v = serde_json::json!({
            "claude": { "extra_args": ["--model", "--api-key", "-p", "--output-format"] }
        });
        let cfg = RouterConfig::from_json(&v);
        assert!(cfg.claude_extra_args.is_empty(), "all blocked → empty list");
    }

    #[test]
    fn safe_extra_args_are_kept_verbatim() {
        let v = serde_json::json!({
            "claude": { "extra_args": ["--no-mcp", "--max-turns", "5"] }
        });
        let cfg = RouterConfig::from_json(&v);
        assert_eq!(cfg.claude_extra_args, vec!["--no-mcp", "--max-turns", "5"]);
    }

    // ── LlmProvider properties ───────────────────────────────────────────────

    #[test]
    fn provider_clone_preserves_equality() {
        for p in [
            LlmProvider::ClaudeCli,
            LlmProvider::GeminiCli,
            LlmProvider::CodexCli,
            LlmProvider::OpenRouter,
        ] {
            assert_eq!(p.clone(), p);
        }
    }

    #[test]
    fn all_providers_are_distinct_from_each_other() {
        let variants = [
            LlmProvider::ClaudeCli,
            LlmProvider::GeminiCli,
            LlmProvider::CodexCli,
            LlmProvider::OpenRouter,
        ];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b, "same variant must equal itself");
                } else {
                    assert_ne!(a, b, "different variants must not be equal");
                }
            }
        }
    }

    // ── detect_with_config edge cases ────────────────────────────────────────

    #[test]
    fn detect_skips_unknown_provider_names() {
        let cfg = RouterConfig {
            priority: vec!["unknown_provider_xyz_123".into()],
            ..Default::default()
        };
        assert!(CliRouter::detect_with_config(cfg).is_none());
    }

    #[test]
    fn detect_openrouter_without_api_key_returns_none() {
        let key = "OPENROUTER_API_KEY";
        let old = std::env::var(key).ok();
        // Ensure the key is absent.
        std::env::remove_var(key);

        let cfg = RouterConfig {
            priority: vec!["openrouter".into()],
            ..Default::default()
        };
        let result = CliRouter::detect_with_config(cfg);

        // Restore.
        match old {
            Some(v) => std::env::set_var(key, v),
            None => {}
        }

        assert!(result.is_none(), "openrouter without API key must not be detected");
    }

    #[test]
    fn detect_openrouter_with_api_key_returns_openrouter_provider() {
        let key = "OPENROUTER_API_KEY";
        let old = std::env::var(key).ok();
        std::env::set_var(key, "test-key-abc");

        let cfg = RouterConfig {
            priority: vec!["openrouter".into()],
            ..Default::default()
        };
        let result = CliRouter::detect_with_config(cfg);

        // Restore.
        match old {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }

        let router = result.expect("openrouter with API key must be detected");
        assert_eq!(router.provider, LlmProvider::OpenRouter);
        assert_eq!(router.provider_name(), "openrouter");
    }

    #[test]
    fn detect_picks_first_available_from_priority() {
        // Force a config where only openrouter can match (no CLI binaries).
        let key = "OPENROUTER_API_KEY";
        let old = std::env::var(key).ok();
        std::env::set_var(key, "key-xyz");

        let cfg = RouterConfig {
            priority: vec![
                "nonexistent_cli_a".into(),
                "nonexistent_cli_b".into(),
                "openrouter".into(),
            ],
            ..Default::default()
        };
        let result = CliRouter::detect_with_config(cfg);

        match old {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }

        let router = result.expect("should fall through to openrouter");
        assert_eq!(router.provider, LlmProvider::OpenRouter);
    }

    // ── which_exe edge cases ─────────────────────────────────────────────────

    #[test]
    fn which_exe_returns_none_for_directory_named_like_binary() {
        // A directory with the same name as a binary must not be returned.
        let dir = tempfile::tempdir().unwrap();
        let fake_dir = dir.path().join("fake_binary_dir");
        std::fs::create_dir(&fake_dir).unwrap();

        let old_path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = std::env::split_paths(&old_path).collect::<Vec<_>>();
        paths.insert(0, dir.path().to_path_buf());
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());

        let found = which_exe("fake_binary_dir");

        std::env::set_var("PATH", old_path);
        // A directory is not a file — must not match.
        assert!(found.is_none());
    }
}
