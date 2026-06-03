//! Post-hook implementation: task|edit|command|session.

use crate::hooks::HookPayload;

/// Execute post-hook logic based on kind.
pub async fn post_hook(payload: HookPayload) -> anyhow::Result<()> {
    match payload.kind {
        crate::hooks::HookKind::Task => {
            // TODO: After task completion (success/fail) — feed outcome to SONA learning
        }
        crate::hooks::HookKind::Edit => {
            // TODO: After file write — capture codemod tier + learning signal
        }
        crate::hooks::HookKind::Command => {
            // TODO: After shell exec — capture outcome (exit code, stdout, stderr)
        }
        crate::hooks::HookKind::Session => {
            // TODO: On session end — persist .rvf snapshot, consolidate memory
        }
    }

    Ok(())
}
