//! gov_issues domain (ADR-028): thin MCP wrappers over the `br` CLI (beads_rust).
//!
//! All tools shell out to `br --db <ruvos-data-root>/issues.db --json <subcommand>`.
//! If `br` is not in PATH every tool returns a structured install-hint response
//! rather than an error, so the MCP server stays up.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::{paths, Result, RuvosError};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::process::Command;

// ── helpers ──────────────────────────────────────────────────────────────────

fn br_not_found() -> Value {
    json!({
        "status": "error",
        "error": "br_not_found",
        "message": "beads_rust `br` binary not found in PATH. \
                    Install: cargo install beads_rust"
    })
}

/// Run `br --db <path> --json --no-color <args...>` and return parsed JSON.
/// Returns `Ok(br_not_found())` when `br` is absent — not a hard `Err`.
async fn run_br(args: &[&str]) -> Result<Value> {
    let db = paths::issues_db().to_string_lossy().to_string();

    let spawn = Command::new("br")
        .args(["--db", &db, "--json", "--no-color"])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let child = match spawn {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(br_not_found()),
        Err(e) => {
            return Err(RuvosError::HandlerError(format!("br spawn failed: {e}")))
        }
    };

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| RuvosError::HandlerError(format!("br wait failed: {e}")))?;

    // `br --json` writes JSON to stdout on success; stderr on failure.
    let raw = if !output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stdout).into_owned()
    } else {
        String::from_utf8_lossy(&output.stderr).into_owned()
    };

    if raw.trim().is_empty() {
        return if output.status.success() {
            Ok(json!({"status": "ok"}))
        } else {
            Err(RuvosError::HandlerError(format!(
                "br exited {} with no output",
                output.status
            )))
        };
    }

    serde_json::from_str(raw.trim())
        .map_err(|e| RuvosError::HandlerError(format!("br output not JSON ({e}): {raw}")))
}

// ── ruvos_gov_issue_create ────────────────────────────────────────────────────

pub struct GovIssueCreateHandler;

impl ToolHandler for GovIssueCreateHandler {
    fn name(&self) -> &'static str {
        "ruvos_gov_issue_create"
    }
    fn domain(&self) -> &'static str {
        "gov_issues"
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["title"],
            "properties": {
                "title":       {"type": "string"},
                "issue_type":  {"type": "string", "description": "bug | feature | task | chore"},
                "priority":    {"type": "string", "description": "P0–P4 or 0–4"},
                "description": {"type": "string"},
                "assignee":    {"type": "string"}
            }
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("title").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams("title required".into()));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let title = params["title"].as_str().unwrap_or_default().to_string();
            let mut args: Vec<String> = vec!["create".into(), title];
            if let Some(t) = params["issue_type"].as_str() {
                args.extend(["--type".into(), t.into()]);
            }
            if let Some(p) = params["priority"].as_str() {
                args.extend(["--priority".into(), p.into()]);
            }
            if let Some(d) = params["description"].as_str() {
                args.extend(["--description".into(), d.into()]);
            }
            if let Some(a) = params["assignee"].as_str() {
                args.extend(["--assignee".into(), a.into()]);
            }
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            run_br(&arg_refs)
                .await
                .map(|v| json!({"status": "ok", "issue": v}))
        })
    }
}

// ── ruvos_gov_issue_list ──────────────────────────────────────────────────────

pub struct GovIssueListHandler;

impl ToolHandler for GovIssueListHandler {
    fn name(&self) -> &'static str {
        "ruvos_gov_issue_list"
    }
    fn domain(&self) -> &'static str {
        "gov_issues"
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "status":   {"type": "string", "description": "open | closed | in_progress"},
                "priority": {"type": "string", "description": "P0–P4 or 0–4"},
                "limit":    {"type": "integer"}
            }
        })
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let mut args: Vec<String> = vec!["list".into()];
            if let Some(s) = params["status"].as_str() {
                args.extend(["--status".into(), s.into()]);
            }
            if let Some(p) = params["priority"].as_str() {
                args.extend(["--priority".into(), p.into()]);
            }
            if let Some(n) = params["limit"].as_u64() {
                args.extend(["--limit".into(), n.to_string()]);
            }
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            run_br(&arg_refs)
                .await
                .map(|v| json!({"status": "ok", "issues": v}))
        })
    }
}

// ── ruvos_gov_issue_show ──────────────────────────────────────────────────────

pub struct GovIssueShowHandler;

impl ToolHandler for GovIssueShowHandler {
    fn name(&self) -> &'static str {
        "ruvos_gov_issue_show"
    }
    fn domain(&self) -> &'static str {
        "gov_issues"
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["issue_id"],
            "properties": {
                "issue_id": {"type": "string"}
            }
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("issue_id").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams("issue_id required".into()));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let id = params["issue_id"].as_str().unwrap_or_default().to_string();
            run_br(&["show", &id])
                .await
                .map(|v| json!({"status": "ok", "issue": v}))
        })
    }
}

