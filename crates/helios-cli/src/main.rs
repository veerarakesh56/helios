use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "helios",
    version,
    about = "Deterministic failure simulation for cloud infrastructure"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(long, global = true, default_value = "info")]
    log_level: String,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a Terraform JSON input (`terraform show -json` of a state or of a saved plan) and
    /// summarise the resource graph.
    Plan {
        /// Path to a directory containing terraform-show.json, or to the JSON file itself.
        input: PathBuf,
    },
    /// Run a failure scenario against the resource graph.
    Simulate {
        input: PathBuf,
        #[arg(long)]
        scenario: PathBuf,
        /// Emit FailureChain as JSON on stdout instead of plain text.
        #[arg(long)]
        json: bool,
    },
    /// Re-run simulation with a proposed fix applied and confirm it resolves failures.
    Verify {
        input: PathBuf,
        #[arg(long)]
        scenario: PathBuf,
        #[arg(long)]
        fix: PathBuf,
    },
    /// Emit a combined `{graph, chain}` JSON document for the web viewer / GitHub Action.
    Inspect {
        input: PathBuf,
        #[arg(long)]
        scenario: PathBuf,
    },
    /// Narrate a FailureChain (read as JSON on stdin) via the helios-ai Python shell.
    Explain,
    /// Simulate, then ask helios-ai for a FixProposal (JSON on stdout, ready for `verify --fix`).
    /// Only the FAILED resources' attributes are sent, with sensitive values redacted.
    ProposeFix {
        input: PathBuf,
        #[arg(long)]
        scenario: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // stderr, NOT stdout. `simulate --json` and `inspect` write a document to stdout that the
    // GitHub Action redirects to a file and parses with `jq`; a warning on stdout corrupts it, so
    // one skipped resource would break the PR comment on any real repository.
    // No colour codes unless a person is watching: CI logs, the Action's captured stderr and
    // `2> file` must stay plain text.
    use std::io::IsTerminal;
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .with_env_filter(tracing_subscriber::EnvFilter::new(&cli.log_level))
        .init();

    match cli.command {
        Command::Plan { input } => cmd_plan(&input),
        Command::Simulate {
            input,
            scenario,
            json,
        } => cmd_simulate(&input, &scenario, json),
        Command::Verify {
            input,
            scenario,
            fix,
        } => cmd_verify(&input, &scenario, &fix),
        Command::Inspect { input, scenario } => cmd_inspect(&input, &scenario),
        Command::Explain => cmd_explain(),
        Command::ProposeFix { input, scenario } => cmd_propose_fix(&input, &scenario),
    }
}

fn cmd_plan(input: &std::path::Path) -> Result<()> {
    let (graph, source) = helios_graph::load_with_source(input)?;
    println!("source: {source}");
    println!(
        "loaded {} resources, {} dependency edges",
        graph.node_count(),
        graph.edge_count()
    );
    Ok(())
}

