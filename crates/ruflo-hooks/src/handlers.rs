use crate::types::*;
use anyhow::Result;
use serde_json::json;

pub struct HookDispatcher;

impl HookDispatcher {
    pub fn new() -> Self {
        HookDispatcher
    }

    pub async fn dispatch_pre(
        &self,
        kind: HookKind,
        payload: serde_json::Value,
    ) -> Result<HookResponse> {
        // Route based on kind
        match kind {
            HookKind::Task => self.handle_pre_task(&payload).await,
            HookKind::Edit => self.handle_pre_edit(&payload).await,
            HookKind::Command => self.handle_pre_command(&payload).await,
            HookKind::Session => self.handle_pre_session(&payload).await,
        }
    }

    pub async fn dispatch_post(
        &self,
        kind: HookKind,
        payload: serde_json::Value,
        outcome: HookOutcome,
    ) -> Result<HookResponse> {
        // Route based on kind
        match kind {
            HookKind::Task => self.handle_post_task(&payload, outcome).await,
            HookKind::Edit => self.handle_post_edit(&payload, outcome).await,
            HookKind::Command => self.handle_post_command(&payload, outcome).await,
            HookKind::Session => self.handle_post_session(&payload, outcome).await,
        }
    }

    async fn handle_pre_task(&self, _payload: &serde_json::Value) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_post_task(
        &self,
        _payload: &serde_json::Value,
        _outcome: HookOutcome,
    ) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_pre_edit(&self, _payload: &serde_json::Value) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_post_edit(
        &self,
        _payload: &serde_json::Value,
        _outcome: HookOutcome,
    ) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_pre_command(&self, _payload: &serde_json::Value) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_post_command(
        &self,
        _payload: &serde_json::Value,
        _outcome: HookOutcome,
    ) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_pre_session(&self, _payload: &serde_json::Value) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }

    async fn handle_post_session(
        &self,
        _payload: &serde_json::Value,
        _outcome: HookOutcome,
    ) -> Result<HookResponse> {
        Ok(HookResponse {
            status: "ok".to_string(),
            routing: None,
            context: json!({}),
        })
    }
}

impl Default for HookDispatcher {
    fn default() -> Self {
        Self::new()
    }
}
