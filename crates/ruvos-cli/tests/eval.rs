use std::process::Command;

// ── compress ──────────────────────────────────────────────────────────────────

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

// ── orchestrate-handoff ───────────────────────────────────────────────────────

#[test]
fn eval_orchestrate_handoff_emits_json_report() {
    let output = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args(["eval", "orchestrate-handoff"])
        .output()
        .expect("run ruvos eval orchestrate-handoff");

    assert!(
        output.status.success(),
        "command failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("expected JSON");
    assert_eq!(v["report"]["suite"], "orchestrate-handoff");
    assert_eq!(v["report"]["summary"]["case_count"], 5);
    assert_eq!(v["report"]["summary"]["all_correct"], true);
    assert_eq!(v["report"]["summary"]["routing_correct"], true);
}

#[test]
fn eval_orchestrate_handoff_write_and_compare() {
    let dir = tempfile::tempdir().expect("tempdir");
    let baseline_path = dir.path().join("handoff-baseline.json");

    let write_out = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args([
            "eval",
            "orchestrate-handoff",
            "--write",
            baseline_path.to_str().unwrap(),
        ])
        .output()
        .expect("write baseline");
    assert!(write_out.status.success(), "write failed");
    assert!(baseline_path.exists());

    let cmp_out = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args([
            "eval",
            "orchestrate-handoff",
            "--compare-to",
            baseline_path.to_str().unwrap(),
        ])
        .output()
        .expect("compare");
    assert!(cmp_out.status.success(), "compare failed");

    let stdout = String::from_utf8(cmp_out.stdout).expect("utf8");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("JSON");
    assert_eq!(v["comparison"]["suite_matches"], true);
    assert_eq!(v["comparison"]["case_count_matches"], true);
    assert_eq!(v["comparison"]["all_correct_current"], true);
}

// ── swarm-recovery ────────────────────────────────────────────────────────────

#[test]
fn eval_swarm_recovery_emits_json_report() {
    let output = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args(["eval", "swarm-recovery"])
        .output()
        .expect("run ruvos eval swarm-recovery");

    assert!(
        output.status.success(),
        "command failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("expected JSON");
    assert_eq!(v["report"]["suite"], "swarm-recovery");
    assert_eq!(v["report"]["summary"]["case_count"], 4);
    assert_eq!(v["report"]["summary"]["all_passed"], true);
}

#[test]
fn eval_swarm_recovery_write_and_compare() {
    let dir = tempfile::tempdir().expect("tempdir");
    let baseline_path = dir.path().join("swarm-recovery-baseline.json");

    let write_out = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args([
            "eval",
            "swarm-recovery",
            "--write",
            baseline_path.to_str().unwrap(),
        ])
        .output()
        .expect("write baseline");
    assert!(write_out.status.success(), "write failed");
    assert!(baseline_path.exists());

    let cmp_out = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args([
            "eval",
            "swarm-recovery",
            "--compare-to",
            baseline_path.to_str().unwrap(),
        ])
        .output()
        .expect("compare");
    assert!(cmp_out.status.success(), "compare failed");

    let stdout = String::from_utf8(cmp_out.stdout).expect("utf8");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("JSON");
    assert_eq!(v["comparison"]["suite_matches"], true);
    assert_eq!(v["comparison"]["all_passed_current"], true);
}

// ── skill-routing ─────────────────────────────────────────────────────────────

#[test]
fn eval_skill_routing_emits_json_report() {
    let output = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args(["eval", "skill-routing"])
        .output()
        .expect("run ruvos eval skill-routing");

    assert!(
        output.status.success(),
        "command failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("expected JSON");
    assert_eq!(v["report"]["suite"], "skill-routing");
    assert_eq!(v["report"]["summary"]["case_count"], 8);
    assert_eq!(v["report"]["summary"]["all_hints_correct"], true);
}

#[test]
fn eval_skill_routing_write_and_compare() {
    let dir = tempfile::tempdir().expect("tempdir");
    let baseline_path = dir.path().join("skill-routing-baseline.json");

    let write_out = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args([
            "eval",
            "skill-routing",
            "--write",
            baseline_path.to_str().unwrap(),
        ])
        .output()
        .expect("write baseline");
    assert!(write_out.status.success(), "write failed");
    assert!(baseline_path.exists());

    let cmp_out = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args([
            "eval",
            "skill-routing",
            "--compare-to",
            baseline_path.to_str().unwrap(),
        ])
        .output()
        .expect("compare");
    assert!(cmp_out.status.success(), "compare failed");

    let stdout = String::from_utf8(cmp_out.stdout).expect("utf8");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("JSON");
    assert_eq!(v["comparison"]["suite_matches"], true);
    assert_eq!(v["comparison"]["all_hints_correct_current"], true);
}

// ── swarm-learning ────────────────────────────────────────────────────────────

#[test]
fn eval_swarm_learning_emits_json_report() {
    let output = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args(["eval", "swarm-learning"])
        .output()
        .expect("run ruvos eval swarm-learning");

    assert!(
        output.status.success(),
        "command failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("expected JSON");
    assert_eq!(v["report"]["suite"], "swarm-learning");
    assert_eq!(v["report"]["summary"]["case_count"], 5);
    // convergence_rate == 1.0 means all cases converged.
    assert_eq!(v["report"]["summary"]["convergence_rate"], 1.0);
}

#[test]
fn eval_swarm_learning_write_and_compare() {
    let dir = tempfile::tempdir().expect("tempdir");
    let baseline_path = dir.path().join("swarm-learning-baseline.json");

    let write_out = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args([
            "eval",
            "swarm-learning",
            "--write",
            baseline_path.to_str().unwrap(),
        ])
        .output()
        .expect("write baseline");
    assert!(write_out.status.success(), "write failed");
    assert!(baseline_path.exists());

    let cmp_out = Command::new(env!("CARGO_BIN_EXE_ruvos"))
        .args([
            "eval",
            "swarm-learning",
            "--compare-to",
            baseline_path.to_str().unwrap(),
        ])
        .output()
        .expect("compare");
    assert!(cmp_out.status.success(), "compare failed");

    let stdout = String::from_utf8(cmp_out.stdout).expect("utf8");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("JSON");
    assert_eq!(v["comparison"]["suite_matches"], true);
    assert_eq!(v["comparison"]["case_count_matches"], true);
    assert_eq!(v["comparison"]["all_converged_current"], true);
}