fn cmd_simulate(input: &std::path::Path, scenario: &std::path::Path, json: bool) -> Result<()> {
    let graph = helios_graph::load(input)?;
    let scenario = helios_engine::scenario::load(scenario)
        .map_err(|e| anyhow::anyhow!("loading scenario: {e}"))?;
    let chain = simulate(&graph, &scenario)?;
    if json {
        serde_json::to_writer_pretty(std::io::stdout().lock(), &chain)?;
        println!();
    } else {
        print!("{}", chain.render_plain());
    }
    if !chain.is_safe() {
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_verify(
    input: &std::path::Path,
    scenario: &std::path::Path,
    fix: &std::path::Path,
) -> Result<()> {
    let graph = helios_graph::load(input)?;
    let scenario = helios_engine::scenario::load(scenario)
        .map_err(|e| anyhow::anyhow!("loading scenario: {e}"))?;
    let fix = helios_engine::fix::load(fix).map_err(|e| anyhow::anyhow!("loading fix: {e}"))?;
    let report = helios_engine::verify(&graph, &scenario, &fix).map_err(|e| {
        if let helios_engine::VerifyError::Simulate(inner) = &e {
            exit_if_inconclusive(inner);
        }
        anyhow::anyhow!("verify: {e}")
    })?;

    println!("Scenario: {}", report.pre_fix.scenario);
    println!("Pre-fix failures:  {}", report.pre_fix.failures.len());
    println!("Post-fix failures: {}", report.post_fix.failures.len());

    if !report.resolved.is_empty() {
        println!("\nResolved ({}):", report.resolved.len());
        for r in &report.resolved {
            println!("  [OK] {r}");
        }
    }
    if !report.new_failures.is_empty() {
        println!("\nNew failures introduced ({}):", report.new_failures.len());
        for n in &report.new_failures {
            println!("  [NEW] {n}");
        }
    }
    if !report.remaining.is_empty() {
        println!("\nStill failing ({}):", report.remaining.len());
        for r in &report.remaining {
            println!("  [--] {r}");
        }
    }
    if !report.is_safe() {
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_inspect(input: &std::path::Path, scenario: &std::path::Path) -> Result<()> {
    let graph = helios_graph::load(input)?;
    let scenario = helios_engine::scenario::load(scenario)
        .map_err(|e| anyhow::anyhow!("loading scenario: {e}"))?;
    let chain = simulate(&graph, &scenario)?;
    let doc = helios_engine::build_inspect(&graph, chain);
    serde_json::to_writer_pretty(std::io::stdout().lock(), &doc)?;
    println!();
    Ok(())
}

/// Exit code for a scenario Helios cannot evaluate: distinct from 0 (resilient) and 1 (failures,
/// or an error), so a pipeline cannot read "cannot say" as either.
const EXIT_INCONCLUSIVE: i32 = 3;

/// [`helios_engine::simulate()`], exiting [`EXIT_INCONCLUSIVE`] on an inconclusive scenario.
fn simulate(
    graph: &helios_graph::ResourceGraph,
    scenario: &helios_engine::Scenario,
) -> Result<helios_engine::FailureChain> {
    helios_engine::simulate(graph, scenario).map_err(|e| {
        exit_if_inconclusive(&e);
        anyhow::anyhow!("simulate: {e}")
    })
}

fn exit_if_inconclusive(e: &helios_engine::SimulateError) {
    if matches!(e, helios_engine::SimulateError::Inconclusive(_)) {
        eprintln!("helios: {e}");
        std::process::exit(EXIT_INCONCLUSIVE);
    }
}

/// Narrate a FailureChain read as JSON on stdin, via `python -m helios_ai explain`.
fn cmd_explain() -> Result<()> {
    use std::io::Read;

    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| anyhow::anyhow!("reading FailureChain JSON from stdin: {e}"))?;
    // Windows PowerShell 5.1 prefixes a byte-order mark to what it pipes into a native command.
    run_helios_ai("explain", input.trim_start_matches('\u{feff}'))
}

/// Simulate, then pipe `{chain, attrs_snapshot}` to `python -m helios_ai propose-fix`, whose
/// FixProposal JSON goes straight to stdout.
fn cmd_propose_fix(input: &std::path::Path, scenario: &std::path::Path) -> Result<()> {
    let graph = helios_graph::load(input)?;
    let scenario = helios_engine::scenario::load(scenario)
        .map_err(|e| anyhow::anyhow!("loading scenario: {e}"))?;
    let chain = simulate(&graph, &scenario)?;
    if chain.is_safe() {
        // Nothing on stdout: a pipeline into `verify --fix` must not receive a proposal for nothing.
        eprintln!("no failures under {} — nothing to fix", chain.scenario);
        return Ok(());
    }
    run_helios_ai("propose-fix", &fix_request(&graph, &chain).to_string())
}

/// The propose-fix request: the chain, and the scrubbed attrs of the resources in it — not the
/// whole graph, which is more than the model needs and more than should leave the machine.
fn fix_request(
    graph: &helios_graph::ResourceGraph,
    chain: &helios_engine::FailureChain,
) -> serde_json::Value {
    let attrs_snapshot: serde_json::Map<String, serde_json::Value> = chain
        .failures
        .iter()
        .filter_map(|f| {
            let idx = graph.node_indices().find(|i| graph[*i].id == f.id)?;
            Some((f.id.clone(), helios_engine::scrub_resource(&graph[idx])))
        })
        .collect();
    serde_json::json!({ "chain": chain, "attrs_snapshot": attrs_snapshot })
}

/// The interpreter that has `helios_ai` installed: `HELIOS_AI_PYTHON` if set, else the repo venv
/// (`helios-ai/.venv`, relative to the working directory) if it exists, else `python` on PATH.
fn helios_ai_python() -> PathBuf {
    if let Some(p) = std::env::var_os("HELIOS_AI_PYTHON") {
        return PathBuf::from(p);
    }
    for rel in [
        "helios-ai/.venv/Scripts/python.exe",
        "helios-ai/.venv/bin/python",
    ] {
        let p = PathBuf::from(rel);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("python")
}

/// Run `python -m helios_ai <sub>`, writing `stdin` to it and passing its stdout/stderr through.
/// Fails if the process cannot be spawned or exits non-zero.
fn run_helios_ai(sub: &str, stdin: &str) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let python = helios_ai_python();
    let shown = python.display();
    let mut child = Command::new(&python)
        .args(["-m", "helios_ai", sub])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            anyhow::anyhow!(
                "spawning `{shown} -m helios_ai {sub}` — is helios-ai installed? Set HELIOS_AI_PYTHON \
                 or create helios-ai/.venv ({e})"
            )
        })?;

    // Drop the pipe after writing so the child sees EOF.
    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(stdin.as_bytes())?;

    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("`{shown} -m helios_ai {sub}` failed: {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fix_request_carries_only_the_failed_resources_scrubbed() {
        let graph = helios_graph::from_json(
            r#"{"format_version":"1.0","values":{"root_module":{"resources":[
              {"address":"aws_db_instance.db","type":"aws_db_instance",
               "values":{"id":"db","availability_zone":"us-east-1a","password":"hunter2",
                 "note":"postgres://app:fake-pw-3@db"},
               "sensitive_values":{"note":true}},
              {"address":"aws_s3_bucket.b","type":"aws_s3_bucket","values":{"id":"b"}}]}}}"#,
        )
        .unwrap();
        let chain = helios_engine::FailureChain {
            scenario: "s".into(),
            failures: vec![helios_engine::FailedResource {
                id: "aws_db_instance.db".into(),
                kind: "DbInstance".into(),
                reason: "r".into(),
            }],
        };
        let req = fix_request(&graph, &chain);
        let snapshot = req["attrs_snapshot"].as_object().unwrap();
        assert_eq!(
            snapshot.keys().collect::<Vec<_>>(),
            vec!["aws_db_instance.db"]
        );
        assert_eq!(
            snapshot["aws_db_instance.db"]["password"],
            helios_engine::REDACTED
        );
        assert_eq!(
            snapshot["aws_db_instance.db"]["availability_zone"],
            "us-east-1a"
        );
        assert_eq!(
            snapshot["aws_db_instance.db"]["note"],
            helios_engine::REDACTED
        );
        assert!(!req.to_string().contains("hunter2") && !req.to_string().contains("fake-pw-3"));
        assert_eq!(req["chain"]["scenario"], "s");
    }
}
