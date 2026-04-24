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

/// Locate helios-ai/.venv/{Scripts|bin}/python for the e2e test, or None if absent.
fn venv_python(root: &std::path::Path) -> Option<PathBuf> {
    for rel in [
        "helios-ai/.venv/Scripts/python.exe",
        "helios-ai/.venv/bin/python",
    ] {
        let p = root.join(rel);
        if p.exists() {
            return Some(p);
        }
    }
    None
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
fn explain_subcommand_pipes_stdin_to_python() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let root = repo_root();
    let Some(python) = venv_python(&root) else {
        eprintln!("skipping: helios-ai/.venv not found — run `uv sync` in helios-ai/");
        return;
    };

    let chain = r#"{"scenario":"e2e-test","failures":[]}"#;

    let mut child = Command::new(env!("CARGO_BIN_EXE_helios"))
        .current_dir(&root)
        .arg("explain")
        .env("HELIOS_AI_PYTHON", &python)
        .env("HELIOS_AI_MOCK", "1")
        .env_remove("ANTHROPIC_API_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn helios explain");

    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(chain.as_bytes())
        .unwrap();

    let output = child.wait_with_output().expect("wait helios explain");
    assert!(
        output.status.success(),
        "helios explain failed: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("e2e-test") && stdout.contains("mocked"),
        "expected mock narrative mentioning scenario, got: {stdout}"
    );
}

#[test]
fn propose_fix_subcommand_emits_valid_fix_json_via_mock() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let root = repo_root();
    let Some(python) = venv_python(&root) else {
        eprintln!("skipping: helios-ai/.venv not found — run `uv sync` in helios-ai/");
        return;
    };

    let payload = r#"{"chain":{"scenario":"lose-us-east-1a","failures":[{"id":"aws_elasticache_cluster.cache","kind":"ElasticacheCluster","reason":"single-AZ in us-east-1a, which is down"}]},"attrs_snapshot":{"aws_elasticache_cluster.cache":{"availability_zone":"us-east-1a"}}}"#;

    let mut child = Command::new(&python)
        .args(["-m", "helios_ai", "propose-fix"])
        .current_dir(root.join("helios-ai"))
        .env("HELIOS_AI_MOCK", "1")
        .env_remove("ANTHROPIC_API_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn helios_ai propose-fix");

    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(payload.as_bytes())
        .unwrap();

    let output = child
        .wait_with_output()
        .expect("wait helios_ai propose-fix");
    assert!(
        output.status.success(),
        "propose-fix failed: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout not valid JSON: {e}\nstdout: {:?}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(parsed["scenario_name"], "lose-us-east-1a");
    let edits = parsed["edits"].as_array().expect("edits is array");
    assert!(!edits.is_empty(), "expected at least one edit");
    assert_eq!(edits[0]["op"], "set_attr");
}

#[test]
fn verify_with_resolving_fix_reports_resolved_section() {
    let root = repo_root();
    let tmp = tempfile::tempdir().unwrap();
    let fix_path = tmp.path().join("fix.json");
    std::fs::write(
        &fix_path,
        r#"{
            "scenario_name": "lose-us-east-1a",
            "explanation": "move cache to us-east-1b",
            "edits": [
                {"op":"set_attr","resource_id":"aws_elasticache_cluster.cache","key":"availability_zone","value":"us-east-1b"}
            ]
        }"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_helios"))
        .current_dir(&root)
        .args([
            "verify",
            "fixtures/three-tier-webapp",
            "--scenario",
            "fixtures/scenarios/az-outage.yaml",
            "--fix",
        ])
        .arg(&fix_path)
        .output()
        .expect("failed to spawn helios verify");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Resolved") && stdout.contains("aws_elasticache_cluster.cache"),
        "expected Resolved section naming cache; got:\n{stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Some failures remain (subnet + instance), so exit code is non-zero.
    assert!(
        !output.status.success(),
        "expected non-zero exit (remaining failures)"
    );
}

#[test]
fn verify_rejects_fix_naming_unknown_resource() {
    let root = repo_root();
    let tmp = tempfile::tempdir().unwrap();
    let fix_path = tmp.path().join("bad.json");
    std::fs::write(
        &fix_path,
        r#"{"scenario_name":"x","explanation":"x","edits":[{"op":"set_attr","resource_id":"aws_nope.ghost","key":"foo","value":1}]}"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_helios"))
        .current_dir(&root)
        .args([
            "verify",
            "fixtures/three-tier-webapp",
            "--scenario",
            "fixtures/scenarios/az-outage.yaml",
            "--fix",
        ])
        .arg(&fix_path)
        .output()
        .expect("spawn helios verify");

    assert!(
        !output.status.success(),
        "expected non-zero exit on unknown resource"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown resource") || stderr.contains("aws_nope.ghost"),
        "expected unknown-resource error; stderr: {stderr}"
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
