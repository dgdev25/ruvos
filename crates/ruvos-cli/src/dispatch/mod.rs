//! Task dispatch logic for routing across hosts and agents.

/// Dispatch a task to an available host/agent.
pub async fn dispatch(_task: &str) -> anyhow::Result<()> {
    // TODO: Route the task based on:
    // - hooks.pre (pre-task hook)
    // - Model + archetype recommendation
    // - Available hosts (Claude Code, Codex CLI, Gemini CLI)
    // - Agent swarm status

    Ok(())
}
