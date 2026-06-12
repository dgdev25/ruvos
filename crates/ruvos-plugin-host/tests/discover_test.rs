use ruvos_plugin_host::{discover::PluginDiscoverer, PluginManifest};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_plugin_dir(parent: &TempDir, name: &str) -> (PathBuf, PluginManifest) {
    let plugin_dir = parent.path().join(name);
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");

    let manifest = PluginManifest {
        plugin: ruvos_plugin_host::PluginInfo {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: format!("{} plugin", name),
            license: "MIT".to_string(),
            authors: vec!["Test Author".to_string()],
        },
        capabilities: ruvos_plugin_host::PluginCapabilities {
            agents: vec![],
            skills: vec![],
            commands: vec![],
            hooks: vec![],
        },
        compat: Default::default(),
    };

    let manifest_path = plugin_dir.join("plugin.toml");
    let manifest_toml = toml::to_string(&manifest).expect("serialize manifest");
    fs::write(&manifest_path, manifest_toml).expect("write manifest");

    (plugin_dir, manifest)
}

#[test]
fn test_discover_plugins_in_directory() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let (plugin_dir, _manifest) = create_test_plugin_dir(&temp_dir, "test-plugin");

    let discoverer = PluginDiscoverer;
    let plugins = discoverer
        .discover_in_directory(temp_dir.path())
        .expect("discovery failed");

    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].name, "test-plugin");
    assert_eq!(plugins[0].manifest.plugin.name, "test-plugin");
    assert_eq!(plugins[0].manifest.plugin.version, "1.0.0");
    assert_eq!(plugins[0].path, plugin_dir);
}

#[test]
fn test_discover_with_agents() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let (plugin_dir, _manifest) = create_test_plugin_dir(&temp_dir, "agent-plugin");

    // Create agents directory
    let agents_dir = plugin_dir.join("agents");
    fs::create_dir_all(&agents_dir).expect("create agents dir");

    // Create test agent file
    let agent_content = r#"---
name: "Test Agent"
description: "A test agent for discovery"
archetype: "coder"
---
# Test Agent

This is a test agent.
"#;
    let agent_path = agents_dir.join("test-agent.md");
    fs::write(&agent_path, agent_content).expect("write agent file");

    let discoverer = PluginDiscoverer;
    let plugins = discoverer
        .discover_in_directory(temp_dir.path())
        .expect("discovery failed");

    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].agents.len(), 1);
    assert_eq!(plugins[0].agents[0].name, "Test Agent");
    assert_eq!(
        plugins[0].agents[0].description,
        "A test agent for discovery"
    );
}

#[test]
fn test_discover_command_with_exec_entrypoint() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let (plugin_dir, _manifest) = create_test_plugin_dir(&temp_dir, "cmd-plugin");

    let commands_dir = plugin_dir.join("commands");
    fs::create_dir_all(&commands_dir).expect("create commands dir");

    let with_exec = r#"---
name: "greet"
description: "Echo a greeting"
exec: "echo"
args:
  - "hello"
---
# greet
"#;
    fs::write(commands_dir.join("greet.md"), with_exec).expect("write command file");

    let without_exec = r#"---
name: "noop"
description: "Declares no entrypoint"
---
# noop
"#;
    fs::write(commands_dir.join("noop.md"), without_exec).expect("write command file");

    let discoverer = PluginDiscoverer;
    let plugins = discoverer
        .discover_in_directory(temp_dir.path())
        .expect("discovery failed");

    assert_eq!(plugins.len(), 1);
    let mut commands = plugins[0].commands.clone();
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(commands.len(), 2);

    assert_eq!(commands[0].name, "greet");
    assert_eq!(commands[0].exec.as_deref(), Some("echo"));
    assert_eq!(commands[0].exec_args, vec!["hello".to_string()]);

    assert_eq!(commands[1].name, "noop");
    assert_eq!(commands[1].exec, None);
    assert!(commands[1].exec_args.is_empty());
}
