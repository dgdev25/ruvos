//! ADR-032: CLI-first LLM routing.
//!
//! Priority order (hard-coded, non-negotiable):
//!   1. `claude` CLI  (subscription, --print --output-format json)
//!   2. `gemini` CLI  (subscription, -o json)
//!   3. `codex`  CLI  (subscription, exec --json)
//!   4. OpenRouter    (OPENROUTER_API_KEY — the only API key ever used)
//!
//! ANTHROPIC_API_KEY is never read or used.

mod config;
mod router;

pub use config::RouterConfig;
#[cfg(test)]
pub(crate) use router::which_exe_in;
pub use router::{which_exe, CliRouter};

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
            Self::ClaudeCli => "claude",
            Self::GeminiCli => "gemini",
            Self::CodexCli => "codex",
            Self::OpenRouter => "openrouter",
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_exe_finds_real_binary() {
        assert!(which_exe("sh").is_some(), "sh must be in PATH");
    }

    #[test]
    fn which_exe_misses_nonexistent() {
        assert!(which_exe("ruvos_nonexistent_binary_xyz").is_none());
    }

    #[test]
    fn provider_name_round_trip() {
        assert_eq!(LlmProvider::ClaudeCli.name(), "claude");
        assert_eq!(LlmProvider::GeminiCli.name(), "gemini");
        assert_eq!(LlmProvider::CodexCli.name(), "codex");
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
        let cfg = RouterConfig {
            priority: vec![],
            ..Default::default()
        };
        assert!(CliRouter::detect_with_config(cfg).is_none());
    }

    #[test]
    fn detect_finds_claude_when_in_path() {
        if which_exe("claude").is_some() {
            // Use detect_with_config to bypass the RUVOS_DISABLE_CLI_ROUTER flag
            // that other tests may have set permanently for the process.
            let cfg = RouterConfig {
                priority: vec!["claude".into()],
                ..Default::default()
            };
            let router =
                CliRouter::detect_with_config(cfg).expect("claude is in PATH, detect must succeed");
            assert_eq!(router.provider, LlmProvider::ClaudeCli);
        }
    }

    #[test]
    fn openrouter_body_has_correct_shape() {
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
        let v = serde_json::json!({ "claude": { "model": "opus" } });
        let cfg = RouterConfig::from_json(&v);
        assert_eq!(cfg.claude_model, "opus");
        assert_eq!(cfg.priority, RouterConfig::default().priority);
        assert_eq!(
            cfg.gemini_extra_args,
            RouterConfig::default().gemini_extra_args
        );
    }

    #[test]
    fn config_from_json_non_array_extra_args_keeps_default() {
        let v = serde_json::json!({ "claude": { "extra_args": "not-an-array" } });
        let cfg = RouterConfig::from_json(&v);
        assert_eq!(
            cfg.claude_extra_args,
            RouterConfig::default().claude_extra_args
        );
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

    #[test]
    fn default_config_extra_args_are_empty() {
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

    #[test]
    fn blocked_args_are_stripped_from_claude_extra_args() {
        let v = serde_json::json!({
            "claude": {
                "extra_args": ["--permission-mode", "acceptEdits", "--safe-user-flag"]
            }
        });
        let cfg = RouterConfig::from_json(&v);
        assert!(
            !cfg.claude_extra_args
                .contains(&"--permission-mode".to_string()),
            "--permission-mode must be stripped"
        );
        assert!(
            cfg.claude_extra_args
                .contains(&"--safe-user-flag".to_string()),
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
        assert!(!cfg
            .codex_extra_args
            .contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
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

    #[test]
    fn detect_skips_unknown_provider_names() {
        let cfg = RouterConfig {
            priority: vec!["unknown_provider_xyz_123".into()],
            ..Default::default()
        };
        assert!(CliRouter::detect_with_config(cfg).is_none());
    }

    // These selection/resolution tests inject the OpenRouter key and the PATH
    // search list directly instead of mutating the process environment. The
    // old env-mutating versions raced every concurrent test that read PATH or
    // OPENROUTER_API_KEY — in particular subprocess spawns failed with ENOENT
    // while PATH was swapped out.

    #[test]
    fn detect_openrouter_without_api_key_returns_none() {
        let cfg = RouterConfig {
            priority: vec!["openrouter".into()],
            ..Default::default()
        };
        let result = CliRouter::detect_with_config_and_key(cfg, None);
        assert!(
            result.is_none(),
            "openrouter without API key must not be detected"
        );
    }

    #[test]
    fn detect_openrouter_with_api_key_returns_openrouter_provider() {
        let cfg = RouterConfig {
            priority: vec!["openrouter".into()],
            ..Default::default()
        };
        let result = CliRouter::detect_with_config_and_key(cfg, Some("test-key-abc"));
        let router = result.expect("openrouter with API key must be detected");
        assert_eq!(router.provider, LlmProvider::OpenRouter);
        assert_eq!(router.provider_name(), "openrouter");
    }

    #[test]
    fn detect_picks_first_available_from_priority() {
        let cfg = RouterConfig {
            priority: vec![
                "nonexistent_cli_a".into(),
                "nonexistent_cli_b".into(),
                "openrouter".into(),
            ],
            ..Default::default()
        };
        let result = CliRouter::detect_with_config_and_key(cfg, Some("key-xyz"));
        let router = result.expect("should fall through to openrouter");
        assert_eq!(router.provider, LlmProvider::OpenRouter);
    }

    #[test]
    fn which_exe_returns_none_for_directory_named_like_binary() {
        let dir = tempfile::tempdir().unwrap();
        let fake_dir = dir.path().join("fake_binary_dir");
        std::fs::create_dir(&fake_dir).unwrap();

        // A directory named like the binary must not count as an executable.
        let path = std::env::join_paths([dir.path().to_path_buf()]).unwrap();
        assert!(which_exe_in("fake_binary_dir", Some(path)).is_none());
    }

    #[test]
    fn which_exe_with_nonexistent_path_dirs_returns_none() {
        let path =
            std::ffi::OsString::from("/nonexistent_ruvos_dir_1/bin:/nonexistent_ruvos_dir_2");
        assert!(
            which_exe_in("sh", Some(path)).is_none(),
            "all PATH dirs nonexistent → sh not found"
        );
    }

    #[test]
    fn provider_debug_format_is_non_empty() {
        for p in [
            LlmProvider::ClaudeCli,
            LlmProvider::GeminiCli,
            LlmProvider::CodexCli,
            LlmProvider::OpenRouter,
        ] {
            assert!(!format!("{p:?}").is_empty());
        }
    }

    #[test]
    fn config_from_json_non_string_model_keeps_default() {
        let v = serde_json::json!({ "claude": { "model": 42 } });
        let cfg = RouterConfig::from_json(&v);
        assert_eq!(cfg.claude_model, RouterConfig::default().claude_model);
    }

    #[test]
    fn config_from_json_null_fields_keep_defaults() {
        let v = serde_json::json!({
            "claude": { "model": null, "extra_args": null },
            "openrouter": { "default_model": null }
        });
        let cfg = RouterConfig::from_json(&v);
        assert_eq!(cfg.claude_model, RouterConfig::default().claude_model);
        assert_eq!(
            cfg.openrouter_model,
            RouterConfig::default().openrouter_model
        );
    }

    #[test]
    fn config_from_json_unknown_keys_ignored() {
        let v = serde_json::json!({
            "totally_unknown": "foo",
            "another_unknown": { "deep": "value" }
        });
        let cfg = RouterConfig::from_json(&v);
        assert_eq!(cfg.priority, RouterConfig::default().priority);
        assert_eq!(cfg.claude_model, RouterConfig::default().claude_model);
    }
}
