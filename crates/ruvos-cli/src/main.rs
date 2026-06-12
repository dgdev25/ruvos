//! rUvOS: the agentic operating system shell.
//!
//! Single static binary entry point. Dispatches to subcommands (init, mcp serve, agent spawn, etc.).

use clap::{Parser, Subcommand};
use compress::defaults::{KEEP_HEAD_LINES, KEEP_TAIL_LINES, MAX_ARRAY_ITEMS, MIN_BYTES};
use std::path::PathBuf;
use tracing::info;

#[derive(Parser)]
#[command(name = "ruvos")]
#[command(about = "The agentic operating system. RuVector is its kernel, rUvOS is its shell.")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a project: create/update CLAUDE.md with ruvos instructions
    Init {
        /// Project name (defaults to current directory name)
        #[arg(short, long)]
        name: Option<String>,
        /// Print what would change without writing anything
        #[arg(long)]
        dry_run: bool,
        /// Overwrite the managed block even if already up to date
        #[arg(long)]
        force: bool,
        /// Skip creating the .ruvos/ data directory
        #[arg(long)]
        no_data_dir: bool,
    },
    /// Run a local health/invariant check.
    Doctor {
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
        /// Exit non-zero if any invariant is violated.
        #[arg(long)]
        strict: bool,
    },
    /// Audit the source skills corpus and emit a manifest.
    Skills {
        #[command(subcommand)]
        command: SkillsCommand,
    },
    /// Run evaluation and regression reports.
    Eval {
        #[command(subcommand)]
        command: EvalCommand,
    },
    /// Compress content from stdin or a file for the frozen baseline path used by Claude Code, Codex CLI, and Gemini CLI.
    Compress {
        /// Input file. If omitted, stdin is read.
        #[arg(short, long)]
        file: Option<PathBuf>,
        /// Force a content kind; otherwise auto-detect.
        #[arg(long, value_parser = ["auto", "json", "code", "log", "text"])]
        kind: Option<String>,
        /// Minimum input size before compression runs.
        #[arg(long, default_value_t = MIN_BYTES)]
        min_bytes: usize,
        /// Number of lines to preserve from the start of text/log content.
        #[arg(long, default_value_t = KEEP_HEAD_LINES)]
        keep_head_lines: usize,
        /// Number of lines to preserve from the end of text/log content.
        #[arg(long, default_value_t = KEEP_TAIL_LINES)]
        keep_tail_lines: usize,
        /// Maximum items to keep when compressing JSON arrays.
        #[arg(long, default_value_t = MAX_ARRAY_ITEMS)]
        max_array_items: usize,
        /// Optional session id. If set, originals are persisted into the .rvf session.
        #[arg(long)]
        session_id: Option<String>,
        /// Print only the compressed payload.
        #[arg(long)]
        raw: bool,
    },
    /// Generate or verify the canonical contract manifest.
    Contracts {
        #[command(subcommand)]
        command: ContractsCommand,
    },
    /// Start the MCP server on stdio
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    /// Scan a project directory for vulnerable dependencies (CVE/OSV).
    Cve {
        #[command(subcommand)]
        command: CveCommand,
    },
    /// Relay daemon — persistent bus listener for the agent execution bridge.
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
}

#[derive(Subcommand)]
enum McpCommand {
    /// Serve the MCP server
    Serve,
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Start the relay inbox listener (runs until SIGINT/SIGTERM).
    Watch {
        /// Relay agent_id to listen on (default: ruvos-daemon).
        #[arg(long, default_value = "ruvos-daemon")]
        agent_id: String,
        /// Inbox poll interval in milliseconds.
        #[arg(long, default_value_t = 500)]
        poll_ms: u64,
    },
}

#[derive(Subcommand)]
enum CveCommand {
    /// Scan a project directory for vulnerable dependencies.
    Scan {
        /// Project directory containing a lockfile.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output raw JSON.
        #[arg(long)]
        json: bool,
        /// Output SARIF 2.1.0 (GitHub Code Scanning).
        #[arg(long)]
        sarif: bool,
        /// Only scan production dependencies.
        #[arg(long)]
        prod_only: bool,
        /// Use offline advisory DB instead of OSV API.
        #[arg(long)]
        offline: bool,
        /// Path to offline advisory DB (SQLite).
        #[arg(long)]
        offline_db: Option<PathBuf>,
        /// Only report findings at or above this severity (low/medium/high/critical).
        #[arg(long)]
        min_severity: Option<String>,
        /// Exit non-zero if any finding meets this severity threshold.
        #[arg(long)]
        fail_on: Option<String>,
        /// Skip reading and writing the OSV query cache.
        #[arg(long)]
        no_cache: bool,
    },
}