// ── ruvos_gov_issue_close ─────────────────────────────────────────────────────

pub struct GovIssueCloseHandler;

impl ToolHandler for GovIssueCloseHandler {
    fn name(&self) -> &'static str {
        "ruvos_gov_issue_close"
    }
    fn domain(&self) -> &'static str {
        "gov_issues"
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["issue_id"],
            "properties": {
                "issue_id": {"type": "string"},
                "reason":   {"type": "string"}
            }
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("issue_id").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams("issue_id required".into()));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let id = params["issue_id"].as_str().unwrap_or_default().to_string();
            let mut args: Vec<String> = vec!["close".into(), id];
            if let Some(note) = params["reason"].as_str() {
                args.extend(["--note".into(), note.into()]);
            }
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            run_br(&arg_refs)
                .await
                .map(|v| json!({"status": "ok", "result": v}))
        })
    }
}

// ── ruvos_gov_issue_search ────────────────────────────────────────────────────

pub struct GovIssueSearchHandler;

impl ToolHandler for GovIssueSearchHandler {
    fn name(&self) -> &'static str {
        "ruvos_gov_issue_search"
    }
    fn domain(&self) -> &'static str {
        "gov_issues"
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string"}
            }
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("query").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams("query required".into()));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let q = params["query"].as_str().unwrap_or_default().to_string();
            run_br(&["search", &q])
                .await
                .map(|v| json!({"status": "ok", "results": v}))
        })
    }
}

// ── ruvos_gov_issue_dep ───────────────────────────────────────────────────────

pub struct GovIssueDepHandler;

impl ToolHandler for GovIssueDepHandler {
    fn name(&self) -> &'static str {
        "ruvos_gov_issue_dep"
    }
    fn domain(&self) -> &'static str {
        "gov_issues"
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["issue_id", "depends_on"],
            "properties": {
                "issue_id":   {"type": "string", "description": "The dependent issue"},
                "depends_on": {"type": "string", "description": "Issue that must complete first"},
                "dep_type":   {"type": "string", "description": "blocks | related | parent-child (default: blocks)"}
            }
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("issue_id").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams("issue_id required".into()));
        }
        if params.get("depends_on").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams("depends_on required".into()));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let id = params["issue_id"].as_str().unwrap_or_default().to_string();
            let dep = params["depends_on"].as_str().unwrap_or_default().to_string();
            let mut args: Vec<String> = vec!["dep".into(), "add".into(), id, dep];
            if let Some(t) = params["dep_type"].as_str() {
                args.extend(["--type".into(), t.into()]);
            }
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            run_br(&arg_refs)
                .await
                .map(|v| json!({"status": "ok", "result": v}))
        })
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn check(name: &str, domain: &str, handler: &dyn ToolHandler) {
        assert_eq!(handler.name(), name);
        assert_eq!(handler.domain(), domain);
    }

    #[test]
    fn handlers_registered_with_correct_name_and_domain() {
        check("ruvos_gov_issue_create", "gov_issues", &GovIssueCreateHandler);
        check("ruvos_gov_issue_list", "gov_issues", &GovIssueListHandler);
        check("ruvos_gov_issue_show", "gov_issues", &GovIssueShowHandler);
        check("ruvos_gov_issue_close", "gov_issues", &GovIssueCloseHandler);
        check("ruvos_gov_issue_search", "gov_issues", &GovIssueSearchHandler);
        check("ruvos_gov_issue_dep", "gov_issues", &GovIssueDepHandler);
    }

    #[test]
    fn validate_rejects_missing_required_fields() {
        assert!(GovIssueCreateHandler.validate(&json!({})).is_err());
        assert!(GovIssueShowHandler.validate(&json!({})).is_err());
        assert!(GovIssueCloseHandler.validate(&json!({})).is_err());
        assert!(GovIssueSearchHandler.validate(&json!({})).is_err());
        assert!(GovIssueDepHandler.validate(&json!({"issue_id": "bd-1"})).is_err());
    }

    #[test]
    fn validate_accepts_valid_params() {
        assert!(GovIssueCreateHandler.validate(&json!({"title": "foo"})).is_ok());
        assert!(GovIssueListHandler.validate(&json!({})).is_ok());
        assert!(GovIssueShowHandler.validate(&json!({"issue_id": "bd-1"})).is_ok());
        assert!(GovIssueCloseHandler.validate(&json!({"issue_id": "bd-1"})).is_ok());
        assert!(GovIssueSearchHandler.validate(&json!({"query": "login"})).is_ok());
        assert!(GovIssueDepHandler
            .validate(&json!({"issue_id": "bd-1", "depends_on": "bd-2"}))
            .is_ok());
    }

    #[tokio::test]
    async fn list_returns_status_field_even_without_br() {
        // Whether br exists or not, the handler returns {status: ...}.
        let r = GovIssueListHandler.execute(json!({})).await.unwrap();
        assert!(r.get("status").is_some());
    }
}
