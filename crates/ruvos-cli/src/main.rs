//! rUvOS: the agentic operating system shell.
//!
//! Single static binary entry point. Dispatches to subcommands (init, mcp serve, agent spawn, etc.).

mod cli;

use clap::Parser;
use cli::{
    Commands, ContractsCommand, CveCommand, DaemonCommand, EvalCommand, McpCommand, PluginCommand,
    SkillsCommand, SkillsPackCommand,
};
use tracing::info;

#[derive(Parser)]
#[command(name = "ruvos")]
#[command(about = "The agentic operating system. RuVector is its kernel, rUvOS is its shell.")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Parse `KEY=VALUE` pairs from `.env` file contents.
///
/// Minimal, dependency-free: supports `KEY=VALUE`, an optional `export ` prefix,
/// `#` comments, blank lines, and matching single/double quotes around the
/// value. Returns pairs in file order; application order (and skipping already-set
/// vars) is the caller's job.
fn parse_dotenv(content: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let val = val.trim();
        let val = val
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| val.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(val);
        out.push((key.to_string(), val.to_string()));
    }
    out
}

/// Load `.env` from the current directory into the process environment.
/// Real environment variables always win — `.env` only fills in what's unset —
/// so secrets exported by the shell or the MCP launcher take precedence.
fn load_dotenv() {
    let Ok(content) = std::fs::read_to_string(".env") else {
        return;
    };
    for (key, val) in parse_dotenv(&content) {
        if std::env::var_os(&key).is_none() {
            std::env::set_var(key, val);
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env before anything reads the environment (e.g. OPENROUTER_API_KEY
    // for the LLM router, RUST_LOG for tracing). Safe here: single-threaded
    // startup, before any tokio worker or tracing subscriber exists.
    load_dotenv();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            name,
            dry_run,
            force,
            no_data_dir,
            hooks,
        } => {
            ruvos_cli::commands::init::init(name, dry_run, force, no_data_dir, hooks).await?;
        }
        Commands::Doctor { json, strict } => {
            ruvos_cli::commands::doctor::doctor(json, strict).await?;
        }
        Commands::Skills { command } => match command {
            SkillsCommand::Audit {
                corpus_root,
                db,
                write,
                format,
            } => {
                let report =
                    ruvos_cli::commands::skills::audit(corpus_root, db, Some(write), format)?;
                ruvos_cli::commands::skills::print_audit_summary(&report);
            }
            SkillsCommand::Pack { command } => match command {
                SkillsPackCommand::Build {
                    manifest,
                    selection_manifest,
                    db,
                    output,
                    tier,
                    extra_tier,
                } => {
                    let mut selected_tiers = vec![tier];
                    selected_tiers.extend(extra_tier);
                    selected_tiers.sort();
                    selected_tiers.dedup();
                    let report = ruvos_cli::commands::skills::build_pack(
                        ruvos_cli::commands::skills::PackBuildConfig {
                            manifest_path: manifest,
                            source_db: db,
                            output,
                            selection_manifest: Some(selection_manifest),
                            selected_tiers,
                        },
                    )?;
                    ruvos_cli::commands::skills::print_pack_summary(&report);
                }
                SkillsPackCommand::Install {
                    source,
                    destination,
                } => {
                    let destination =
                        destination.unwrap_or_else(ruvos_mcp::paths::skills_pack_file);
                    let report = ruvos_cli::commands::skills::install_pack(source, destination)?;
                    ruvos_cli::commands::skills::print_install_summary(&report);
                }
            },
        },
        Commands::Eval { command } => match command {
            EvalCommand::Compress { write, compare_to } => {
                ruvos_cli::commands::eval::run_compress(
                    ruvos_cli::commands::eval::CompressEvalCommand { write, compare_to },
                )?;
            }
            EvalCommand::OrchestrateHandoff { write, compare_to } => {
                ruvos_cli::commands::eval::run_orchestrate_handoff(
                    ruvos_cli::commands::eval::OrchestrateHandoffEvalCommand { write, compare_to },
                )?;
            }
            EvalCommand::SwarmRecovery { write, compare_to } => {
                ruvos_cli::commands::eval::run_swarm_recovery(
                    ruvos_cli::commands::eval::SwarmRecoveryEvalCommand { write, compare_to },
                )?;
            }
            EvalCommand::SkillRouting { write, compare_to } => {
                ruvos_cli::commands::eval::run_skill_routing(
                    ruvos_cli::commands::eval::SkillRoutingEvalCommand { write, compare_to },
                )?;
            }
            EvalCommand::SwarmLearning { write, compare_to } => {
                ruvos_cli::commands::eval::run_swarm_learning(
                    ruvos_cli::commands::eval::SwarmLearningEvalCommand { write, compare_to },
                )?;
            }
        },
        Commands::Compress {
            file,
            kind,
            min_bytes,
            keep_head_lines,
            keep_tail_lines,
            max_array_items,
            session_id,
            raw,
        } => {
            ruvos_cli::commands::compress::run(ruvos_cli::commands::compress::CompressCommand {
                file,
                kind,
                min_bytes,
                keep_head_lines,
                keep_tail_lines,
                max_array_items,
                session_id,
                raw,
            })
            .await?;
        }
        Commands::Contracts { command } => match command {
            ContractsCommand::Generate { format, write } => {
                ruvos_cli::commands::contracts::generate(format, write)?;
            }
            ContractsCommand::Check { path } => {
                ruvos_cli::commands::contracts::check(path)?;
            }
        },
        Commands::Mcp { command } => match command {
            McpCommand::Serve => {
                info!("Starting MCP server");
                ruvos_cli::commands::mcp::serve().await?;
            }
        },
        Commands::Hook { kind, phase } => {
            ruvos_cli::commands::hook::run_from_stdin(&kind, &phase).await?;
        }
        Commands::Status { json } => ruvos_cli::commands::status::run(json).await?,
        Commands::Daemon { command } => match command {
            DaemonCommand::Watch { agent_id, poll_ms } => {
                info!("Starting relay daemon (agent_id={agent_id}, poll_ms={poll_ms})");
                let (tx, rx) = tokio::sync::watch::channel(false);
                // Graceful shutdown on Ctrl-C / SIGTERM.
                tokio::spawn(async move {
                    wait_for_shutdown_signal().await;
                    let _ = tx.send(true);
                });
                let cfg = ruvos_mcp::daemon::DaemonConfig {
                    agent_id,
                    poll_interval_ms: poll_ms,
                };
                ruvos_mcp::daemon::run_daemon(cfg, rx).await;
            }
        },
        Commands::Cve { command } => match command {
            CveCommand::Scan {
                path,
                json,
                sarif,
                prod_only,
                offline,
                offline_db,
                min_severity,
                fail_on,
                no_cache,
            } => {
                let cache_path = ruvos_mcp::paths::data_root()
                    .join("cve")
                    .join("osv-cache.json");
                ruvos_cli::commands::cve::run_cve_scan(ruvos_cli::commands::cve::CveScanCommand {
                    path,
                    json,
                    sarif,
                    prod_only,
                    offline,
                    offline_db,
                    min_severity,
                    fail_on,
                    no_cache,
                    cache_path: if no_cache { None } else { Some(cache_path) },
                })
                .await?;
            }
        },
        Commands::Plugin { command } => match command {
            PluginCommand::Install { name, from } => {
                let dest_root = std::path::PathBuf::from("./.ruvos/plugins");
                ruvos_cli::commands::plugin::run_install(&name, &from, &dest_root).await?;
            }
        },
    }

    Ok(())
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate()).expect("failed to listen for SIGTERM");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl-c");
    }
}

#[cfg(test)]
mod tests {
    use super::parse_dotenv;

    #[test]
    fn parses_pairs_comments_quotes_and_export() {
        let env = r#"
# a comment
OPENROUTER_API_KEY=sk-or-abc123
export RUVOS_OPENROUTER_MODEL="anthropic/claude-sonnet-4-6"
QUOTED='single quoted'
  SPACED = value with spaces

NOEQ_LINE_IGNORED
=novalue_key_ignored
"#;
        let pairs = parse_dotenv(env);
        assert_eq!(
            pairs,
            vec![
                ("OPENROUTER_API_KEY".to_string(), "sk-or-abc123".to_string()),
                (
                    "RUVOS_OPENROUTER_MODEL".to_string(),
                    "anthropic/claude-sonnet-4-6".to_string()
                ),
                ("QUOTED".to_string(), "single quoted".to_string()),
                ("SPACED".to_string(), "value with spaces".to_string()),
            ]
        );
    }

    #[test]
    fn empty_input_yields_no_pairs() {
        assert!(parse_dotenv("").is_empty());
        assert!(parse_dotenv("\n# only a comment\n").is_empty());
    }
}
