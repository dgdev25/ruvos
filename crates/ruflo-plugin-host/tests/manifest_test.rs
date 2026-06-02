use ruflo_plugin_host::manifest::parse_manifest;

#[test]
fn test_parse_valid_manifest() {
    let toml_content = r#"
[plugin]
name = "test-plugin"
version = "1.0.0"
description = "A test plugin"
license = "MIT"
authors = ["Test Author"]

[capabilities]
agents = ["test-agent"]
skills = ["test-skill"]
commands = ["test"]

[compat]
ruflo_min = "4.0.0"
"#;

    let manifest = parse_manifest(toml_content).expect("parse failed");
    assert_eq!(manifest.plugin.name, "test-plugin");
    assert_eq!(manifest.plugin.version, "1.0.0");
    assert_eq!(manifest.capabilities.agents.len(), 1);
    assert_eq!(manifest.compat.ruflo_min, Some("4.0.0".to_string()));
}

#[test]
fn test_parse_manifest_minimal() {
    let toml_content = r#"
[plugin]
name = "minimal"
version = "0.1.0"
description = "Minimal plugin"
license = "MIT"
authors = []

[capabilities]
"#;

    let manifest = parse_manifest(toml_content).expect("parse failed");
    assert_eq!(manifest.plugin.name, "minimal");
    assert!(manifest.capabilities.agents.is_empty());
}

#[test]
fn test_parse_manifest_invalid() {
    let toml_content = "invalid toml [[[";
    let result = parse_manifest(toml_content);
    assert!(result.is_err());
}