#[derive(Subcommand)]
enum ContractsCommand {
    /// Emit the canonical contract manifest.
    Generate {
        /// Output format.
        #[arg(long, value_enum, default_value_t = ruvos_cli::commands::contracts::ContractFormat::Json)]
        format: ruvos_cli::commands::contracts::ContractFormat,
        /// Write the manifest to a file instead of stdout.
        #[arg(long)]
        write: Option<PathBuf>,
    },
    /// Verify a manifest file matches the live registry.
    Check {
        /// Manifest path to verify.
        #[arg(
            value_name = "PATH",
            default_value = "docs/contracts/contract-manifest.json"
        )]
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum SkillsCommand {
    /// Audit the source corpus and write a deterministic manifest.
    Audit {
        /// Path to the source corpus root.
        #[arg(long, env = "RUVOS_SKILLBASE_ROOT")]
        corpus_root: PathBuf,
        /// Path to the source SQLite corpus database (typically <corpus-root>/data/skills.db).
        #[arg(long, env = "RUVOS_SKILLBASE_DB")]
        db: PathBuf,
        /// Output path for the manifest.
        #[arg(long, default_value = "generated/skills-audit.json")]
        write: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = ruvos_cli::commands::skills::SkillsAuditFormat::Json)]
        format: ruvos_cli::commands::skills::SkillsAuditFormat,
    },
    /// Build a portable skills pack from an audit manifest.
    Pack {
        #[command(subcommand)]
        command: SkillsPackCommand,
    },
}

#[derive(Subcommand)]
enum SkillsPackCommand {
    /// Build `skills.redb` from an audit manifest.
    Build {
        /// Path to the audit manifest.
        #[arg(long, default_value = "generated/skills-audit.json")]
        manifest: PathBuf,
        /// Path to the curated skill selection manifest.
        #[arg(long, default_value = "docs/skills/selected-300-ruvos.json")]
        selection_manifest: PathBuf,
        /// Path to the source SQLite corpus database.
        #[arg(long, env = "RUVOS_SKILLBASE_DB")]
        db: PathBuf,
        /// Output path for the redb pack.
        #[arg(long, default_value = "generated/skills.redb")]
        output: PathBuf,
        /// Selected tiers to include in the default pack.
        #[arg(long, value_enum, default_value_t = ruvos_cli::commands::skills::SkillsPackTier::Core)]
        tier: ruvos_cli::commands::skills::SkillsPackTier,
        /// Include additional tiers in the pack output.
        #[arg(long, value_enum)]
        extra_tier: Vec<ruvos_cli::commands::skills::SkillsPackTier>,
    },
    /// Install a bundled `skills.redb` into the runtime data directory.
    Install {
        /// Source path for the bundled pack.
        #[arg(long, default_value = "docs/skills/public/skills.redb")]
        source: PathBuf,
        /// Destination path for the runtime pack.
        #[arg(long)]
        destination: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum EvalCommand {
    /// Run the compression regression suite and print a JSON report.
    Compress {
        /// Write the JSON report to a file instead of only stdout.
        #[arg(long)]
        write: Option<PathBuf>,
        /// Compare the current run against a saved baseline report.
        #[arg(long)]
        compare_to: Option<PathBuf>,
    },
    /// Verify GOAP pipeline generation and conditional-edge graph routing.
    OrchestrateHandoff {
        #[arg(long)]
        write: Option<PathBuf>,
        #[arg(long)]
        compare_to: Option<PathBuf>,
    },
    /// Verify swarm stale detection, policy updates, and topology convergence.
    SwarmRecovery {
        #[arg(long)]
        write: Option<PathBuf>,
        #[arg(long)]
        compare_to: Option<PathBuf>,
    },
    /// Verify skill query construction and bundle selection per archetype.
    SkillRouting {
        #[arg(long)]
        write: Option<PathBuf>,
        #[arg(long)]
        compare_to: Option<PathBuf>,
    },
    /// Verify the swarm learning loop converges on the correct topology.
    SwarmLearning {
        #[arg(long)]
        write: Option<PathBuf>,
        #[arg(long)]
        compare_to: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
        } => {
            ruvos_cli::commands::init::init(name, dry_run, force, no_data_dir).await?;
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
