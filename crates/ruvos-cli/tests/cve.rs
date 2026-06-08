use std::io::Write;
use std::process::Command;
use tempfile::tempdir;

fn ruvos_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_ruvos"))
}

fn write_fixture(dir: &std::path::Path, filename: &str, content: &str) {
    let path = dir.join(filename);
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

const CLEAN_LOCK: &str = r#"{
  "lockfileVersion": 3,
  "packages": {
    "node_modules/once": { "version": "1.4.0", "dev": false }
  }
}"#;

#[test]
fn scan_no_lockfile_exits_zero() {
    let dir = tempdir().unwrap();
    let out = Command::new(ruvos_bin())
        .args([
            "cve",
            "scan",
            "--json",
            "--no-cache",
            dir.path().to_str().unwrap(),
        ])
        .env("RUVOS_HOME", dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("must output valid JSON");
    assert_eq!(v["has_vulnerabilities"], false);
}

#[test]
fn scan_json_output_is_valid() {
    let dir = tempdir().unwrap();
    write_fixture(dir.path(), "package-lock.json", CLEAN_LOCK);
    let out = Command::new(ruvos_bin())
        .args([
            "cve",
            "scan",
            "--json",
            "--no-cache",
            dir.path().to_str().unwrap(),
        ])
        .env("RUVOS_HOME", dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("JSON output must be valid");
    assert!(v.get("findings").is_some(), "missing 'findings' key");
    assert!(v.get("total_packages_scanned").is_some());
}

#[test]
fn scan_sarif_output_is_valid() {
    let dir = tempdir().unwrap();
    write_fixture(dir.path(), "package-lock.json", CLEAN_LOCK);
    let out = Command::new(ruvos_bin())
        .args([
            "cve",
            "scan",
            "--sarif",
            "--no-cache",
            dir.path().to_str().unwrap(),
        ])
        .env("RUVOS_HOME", dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("SARIF must be valid JSON");
    assert_eq!(v["version"], "2.1.0");
}

#[test]
fn scan_terminal_output_contains_package_count() {
    let dir = tempdir().unwrap();
    write_fixture(dir.path(), "package-lock.json", CLEAN_LOCK);
    let out = Command::new(ruvos_bin())
        .args(["cve", "scan", "--no-cache", dir.path().to_str().unwrap()])
        .env("RUVOS_HOME", dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("1"), "got: {stdout}");
}

#[test]
fn pnpm_lockfile_parsed() {
    let dir = tempdir().unwrap();
    let pnpm_yaml = "lockfileVersion: 5.4\npackages:\n  /react@18.0.0:\n    resolution: {integrity: sha512-abc}\n    dev: false\n";
    write_fixture(dir.path(), "pnpm-lock.yaml", pnpm_yaml);
    let out = Command::new(ruvos_bin())
        .args([
            "cve",
            "scan",
            "--json",
            "--no-cache",
            dir.path().to_str().unwrap(),
        ])
        .env("RUVOS_HOME", dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(v["total_packages_scanned"].as_u64().unwrap_or(0) >= 1);
}
