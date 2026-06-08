use super::handler::{ExecuteFuture, ToolHandler};
use crate::{paths, Result, RuvosError};
use ruvos_cve_lite::{
    output, remediation,
    scanner::{scan, ScanOptions},
    types::Severity,
};
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct GovCveLookupHandler;

impl ToolHandler for GovCveLookupHandler {
    fn name(&self) -> &'static str {
        "cve_lookup"
    }

    fn domain(&self) -> &'static str {
        "gov"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        if params
            .get("project_path")
            .and_then(|v| v.as_str())
            .is_none()
        {
            return Err(RuvosError::InvalidParams(
                "missing 'project_path' field (string)".to_string(),
            ));
        }
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let project_path = params["project_path"].as_str().unwrap_or(".");
            let prod_only = params
                .get("prod_only")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let offline = params
                .get("offline")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let min_severity = params.get("min_severity").and_then(|v| v.as_str());
            let format = params
                .get("format")
                .and_then(|v| v.as_str())
                .unwrap_or("json");

            let path = PathBuf::from(project_path);
            if !path.exists() {
                return Err(RuvosError::InvalidParams(format!(
                    "project_path does not exist: {project_path}"
                )));
            }

            let cache_path = paths::data_root().join("cve").join("osv-cache.json");
            let opts = ScanOptions {
                offline,
                prod_only,
                cache_path: Some(cache_path),
                min_severity: min_severity.and_then(parse_severity),
                ..ScanOptions::default()
            };

            let result = scan(&path, &opts)
                .await
                .map_err(|e| RuvosError::InternalError(format!("cve scan failed: {e}")))?;

            let fixes = remediation::suggest_fixes(&result.findings, &result.scan_input.source);

            let output_str = match format {
                "sarif" => output::to_sarif(&result),
                "terminal" => output::to_terminal(&result, &fixes),
                _ => output::to_json(&result),
            };

            Ok(json!({
                "status": if result.has_vulnerabilities { "vulnerable" } else { "clean" },
                "total_packages": result.total_packages_scanned,
                "finding_count": result.findings.len(),
                "highest_severity": format!("{:?}", result.highest_severity).to_lowercase(),
                "fix_count": fixes.len(),
                "output": output_str
            }))
        })
    }
}

fn parse_severity(s: &str) -> Option<Severity> {
    match s.to_lowercase().as_str() {
        "critical" => Some(Severity::Critical),
        "high" => Some(Severity::High),
        "medium" => Some(Severity::Medium),
        "low" => Some(Severity::Low),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn handler_names() {
        let h = GovCveLookupHandler;
        assert_eq!(h.name(), "cve_lookup");
        assert_eq!(h.domain(), "gov");
    }

    #[test]
    fn validate_rejects_missing_project_path() {
        let h = GovCveLookupHandler;
        assert!(h.validate(&json!({})).is_err());
    }

    #[test]
    fn validate_accepts_valid_params() {
        let h = GovCveLookupHandler;
        assert!(h.validate(&json!({ "project_path": "/tmp" })).is_ok());
    }
}
