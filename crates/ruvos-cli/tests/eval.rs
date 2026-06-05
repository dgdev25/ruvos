use std::process::Command;

#[test]
fn eval_compress_emits_json_report() {
    let output = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args(["eval", "compress"])
        .output()
        .expect("run ruvos eval compress");

    assert!(output.status.success(), "command failed");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("expected JSON regression report");
    assert_eq!(report["report"]["suite"], "compress-regression");
    assert_eq!(report["report"]["summary"]["case_count"], 4);
}

#[test]
fn eval_compress_can_compare_against_saved_report() {
    let dir = tempfile::tempdir().expect("tempdir");
    let baseline_path = dir.path().join("baseline.json");

    let baseline = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args([
            "eval",
            "compress",
            "--write",
            baseline_path.to_str().expect("utf8 path"),
        ])
        .output()
        .expect("run baseline report");
    assert!(baseline.status.success(), "baseline command failed");
    assert!(baseline_path.exists(), "baseline report should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args([
            "eval",
            "compress",
            "--compare-to",
            baseline_path.to_str().expect("utf8 path"),
        ])
        .output()
        .expect("run compare report");
    assert!(output.status.success(), "compare command failed");

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("expected JSON compare report");
    assert_eq!(report["comparison"]["suite_matches"], true);
    assert_eq!(report["comparison"]["case_count_matches"], true);
    assert_eq!(report["comparison"]["matching_case_names"], true);
}
