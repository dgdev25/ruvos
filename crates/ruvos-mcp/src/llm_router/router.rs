use super::config::RouterConfig;
use super::LlmProvider;
use crate::{Result, RuvosError};
use serde_json::Value;
use std::path::PathBuf;

pub struct CliRouter {
    pub provider: LlmProvider,
    pub(super) config: RouterConfig,
}

impl CliRouter {
    /// Detect the first available provider using the default priority order.
    /// Returns `None` if no CLI is in PATH and OPENROUTER_API_KEY is unset.
    pub fn detect() -> Option<Self> {
        // Escape hatch: when RUVOS_DISABLE_CLI_ROUTER is truthy, skip all
        // provider auto-detection and return None, so callers fall back to
        // their non-router path (e.g. run_task's deterministic placeholder).
        // Honored ONLY here — not in detect_with_config — so the routing-logic
        // unit tests that call detect_with_config(explicit_cfg) stay pure, and
        // hermetic tests / operators can force no-router mode without touching
        // PATH or OPENROUTER_API_KEY. Production default is unchanged (unset).
        if std::env::var("RUVOS_DISABLE_CLI_ROUTER")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            return None;
        }
        Self::detect_with_config(RouterConfig::load())
    }

    pub fn detect_with_config(config: RouterConfig) -> Option<Self> {
        let openrouter_key = std::env::var("OPENROUTER_API_KEY").ok();
        Self::detect_with_config_and_key(config, openrouter_key.as_deref())
    }

    /// Provider-selection logic with the OpenRouter key supplied explicitly.
    /// Separated from the env read so tests can exercise selection without
    /// mutating the process-global `OPENROUTER_API_KEY` (which would race
    /// concurrent tests that read it).
    pub(crate) fn detect_with_config_and_key(
        config: RouterConfig,
        openrouter_key: Option<&str>,
    ) -> Option<Self> {
        for name in &config.priority {
            let provider = match name.as_str() {
                "claude" if which_exe("claude").is_some() => LlmProvider::ClaudeCli,
                "gemini" if which_exe("gemini").is_some() => LlmProvider::GeminiCli,
                "codex" if which_exe("codex").is_some() => LlmProvider::CodexCli,
                "openrouter" if openrouter_key.is_some() => LlmProvider::OpenRouter,
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
            LlmProvider::ClaudeCli => self.call_claude(system_prompt, user_prompt).await,
            LlmProvider::GeminiCli => self.call_gemini(system_prompt, user_prompt).await,
            LlmProvider::CodexCli => self.call_codex(system_prompt, user_prompt).await,
            LlmProvider::OpenRouter => self.call_openrouter(system_prompt, user_prompt).await,
        }
    }

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
            return Err(RuvosError::InternalError(format!(
                "claude exited {}: {stderr}",
                out.status
            )));
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        let v: Value = serde_json::from_str(&stdout)
            .map_err(|e| RuvosError::InternalError(format!("claude JSON: {e} — raw: {stdout}")))?;
        v["result"].as_str().map(String::from).ok_or_else(|| {
            RuvosError::InternalError(format!("claude: no .result field — raw: {stdout}"))
        })
    }

    async fn call_gemini(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
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
        parse_gemini_output(&stdout)
    }

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
        parse_codex_output(&stdout)
    }

    async fn call_openrouter(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .map_err(|_| RuvosError::InternalError("OPENROUTER_API_KEY not set".into()))?;

        let body = serde_json::json!({
            "model": self.config.openrouter_model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user",   "content": user_prompt }
            ]
        })
        .to_string();

        // curl subprocess — zero extra deps, avoids ring/rustls compile overhead.
        let out = tokio::process::Command::new("curl")
            .args([
                "-s",
                "-X",
                "POST",
                "https://openrouter.ai/api/v1/chat/completions",
                "-H",
                &format!("Authorization: Bearer {api_key}"),
                "-H",
                "Content-Type: application/json",
                "-d",
                &body,
            ])
            .output()
            .await
            .map_err(|e| RuvosError::InternalError(format!("curl launch: {e}")))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(RuvosError::InternalError(format!(
                "curl exited {}: {stderr}",
                out.status
            )));
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        let v: Value = serde_json::from_str(&stdout).map_err(|e| {
            RuvosError::InternalError(format!("openrouter JSON: {e} — raw: {stdout}"))
        })?;

        v["choices"][0]["message"]["content"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| {
                RuvosError::InternalError(format!("openrouter: unexpected response: {v}"))
            })
    }
}

// ── Output parsers ───────────────────────────────────────────────────────────

