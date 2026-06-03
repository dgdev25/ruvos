//! Integration tests for CliHost adapters.

use ruvos_host::{
    host::{AgentEvent, AgentRequest, CliError, CliHost, ToolCall},
    ClaudeHost, CodexHost,
};
use serde_json::json;

#[tokio::test]
async fn test_claude_host_available_models() {
    let host = ClaudeHost::new();
    let models = host.available_models().await.unwrap();

    assert!(!models.is_empty());
    assert_eq!(host.name(), "claude");
    assert!(models.iter().any(|m| m.name == "claude-opus"));
    assert!(models.iter().any(|m| m.name == "claude-sonnet"));
    assert!(models.iter().any(|m| m.name == "claude-haiku"));
}

#[tokio::test]
async fn test_codex_host_available_models() {
    let host = CodexHost::new();
    let models = host.available_models().await.unwrap();

    assert!(!models.is_empty());
    assert_eq!(host.name(), "codex");
    assert!(models.iter().any(|m| m.name == "codex-mini"));
}

#[tokio::test]
async fn test_claude_host_run() {
    let host = ClaudeHost::new();
    let request = AgentRequest {
        archetype: "coder".to_string(),
        prompt: "Write a function".to_string(),
        traits: vec!["backend".to_string()],
        model: "claude-opus".to_string(),
        budget: 4000,
    };

    let result = host.run(request).await.unwrap();
    assert!(!result.is_empty());
    assert!(result.contains("successfully"));
}

#[tokio::test]
async fn test_codex_host_run() {
    let host = CodexHost::new();
    let request = AgentRequest {
        archetype: "coder".to_string(),
        prompt: "Write a function".to_string(),
        traits: vec!["frontend".to_string()],
        model: "codex-mini".to_string(),
        budget: 2000,
    };

    let result = host.run(request).await.unwrap();
    assert!(!result.is_empty());
    assert!(result.contains("successfully"));
}

#[tokio::test]
async fn test_claude_host_stream() {
    let host = ClaudeHost::new();
    let request = AgentRequest {
        archetype: "reviewer".to_string(),
        prompt: "Review this code".to_string(),
        traits: vec!["audit".to_string()],
        model: "claude-sonnet".to_string(),
        budget: 3000,
    };

    let events = host.stream(request).await.unwrap();
    assert!(!events.is_empty());

    // Verify event types
    let has_started = events
        .iter()
        .any(|e| matches!(e, AgentEvent::Started { .. }));
    let has_output = events
        .iter()
        .any(|e| matches!(e, AgentEvent::Output { .. }));
    let has_completed = events
        .iter()
        .any(|e| matches!(e, AgentEvent::Completed { .. }));

    assert!(has_started);
    assert!(has_output);
    assert!(has_completed);
}

#[tokio::test]
async fn test_codex_host_stream() {
    let host = CodexHost::new();
    let request = AgentRequest {
        archetype: "tester".to_string(),
        prompt: "Write tests".to_string(),
        traits: vec!["test-driven-development".to_string()],
        model: "codex-mini".to_string(),
        budget: 1500,
    };

    let events = host.stream(request).await.unwrap();
    assert!(!events.is_empty());

    // Verify event types
    let has_started = events
        .iter()
        .any(|e| matches!(e, AgentEvent::Started { .. }));
    let has_completed = events
        .iter()
        .any(|e| matches!(e, AgentEvent::Completed { .. }));

    assert!(has_started);
    assert!(has_completed);
}

#[tokio::test]
async fn test_claude_host_tool_call_round_trip() {
    let host = ClaudeHost::new();

    let tool_call = ToolCall {
        id: "tc-123".to_string(),
        method: "memory.search".to_string(),
        params: json!({ "query": "test query" }),
    };

    host.send_tool_call(tool_call).await.unwrap();

    let response = host.receive_response().await.unwrap();
    assert!(!response.id.is_empty());
    assert!(response.error.is_none());
    assert!(response.result.is_some());
}

#[tokio::test]
async fn test_codex_host_tool_call_round_trip() {
    let host = CodexHost::new();

    let tool_call = ToolCall {
        id: "tc-456".to_string(),
        method: "agent.spawn".to_string(),
        params: json!({ "archetype": "coder" }),
    };

    host.send_tool_call(tool_call).await.unwrap();

    let response = host.receive_response().await.unwrap();
    assert!(!response.id.is_empty());
    assert!(response.error.is_none());
    assert!(response.result.is_some());
}

#[tokio::test]
async fn test_claude_host_error_reporting() {
    let host = ClaudeHost::new();

    let error = CliError {
        code: 400,
        message: "Invalid request".to_string(),
        data: Some(json!({ "field": "archetype" })),
    };

    let result = host.report_error(error).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_codex_host_error_reporting() {
    let host = CodexHost::new();

    let error = CliError {
        code: 500,
        message: "Internal server error".to_string(),
        data: None,
    };

    let result = host.report_error(error).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_adapter_trait_impl_verified() {
    // Verify both adapters properly implement CliHost
    let _claude: Box<dyn CliHost> = Box::new(ClaudeHost::new());
    let _codex: Box<dyn CliHost> = Box::new(CodexHost::new());

    // If this compiles and runs, the trait impl is correct
}

#[tokio::test]
async fn test_claude_stream_with_multiple_traits() {
    let host = ClaudeHost::new();
    let request = AgentRequest {
        archetype: "architect".to_string(),
        prompt: "Design a system".to_string(),
        traits: vec!["backend".to_string(), "cloud".to_string(), "db".to_string()],
        model: "claude-opus".to_string(),
        budget: 8000,
    };

    let events = host.stream(request).await.unwrap();

    // Verify all traits were processed
    let trait_events: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Output { text } if text.contains("Applying trait") => Some(text.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(trait_events.len(), 3);
}

#[tokio::test]
async fn test_model_spec_factory() {
    use ruvos_host::host::ModelSpec;

    let model = ModelSpec::new("test-model", 2, 100000);
    assert_eq!(model.name, "test-model");
    assert_eq!(model.tier, 2);
    assert_eq!(model.context_window, 100000);
}
