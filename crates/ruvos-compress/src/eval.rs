use crate::{compress_content, CompressionConfig, ContentKind};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionRegressionCase {
    pub name: String,
    pub kind: ContentKind,
    pub input: String,
    pub config: CompressionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionRegressionCaseResult {
    pub name: String,
    pub kind: ContentKind,
    pub changed: bool,
    pub original_bytes: usize,
    pub compressed_bytes: usize,
    pub bytes_saved: usize,
    pub compression_ratio: f64,
    pub tokens_before: usize,
    pub tokens_after: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionRegressionSummary {
    pub case_count: usize,
    pub changed_cases: usize,
    pub original_bytes: usize,
    pub compressed_bytes: usize,
    pub bytes_saved: usize,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub token_reduction_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionRegressionReport {
    pub suite: String,
    pub cases: Vec<CompressionRegressionCaseResult>,
    pub summary: CompressionRegressionSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionRegressionComparison {
    pub suite_matches: bool,
    pub case_count_matches: bool,
    pub matching_case_names: bool,
    pub baseline: CompressionRegressionReport,
    pub current: CompressionRegressionReport,
    pub summary_delta: CompressionRegressionSummaryDelta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionRegressionSummaryDelta {
    pub original_bytes: isize,
    pub compressed_bytes: isize,
    pub bytes_saved: isize,
    pub tokens_before: isize,
    pub tokens_after: isize,
    pub token_reduction_ratio: f64,
}

fn default_cases() -> Vec<CompressionRegressionCase> {
    let json = serde_json::json!([
        {"id": 1, "status": "ok", "path": "/health"},
        {"id": 2, "status": "ok", "path": "/users"},
        {"id": 3, "status": "failed", "error": "timeout", "path": "/payments"},
        {"id": 4, "status": "ok", "path": "/settings"},
        {"id": 5, "status": "ok", "path": "/profile"},
        {"id": 6, "status": "ok", "path": "/billing"},
        {"id": 7, "status": "ok", "path": "/reports"},
        {"id": 8, "status": "ok", "path": "/logs"},
        {"id": 9, "status": "ok", "path": "/search"},
        {"id": 10, "status": "ok", "path": "/admin"},
        {"id": 11, "status": "ok", "path": "/sessions"},
        {"id": 12, "status": "ok", "path": "/audit"},
        {"id": 13, "status": "ok", "path": "/graph"},
        {"id": 14, "status": "ok", "path": "/tasks"},
        {"id": 15, "status": "ok", "path": "/events"}
    ])
    .to_string();

    let mut log = String::new();
    for i in 0..24 {
        log.push_str(&format!(
            "2026-06-05T12:00:{i:02}Z INFO heartbeat tick {i}\n"
        ));
    }
    log.push_str("2026-06-05T12:01:00Z ERROR request failed\n");
    log.push_str("thread 'worker-1' panicked at src/lib.rs:12:8\n");
    log.push_str("stack backtrace:\n");
    log.push_str("   0: core::panicking::panic_fmt\n");
    log.push_str("   1: worker::run\n");
    log.push_str("Caused by: timeout while contacting upstream\n");
    for i in 0..8 {
        log.push_str(&format!(
            "2026-06-05T12:01:{i:02}Z WARN retry attempt {i}\n"
        ));
    }

    let mut code = String::from("use std::fmt;\n\npub fn outer() {\n");
    for i in 0..40 {
        code.push_str(&format!("    let filler_{i} = {i};\n"));
    }
    code.push_str(
        "}\n\npub trait Formatter {\n    fn format(&self, input: &str) -> String;\n}\n\n",
    );
    code.push_str("pub async fn render() {\n    match true {\n        true => println!(\"hit\"),\n        false => println!(\"miss\"),\n    }\n}\n");

    let mut text = String::new();
    text.push_str("This is a long plaintext payload for regression testing.\n");
    for i in 0..80 {
        text.push_str(&format!(
            "line {i}: the compressor should keep useful context while trimming repetition\n"
        ));
    }

    vec![
        CompressionRegressionCase {
            name: "json".to_string(),
            kind: ContentKind::Json,
            input: json,
            config: CompressionConfig {
                min_bytes: 1,
                keep_head_lines: 2,
                keep_tail_lines: 2,
                max_array_items: 6,
            },
        },
        CompressionRegressionCase {
            name: "log".to_string(),
            kind: ContentKind::Log,
            input: log,
            config: CompressionConfig {
                min_bytes: 1,
                keep_head_lines: 2,
                keep_tail_lines: 2,
                max_array_items: 12,
            },
        },
        CompressionRegressionCase {
            name: "code".to_string(),
            kind: ContentKind::Code,
            input: code,
            config: CompressionConfig {
                min_bytes: 1,
                keep_head_lines: 2,
                keep_tail_lines: 2,
                max_array_items: 12,
            },
        },
        CompressionRegressionCase {
            name: "text".to_string(),
            kind: ContentKind::Text,
            input: text,
            config: CompressionConfig {
                min_bytes: 1,
                keep_head_lines: 3,
                keep_tail_lines: 3,
                max_array_items: 12,
            },
        },
    ]
}

pub fn default_regression_cases() -> Vec<CompressionRegressionCase> {
    default_cases()
}

pub fn run_compression_regression_suite() -> CompressionRegressionReport {
    let cases = default_cases();
    let mut results = Vec::with_capacity(cases.len());
    let mut summary = CompressionRegressionSummary {
        case_count: cases.len(),
        changed_cases: 0,
        original_bytes: 0,
        compressed_bytes: 0,
        bytes_saved: 0,
        tokens_before: 0,
        tokens_after: 0,
        token_reduction_ratio: 1.0,
    };

    for case in cases {
        let result = compress_content(&case.input, Some(case.kind), case.config);
        if result.changed {
            summary.changed_cases += 1;
        }
        summary.original_bytes += result.original_bytes;
        summary.compressed_bytes += result.compressed_bytes;
        summary.bytes_saved += result.bytes_saved;
        summary.tokens_before += result.tokens_before;
        summary.tokens_after += result.tokens_after;
        results.push(CompressionRegressionCaseResult {
            name: case.name,
            kind: result.kind,
            changed: result.changed,
            original_bytes: result.original_bytes,
            compressed_bytes: result.compressed_bytes,
            bytes_saved: result.bytes_saved,
            compression_ratio: result.compression_ratio,
            tokens_before: result.tokens_before,
            tokens_after: result.tokens_after,
        });
    }

    summary.token_reduction_ratio = if summary.tokens_before == 0 {
        1.0
    } else {
        summary.tokens_after as f64 / summary.tokens_before as f64
    };

    CompressionRegressionReport {
        suite: "compress-regression".to_string(),
        cases: results,
        summary,
    }
}

pub fn load_regression_report(
    path: impl AsRef<Path>,
) -> anyhow::Result<CompressionRegressionReport> {
    let rendered = std::fs::read_to_string(path)?;
    let report = serde_json::from_str(&rendered)?;
    Ok(report)
}

pub fn compare_regression_reports(
    current: &CompressionRegressionReport,
    baseline: &CompressionRegressionReport,
) -> CompressionRegressionComparison {
    let matching_case_names = current
        .cases
        .iter()
        .map(|case| &case.name)
        .eq(baseline.cases.iter().map(|case| &case.name));

    CompressionRegressionComparison {
        suite_matches: current.suite == baseline.suite,
        case_count_matches: current.summary.case_count == baseline.summary.case_count,
        matching_case_names,
        summary_delta: CompressionRegressionSummaryDelta {
            original_bytes: current.summary.original_bytes as isize
                - baseline.summary.original_bytes as isize,
            compressed_bytes: current.summary.compressed_bytes as isize
                - baseline.summary.compressed_bytes as isize,
            bytes_saved: current.summary.bytes_saved as isize
                - baseline.summary.bytes_saved as isize,
            tokens_before: current.summary.tokens_before as isize
                - baseline.summary.tokens_before as isize,
            tokens_after: current.summary.tokens_after as isize
                - baseline.summary.tokens_after as isize,
            token_reduction_ratio: current.summary.token_reduction_ratio
                - baseline.summary.token_reduction_ratio,
        },
        baseline: baseline.clone(),
        current: current.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_suite_reports_all_core_cases() {
        let report = run_compression_regression_suite();
        assert_eq!(report.suite, "compress-regression");
        assert_eq!(report.summary.case_count, 4);
        assert!(report.summary.changed_cases >= 3);
        assert!(report.summary.tokens_after <= report.summary.tokens_before);
    }

    #[test]
    fn compare_regression_reports_detects_deltas() {
        let baseline = run_compression_regression_suite();
        let current = baseline.clone();
        let comparison = compare_regression_reports(&current, &baseline);
        assert!(comparison.suite_matches);
        assert!(comparison.case_count_matches);
        assert!(comparison.matching_case_names);
        assert_eq!(comparison.summary_delta.tokens_before, 0);
        assert_eq!(comparison.summary_delta.tokens_after, 0);
    }

    #[test]
    fn load_regression_report_roundtrips_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.json");
        let report = run_compression_regression_suite();
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();

        let loaded = load_regression_report(&path).unwrap();
        assert_eq!(loaded.suite, report.suite);
        assert_eq!(loaded.summary.case_count, report.summary.case_count);
    }
}
