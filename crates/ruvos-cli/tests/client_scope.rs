use std::fs;
use std::path::Path;

#[test]
fn frozen_client_scope_is_documented() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let checklist = fs::read_to_string(root.join("docs/roadmaps/compress-baseline-checklist.md"))
        .expect("read checklist");
    let readme = fs::read_to_string(root.join("README.md")).expect("read README");

    for client in ["Claude Code", "Codex CLI", "Gemini CLI"] {
        assert!(
            checklist.contains(client),
            "checklist must list supported client {client}"
        );
    }

    for client in ["Cursor", "Aider", "Copilot CLI", "OpenClaw"] {
        assert!(
            checklist.contains(client),
            "checklist must list dropped client {client}"
        );
    }

    for client in ["Claude Code", "Codex CLI", "Gemini CLI"] {
        assert!(
            readme.contains(client),
            "README must describe the frozen baseline client scope"
        );
    }
}
