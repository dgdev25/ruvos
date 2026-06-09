//! Thin async wrapper around the Anthropic Messages API.
//!
//! `call_llm` returns `Ok(None)` when `ANTHROPIC_API_KEY` is not set so
//! callers can fall back to deterministic templates without error.

use crate::{Result, RuvosError};
use serde_json::json;

/// Archetype-specific system prompts — project-agnostic, task-focused.
pub fn archetype_system_prompt(archetype: &str) -> &'static str {
    match archetype {
        "planner" => {
            "You are a technical planner. Given a task description, decompose it into a numbered \
             list of concrete implementation steps. Be specific, sequential, and brief. Output \
             only the plan — no preamble."
        }
        "coder" => {
            "You are an expert software engineer. Given a task, produce clean, working code. \
             Include relevant code blocks with language identifiers and brief inline comments. \
             No lengthy explanations outside of code blocks."
        }
        "tester" => {
            "You are a QA engineer. Given a task or implementation, write a comprehensive set of \
             test cases covering the happy path, edge cases, and failure modes. Use a numbered list."
        }
        "reviewer" => {
            "You are a senior code reviewer. Given an implementation or plan, identify correctness \
             issues, security concerns, and style improvements. Be specific and constructive."
        }
        "researcher" => {
            "You are a technical researcher. Given a topic or problem, identify the key questions \
             to answer, relevant sources to check, and open unknowns. Output a structured \
             investigation plan."
        }
        "architect" => {
            "You are a software architect. Given a task, define component boundaries, interfaces, \
             and data flow. Focus on modularity, coupling, and trade-offs."
        }
        "security" => {
            "You are a security engineer. Given a task or code, build a threat model, identify \
             attack surfaces, and list specific vulnerabilities to check. Be concrete."
        }
        "perf" => {
            "You are a performance engineer. Given a task or code, identify hotspots to profile, \
             algorithmic improvements, and specific optimizations to try."
        }
        "devops" => {
            "You are a DevOps engineer. Given a task, outline the CI/CD pipeline steps, \
             deployment plan, and operational considerations."
        }
        "data" => {
            "You are a data engineer. Given a task, define the schema, migrations, and queries \
             needed. Be precise about types and indexes."
        }
        "docs" => {
            "You are a technical writer. Given a task or code, identify the sections to document \
             and write clear, example-driven documentation."
        }
        "coordinator" => {
            "You are a project coordinator. Given a task, identify the sub-agents needed, their \
             responsibilities, and the execution order."
        }
        _ => "You are a helpful technical expert. Complete the task described below.",
    }
}

/// Call the Anthropic Messages API with the given system prompt and user message.
///
/// Returns `Ok(None)` when `ANTHROPIC_API_KEY` is absent — the caller should
/// fall back to a deterministic template. Returns `Ok(Some(text))` on success.
pub async fn call_llm(
    system_prompt: &str,
    user_message: &str,
    model: &str,
) -> Result<Option<String>> {
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) if !key.is_empty() => key,
        _ => return Ok(None),
    };

    let client = reqwest::Client::new();
    let body = json!({
        "model": model,
        "max_tokens": 1024,
        "system": system_prompt,
        "messages": [{ "role": "user", "content": user_message }]
    });

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| RuvosError::InternalError(format!("llm request: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(RuvosError::InternalError(format!(
            "llm api {status}: {text}"
        )));
    }

    let payload: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| RuvosError::InternalError(format!("llm response parse: {e}")))?;

    let text = payload["content"]
        .as_array()
        .and_then(|blocks| blocks.first())
        .and_then(|block| block["text"].as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            RuvosError::InternalError("llm response missing content block".to_string())
        })?;

    Ok(Some(text))
}
