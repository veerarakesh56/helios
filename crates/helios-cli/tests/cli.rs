//! CLI integration tests — spawn the compiled `helios` binary and inspect stdout.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn simulate_json_emits_failure_chain_as_json() {
    let root = repo_root();
    let output = Command::new(env!("CARGO_BIN_EXE_helios"))
        .current_dir(&root)
        .args([
            "simulate",
            "fixtures/three-tier-webapp",
            "--scenario",
            "fixtures/scenarios/az-outage.yaml",
            "--json",
        ])
        .output()
        .expect("failed to spawn helios");

    // az-outage produces failures → non-zero exit
    assert!(
        !output.status.success(),
        "expected non-zero exit on failures"
    );

    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout not valid JSON: {e}\nstdout: {:?}",
            String::from_utf8_lossy(&output.stdout)
        )
    });

    assert_eq!(parsed["scenario"], "lose-us-east-1a");
    assert!(parsed["failures"].is_array());
    assert!(
        parsed["failures"].as_array().unwrap().len() >= 3,
        "expected at least 3 failures, got {:?}",
        parsed["failures"]
    );
}

#[test]
fn simulate_plain_still_works() {
    let root = repo_root();
    let output = Command::new(env!("CARGO_BIN_EXE_helios"))
        .current_dir(&root)
        .args([
            "simulate",
            "fixtures/three-tier-webapp",
            "--scenario",
            "fixtures/scenarios/az-outage.yaml",
        ])
        .output()
        .expect("failed to spawn helios");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("lose-us-east-1a") || stdout.contains("FAIL") || stdout.contains("aws_"),
        "plain render should mention the scenario or failures; got: {stdout}"
    );
}