/// Parse Gemini JSON stdout: try `candidates[0].content.parts[0].text`, then
/// `text` top-level field, then fall back to trimmed raw text.
pub(super) fn parse_gemini_output(stdout: &str) -> Result<String> {
    if let Ok(v) = serde_json::from_str::<Value>(stdout) {
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

/// Parse Codex JSONL stdout: find last line with `type=message` and return
/// `content`. Falls back to trimmed raw text when no message lines exist.
pub(super) fn parse_codex_output(stdout: &str) -> Result<String> {
    let text = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let v: Value = serde_json::from_str(line).ok()?;
            if v["type"].as_str() == Some("message") {
                v["content"].as_str().map(String::from)
            } else {
                None
            }
        })
        .next_back();

    text.or_else(|| {
        let t = stdout.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    })
    .ok_or_else(|| RuvosError::InternalError("codex: no output".into()))
}

// ── PATH helper ───────────────────────────────────────────────────────────────

/// Find an executable in PATH without the `which` crate.
pub fn which_exe(name: &str) -> Option<PathBuf> {
    which_exe_in(name, std::env::var_os("PATH"))
}

/// Resolve `name` against an explicit PATH-style search list. Separated from
/// the global-env read in [`which_exe`] so tests can exercise PATH resolution
/// without mutating the process-global `PATH` — mutating it races every
/// concurrent test that spawns a bare-named subprocess (it would fail their
/// PATH lookup with ENOENT).
pub(crate) fn which_exe_in(name: &str, path: Option<std::ffi::OsString>) -> Option<PathBuf> {
    path.map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── detect() escape hatch ────────────────────────────────────────────────

    #[test]
    fn detect_returns_none_when_disabled() {
        // Monotonic: set and never unset (matches the agent test isolate()
        // convention), so this introduces no cross-test env race. No test
        // expects detect() to return Some, so a process-wide disable is safe.
        std::env::set_var("RUVOS_DISABLE_CLI_ROUTER", "1");
        assert!(
            CliRouter::detect().is_none(),
            "RUVOS_DISABLE_CLI_ROUTER must short-circuit detect() to None"
        );
    }

    // ── parse_codex_output ───────────────────────────────────────────────────

    #[test]
    fn codex_single_message_line_extracts_content() {
        let out = r#"{"type":"message","content":"hello world"}"#;
        assert_eq!(parse_codex_output(out).unwrap(), "hello world");
    }

    #[test]
    fn codex_multiple_message_lines_last_wins() {
        let out = "{\"type\":\"message\",\"content\":\"first\"}\n\
                   {\"type\":\"message\",\"content\":\"last\"}";
        assert_eq!(parse_codex_output(out).unwrap(), "last");
    }

    #[test]
    fn codex_non_message_type_skipped() {
        let out = "{\"type\":\"delta\",\"content\":\"skip\"}\n\
                   {\"type\":\"message\",\"content\":\"kept\"}";
        assert_eq!(parse_codex_output(out).unwrap(), "kept");
    }

    #[test]
    fn codex_malformed_lines_skipped_valid_still_parsed() {
        let out = "not json at all\n{\"type\":\"message\",\"content\":\"good\"}";
        assert_eq!(parse_codex_output(out).unwrap(), "good");
    }

    #[test]
    fn codex_no_message_type_non_empty_raw_falls_back() {
        let out = "{\"type\":\"delta\",\"content\":\"nope\"}\nsome fallback text";
        let result = parse_codex_output(out).unwrap();
        assert!(!result.is_empty(), "should fall back to non-empty raw text");
    }

    #[test]
    fn codex_empty_stdout_returns_err() {
        let err = parse_codex_output("").unwrap_err();
        assert!(format!("{err:?}").contains("codex: no output"));
    }

    #[test]
    fn codex_whitespace_only_returns_err() {
        assert!(parse_codex_output("   \n  \n").is_err());
    }

    // ── parse_gemini_output ──────────────────────────────────────────────────

    #[test]
    fn gemini_deep_candidates_path_extracted() {
        let out = r#"{"candidates":[{"content":{"parts":[{"text":"deep result"}]}}]}"#;
        assert_eq!(parse_gemini_output(out).unwrap(), "deep result");
    }

    #[test]
    fn gemini_top_level_text_field_extracted() {
        let out = r#"{"text":"top result"}"#;
        assert_eq!(parse_gemini_output(out).unwrap(), "top result");
    }

    #[test]
    fn gemini_non_json_returns_trimmed_raw_text() {
        let out = "  plain text output  ";
        assert_eq!(parse_gemini_output(out).unwrap(), "plain text output");
    }

    #[test]
    fn gemini_empty_stdout_returns_err() {
        let err = parse_gemini_output("").unwrap_err();
        assert!(format!("{err:?}").contains("gemini: empty output"));
    }

    #[test]
    fn gemini_json_without_known_paths_falls_back_to_raw() {
        let out = r#"{"unknown_key":"value"}"#;
        let result = parse_gemini_output(out).unwrap();
        assert!(
            !result.is_empty(),
            "valid JSON with no known paths → raw text"
        );
    }

    #[test]
    fn gemini_whitespace_only_returns_err() {
        assert!(parse_gemini_output("   ").is_err());
    }
}
