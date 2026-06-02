//! `ruflo init` command: initialize a new Ruflo project.

use tracing::info;

/// Initialize a new Ruflo project with the given name.
pub async fn init(name: Option<String>) -> anyhow::Result<()> {
    let project_name = name.unwrap_or_else(|| "ruflo-project".to_string());
    info!("Creating new project: {}", project_name);

    // TODO: Create project structure:
    // - .ruflo/ directory
    // - Default config (TOML)
    // - Plugin registry path
    // - Session storage path

    Ok(())
}
