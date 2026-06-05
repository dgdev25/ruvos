//! Evaluation and regression commands.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct CompressEvalCommand {
    pub write: Option<PathBuf>,
    pub compare_to: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct EvalOutput {
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
    let output = EvalOutput { report, comparison };
    if let Some(path) = command.write {
        std::fs::write(
            path,
            serde_json::to_string_pretty(&output.report)?.as_bytes(),
        )?;
    }
    let rendered = serde_json::to_string_pretty(&output)?;
    println!("{rendered}");
    Ok(())
}
