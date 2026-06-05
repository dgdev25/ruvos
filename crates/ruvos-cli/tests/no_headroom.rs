use std::fs;
use std::path::{Path, PathBuf};

fn scan_dir(dir: &Path, hits: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, hits);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if contents.to_ascii_lowercase().contains("headroom") {
            hits.push(path);
        }
    }
}

#[test]
fn runtime_sources_do_not_reference_headroom() {
    let roots = [
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../ruvos-cli/src"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ruvos-mcp/src"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ruvos-hooks/src"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ruvos-session/src"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ruvos-host/src"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ruvos-plugin-host/src"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../ruvos-compress/src"),
    ];

    let mut hits = Vec::new();
    for root in roots {
        scan_dir(&root, &mut hits);
    }

    assert!(
        hits.is_empty(),
        "runtime sources must not reference headroom: {:?}",
        hits
    );
}
