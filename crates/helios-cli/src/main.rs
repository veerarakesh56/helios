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
    /// Parse a Terraform JSON input and print the resource graph.
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
    /// Narrate a FailureChain (read as JSON on stdin) via the helios-ai Python shell.
    Explain,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
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
        Command::Explain => cmd_explain(),
    }
}

fn cmd_plan(input: &std::path::Path) -> Result<()> {
    let graph = helios_graph::load(input)?;
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
    let chain =
        helios_engine::simulate(&graph, &scenario).map_err(|e| anyhow::anyhow!("simulate: {e}"))?;
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
    let report = helios_engine::verify(&graph, &scenario, &fix)
        .map_err(|e| anyhow::anyhow!("verify: {e}"))?;

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

/// Shell out to `python -m helios_ai explain`, piping stdin through and stdout back.
///
/// The Python shell must be on PATH with `helios_ai` importable (e.g. from the
/// helios-ai/.venv activated, or via `uv run --project helios-ai python -m ...`).
/// Override the interpreter with `HELIOS_AI_PYTHON=/path/to/python`.
fn cmd_explain() -> Result<()> {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};

    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| anyhow::anyhow!("reading FailureChain JSON from stdin: {e}"))?;

    let python = std::env::var("HELIOS_AI_PYTHON").unwrap_or_else(|_| "python".to_string());

    let mut child = Command::new(&python)
        .args(["-m", "helios_ai", "explain"])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            anyhow::anyhow!(
                "spawning `{python} -m helios_ai explain` — is helios-ai installed? ({e})"
            )
        })?;

    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(input.as_bytes())?;

    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("`{python} -m helios_ai explain` failed: {status}");
    }
    Ok(())
}
