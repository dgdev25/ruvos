use anyhow::Result;
use ruvos_cve_lite::{
    output, remediation,
    scanner::{scan, ScanOptions},
    types::Severity,
};
use std::path::PathBuf;

pub struct CveScanCommand {
    pub path: PathBuf,
    pub json: bool,
    pub sarif: bool,
    pub prod_only: bool,
    pub offline: bool,
    pub offline_db: Option<PathBuf>,
    pub min_severity: Option<String>,
    pub fail_on: Option<String>,
    pub no_cache: bool,
    pub cache_path: Option<PathBuf>,
}

pub async fn run_cve_scan(cmd: CveScanCommand) -> Result<()> {
    let min_severity = parse_optional_severity(cmd.min_severity.as_deref(), "--min-severity")?;
    let fail_threshold = parse_optional_severity(cmd.fail_on.as_deref(), "--fail-on")?;

    let opts = ScanOptions {
        offline: cmd.offline,
        offline_db: cmd.offline_db,
        prod_only: cmd.prod_only,
        min_severity,
        no_cache: cmd.no_cache,
        cache_path: cmd.cache_path,
        ..ScanOptions::default()
    };

    let result = scan(&cmd.path, &opts).await?;
    let fixes = remediation::suggest_fixes(&result.findings, &result.scan_input.source);

    if cmd.sarif {
        println!("{}", output::to_sarif(&result));
    } else if cmd.json {
        println!("{}", output::to_json(&result));
    } else {
        print!("{}", output::to_terminal(&result, &fixes));
    }

    if let Some(threshold) = fail_threshold {
        if severity_order(&result.highest_severity) >= severity_order(&threshold) {
            std::process::exit(1);
        }
    }

    Ok(())
}

fn parse_optional_severity(value: Option<&str>, flag_name: &str) -> Result<Option<Severity>> {
    value.map(|s| parse_severity(s, flag_name)).transpose()
}

fn parse_severity(s: &str, flag_name: &str) -> Result<Severity> {
    match s.to_lowercase().as_str() {
        "critical" => Ok(Severity::Critical),
        "high" => Ok(Severity::High),
        "medium" => Ok(Severity::Medium),
        "low" => Ok(Severity::Low),
        _ => Err(anyhow::anyhow!(
            "invalid value for {flag_name}: {s} (expected low, medium, high, or critical)"
        )),
    }
}

fn severity_order(sev: &Severity) -> u8 {
    match sev {
        Severity::Critical => 4,
        Severity::High => 3,
        Severity::Medium => 2,
        Severity::Low => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_severity_accepts_valid_values() {
        assert_eq!(
            parse_severity("high", "--fail-on").unwrap(),
            Severity::High
        );
    }

    #[test]
    fn parse_severity_rejects_invalid_values() {
        assert!(parse_severity("oops", "--fail-on").is_err());
    }
}
