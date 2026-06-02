use crate::error::{PluginError, Result};
use crate::types::PluginManifest;
use std::path::Path;

pub fn parse_manifest(content: &str) -> Result<PluginManifest> {
    toml::from_str(content).map_err(|e| PluginError::ManifestParse(e.to_string()))
}

pub fn read_manifest_from_file(path: &Path) -> Result<PluginManifest> {
    let content = std::fs::read_to_string(path).map_err(|e| PluginError::Io(e))?;
    parse_manifest(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let content = r#"
[plugin]
name = "test"
version = "1.0.0"
description = "test"
license = "MIT"
authors = []

[capabilities]
"#;
        let result = parse_manifest(content);
        assert!(result.is_ok());
    }
}
