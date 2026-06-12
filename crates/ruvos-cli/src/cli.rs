//! Clap command-line surface for the `ruvos` binary.
//!
//! Pure declaration: the `Commands` enum and its sub-enums. Dispatch lives in
//! `main.rs`. Extracted so `main.rs` stays under the 500-line cap.

use clap::Subcommand;
use compress::defaults::{KEEP_HEAD_LINES, KEEP_TAIL_LINES, MAX_ARRAY_ITEMS, MIN_BYTES};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum Commands {
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
        /// Also write .claude/settings.json hook bindings (PreToolUse/PostToolUse/
        /// SessionStart/Stop -> ruvos hook) so hooks fire mechanically
        #[arg(long)]
        hooks: bool,
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
    /// Dispatch a hook event (called by Claude Code hook bindings; reads JSON from stdin)
    Hook {
        /// Hook kind: task|edit|command|session
        kind: String,
        /// Hook phase
        #[arg(long, default_value = "pre")]
        phase: String,
    },
    /// Show live system state: health, swarm, agents, events, relays (read-only)
    Status {
        #[arg(long, help = "Emit raw JSON instead of the human view")]
        json: bool,
    },
    /// Relay daemon — persistent bus listener for the agent execution bridge.
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// Plugin management
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
}

#[derive(Subcommand)]
pub enum McpCommand {
    /// Serve the MCP server
    Serve,
}

#[derive(Subcommand)]
pub enum PluginCommand {
    /// Fetch, verify (sha256 + optional HMAC), and install a plugin tarball
    Install {
        /// Plugin name (directory name under ./.ruvos/plugins/)
        name: String,
        /// Tarball source: local path or https URL (expects .sha256 sidecar)
        #[arg(long)]
        from: String,
    },
}

#[derive(Subcommand)]
pub enum DaemonCommand {
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
pub enum CveCommand {
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
pub enum ContractsCommand {
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
pub enum SkillsCommand {
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
pub enum SkillsPackCommand {
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
pub enum EvalCommand {
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
