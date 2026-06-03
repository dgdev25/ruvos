use ruflo_plugin_host::parser::{extract_body, parse_frontmatter};

#[test]
fn test_parse_skill_frontmatter() {
    let content = r#"---
name: "Code Review Skill"
description: "Performs systematic code review with expert feedback"
version: "1.0.0"
author: "Anthropic"
tags: ["review", "code-quality"]
---
# Code Review Skill

This skill provides comprehensive code review capabilities.
"#;

    let meta = parse_frontmatter(content).expect("parse failed");
    assert_eq!(meta.name, "Code Review Skill");
    assert_eq!(
        meta.description,
        "Performs systematic code review with expert feedback"
    );
    assert_eq!(
        meta.metadata.get("version").and_then(|v| v.as_str()),
        Some("1.0.0")
    );
    assert_eq!(
        meta.metadata.get("author").and_then(|v| v.as_str()),
        Some("Anthropic")
    );
}

#[test]
fn test_parse_agent_frontmatter() {
    let content = r#"---
name: "Coder Agent"
description: "Specialized agent for writing and refactoring code"
archetype: "coder"
traits: ["tdd", "backend"]
---
# Coder Agent

This agent specializes in code implementation.
"#;

    let meta = parse_frontmatter(content).expect("parse failed");
    assert_eq!(meta.name, "Coder Agent");
    assert_eq!(
        meta.description,
        "Specialized agent for writing and refactoring code"
    );
    assert_eq!(
        meta.metadata.get("archetype").and_then(|v| v.as_str()),
        Some("coder")
    );
}

#[test]
fn test_parse_missing_frontmatter() {
    let content = r#"# No Frontmatter

This is just markdown content without frontmatter.
"#;

    let result = parse_frontmatter(content);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("frontmatter must start with"));
}

#[test]
fn test_parse_invalid_yaml() {
    let content = r#"---
name: "Test"
description: "Test"
invalid: [unclosed list
---
Content
"#;

    let result = parse_frontmatter(content);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("invalid YAML"));
}

#[test]
fn test_extract_body() {
    let content = r#"---
name: "Test"
description: "Test"
---
# Body Content

This is the markdown body.
"#;

    let body = extract_body(content);
    assert!(body.contains("# Body Content"));
    assert!(body.contains("This is the markdown body."));
    assert!(!body.contains("---"));
    assert!(!body.contains("name:"));
}

#[test]
fn test_extract_body_no_frontmatter() {
    let content = r#"# Direct Content

No frontmatter here.
"#;

    let body = extract_body(content);
    assert_eq!(body, "# Direct Content\n\nNo frontmatter here.");
}
