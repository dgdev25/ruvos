//! Markdown + YAML frontmatter parser for agents, skills, and commands.

use crate::error::{PluginError, Result};
use std::collections::HashMap;

/// Metadata extracted from YAML frontmatter.
#[derive(Debug, Clone)]
pub struct FrontmatterMetadata {
    pub name: String,
    pub description: String,
    pub metadata: HashMap<String, serde_yaml::Value>,
}

/// Parse YAML frontmatter from markdown content.
///
/// Expected format:
/// ```text
/// ---
/// name: "Agent Name"
/// description: "Agent description"
/// key: value
/// ---
/// # Markdown content follows
/// ```
pub fn parse_frontmatter(content: &str) -> Result<FrontmatterMetadata> {
    let trimmed = content.trim();

    // Check if content starts with ---
    if !trimmed.starts_with("---") {
        return Err(PluginError::MarkdownParse(
            "frontmatter must start with ---".to_string(),
        ));
    }

    // Find the closing ---
    let rest = &trimmed[3..];
    let closing_marker = "---";

    let Some(closing_pos) = rest.find(closing_marker) else {
        return Err(PluginError::MarkdownParse(
            "frontmatter must be closed with ---".to_string(),
        ));
    };

    let frontmatter_str = &rest[..closing_pos].trim();

    // Parse YAML
    let parsed: serde_yaml::Value = serde_yaml::from_str(frontmatter_str)
        .map_err(|e| PluginError::MarkdownParse(format!("invalid YAML: {}", e)))?;

    let mapping = parsed.as_mapping().ok_or_else(|| {
        PluginError::MarkdownParse("frontmatter must be a YAML object".to_string())
    })?;

    // Extract name and description as required fields
    let name = mapping
        .get(serde_yaml::Value::String("name".to_string()))
        .and_then(|v| v.as_str())
        .ok_or_else(|| PluginError::MarkdownParse("frontmatter missing 'name' field".to_string()))?
        .to_string();

    let description = mapping
        .get(serde_yaml::Value::String("description".to_string()))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            PluginError::MarkdownParse("frontmatter missing 'description' field".to_string())
        })?
        .to_string();

    // Convert all metadata to HashMap
    let mut metadata = HashMap::new();
    for (k, v) in mapping.iter() {
        if let Some(key_str) = k.as_str() {
            if key_str != "name" && key_str != "description" {
                metadata.insert(key_str.to_string(), v.clone());
            }
        }
    }

    Ok(FrontmatterMetadata {
        name,
        description,
        metadata,
    })
}

/// Extract markdown body (content after frontmatter).
pub fn extract_body(content: &str) -> String {
    let trimmed = content.trim();

    if !trimmed.starts_with("---") {
        return trimmed.to_string();
    }

    let rest = &trimmed[3..];
    if let Some(closing_pos) = rest.find("---") {
        let body_start = closing_pos + 3;
        rest[body_start..].trim().to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_frontmatter() {
        let content = r#"---
name: "Test Agent"
description: "A test agent"
type: "agent"
---
# Content here
"#;
        let meta = parse_frontmatter(content).expect("parse failed");
        assert_eq!(meta.name, "Test Agent");
        assert_eq!(meta.description, "A test agent");
        assert_eq!(
            meta.metadata.get("type").and_then(|v| v.as_str()),
            Some("agent")
        );
    }
}
