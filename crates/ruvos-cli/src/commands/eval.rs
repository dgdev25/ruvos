//! Evaluation and regression commands.

use serde::Serialize;
use std::path::PathBuf;

// ── compress ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CompressEvalCommand {
    pub write: Option<PathBuf>,
    pub compare_to: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct CompressEvalOutput {
    report: compress::CompressionRegressionReport,
    comparison: Option<compress::CompressionRegressionComparison>,
}

pub fn run_compress(command: CompressEvalCommand) -> anyhow::Result<()> {
    let report = compress::run_compression_regression_suite();
    let comparison = if let Some(path) = command.compare_to.as_ref() {
        let baseline = compress::load_regression_report(path)?;
        Some(compress::compare_regression_reports(&report, &baseline))
    } else {
        None
    };
    let output = CompressEvalOutput { report, comparison };
    if let Some(path) = command.write {
        std::fs::write(
            path,
            serde_json::to_string_pretty(&output.report)?.as_bytes(),
        )?;
    }
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// ── orchestrate-handoff ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OrchestrateHandoffEvalCommand {
    pub write: Option<PathBuf>,
    pub compare_to: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct OrchestrateHandoffOutput {
    report: ruvos_mcp::eval::orchestrate::HandoffReport,
    comparison: Option<ruvos_mcp::eval::orchestrate::HandoffComparison>,
}

pub fn run_orchestrate_handoff(command: OrchestrateHandoffEvalCommand) -> anyhow::Result<()> {
    let report = ruvos_mcp::eval::orchestrate::run_orchestrate_handoff_suite();
    let comparison = if let Some(path) = command.compare_to.as_ref() {
        let baseline = ruvos_mcp::eval::orchestrate::load_handoff_report(path)?;
        Some(ruvos_mcp::eval::orchestrate::compare_handoff_reports(
            &report, &baseline,
        ))
    } else {
        None
    };
    let output = OrchestrateHandoffOutput { report, comparison };
    if let Some(path) = command.write {
        std::fs::write(
            path,
            serde_json::to_string_pretty(&output.report)?.as_bytes(),
        )?;
    }
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// ── swarm-recovery ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SwarmRecoveryEvalCommand {
    pub write: Option<PathBuf>,
    pub compare_to: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct SwarmRecoveryOutput {
    report: ruvos_mcp::eval::swarm_recovery::SwarmRecoveryReport,
    comparison: Option<ruvos_mcp::eval::swarm_recovery::SwarmRecoveryComparison>,
}

pub fn run_swarm_recovery(command: SwarmRecoveryEvalCommand) -> anyhow::Result<()> {
    let report = ruvos_mcp::eval::swarm_recovery::run_swarm_recovery_suite();
    let comparison = if let Some(path) = command.compare_to.as_ref() {
        let baseline = ruvos_mcp::eval::swarm_recovery::load_swarm_recovery_report(path)?;
        Some(ruvos_mcp::eval::swarm_recovery::compare_swarm_recovery_reports(&report, &baseline))
    } else {
        None
    };
    let output = SwarmRecoveryOutput { report, comparison };
    if let Some(path) = command.write {
        std::fs::write(
            path,
            serde_json::to_string_pretty(&output.report)?.as_bytes(),
        )?;
    }
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// ── skill-routing ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SkillRoutingEvalCommand {
    pub write: Option<PathBuf>,
    pub compare_to: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct SkillRoutingOutput {
    report: ruvos_mcp::eval::skill_routing::SkillRoutingReport,
    comparison: Option<ruvos_mcp::eval::skill_routing::SkillRoutingComparison>,
}

pub fn run_skill_routing(command: SkillRoutingEvalCommand) -> anyhow::Result<()> {
    let report = ruvos_mcp::eval::skill_routing::run_skill_routing_suite();
    let comparison = if let Some(path) = command.compare_to.as_ref() {
        let baseline = ruvos_mcp::eval::skill_routing::load_skill_routing_report(path)?;
        Some(ruvos_mcp::eval::skill_routing::compare_skill_routing_reports(&report, &baseline))
    } else {
        None
    };
    let output = SkillRoutingOutput { report, comparison };
    if let Some(path) = command.write {
        std::fs::write(
            path,
            serde_json::to_string_pretty(&output.report)?.as_bytes(),
        )?;
    }
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// ── swarm-learning ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SwarmLearningEvalCommand {
    pub write: Option<PathBuf>,
    pub compare_to: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct SwarmLearningOutput {
    report: ruvos_mcp::eval::swarm_learning::SwarmLearningReport,
    comparison: Option<ruvos_mcp::eval::swarm_learning::SwarmLearningComparison>,
}

pub fn run_swarm_learning(command: SwarmLearningEvalCommand) -> anyhow::Result<()> {
    let report = ruvos_mcp::eval::swarm_learning::run_swarm_learning_suite();
    let comparison = if let Some(path) = command.compare_to.as_ref() {
        let baseline = ruvos_mcp::eval::swarm_learning::load_swarm_learning_report(path)?;
        Some(ruvos_mcp::eval::swarm_learning::compare_swarm_learning_reports(&report, &baseline))
    } else {
        None
    };
    let output = SwarmLearningOutput { report, comparison };
    if let Some(path) = command.write {
        std::fs::write(
            path,
            serde_json::to_string_pretty(&output.report)?.as_bytes(),
        )?;
    }
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
