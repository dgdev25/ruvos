use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("plugin not found: {0}")]
    NotFound(String),

    #[error("manifest parse error: {0}")]
    ManifestParse(String),

    #[error("markdown parse error: {0}")]
    MarkdownParse(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("command execution failed: {0}")]
    ExecutionFailed(String),

    #[error("invalid plugin directory: {0}")]
    InvalidDirectory(String),
}

pub type Result<T> = std::result::Result<T, PluginError>;
