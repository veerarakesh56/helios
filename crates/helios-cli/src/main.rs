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
    },
    /// Re-run simulation with a proposed fix applied and confirm it resolves failures.
    Verify {
        input: PathBuf,
        #[arg(long)]
        scenario: PathBuf,
        #[arg(long)]
        fix: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(&cli.log_level))
        .init();

    match cli.command {
        Command::Plan { input } => cmd_plan(&input),
        Command::Simulate { input, scenario } => cmd_simulate(&input, &scenario),
        Command::Verify {
            input,
            scenario,
            fix,
        } => cmd_verify(&input, &scenario, &fix),
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

fn cmd_simulate(input: &std::path::Path, scenario: &std::path::Path) -> Result<()> {
    let graph = helios_graph::load(input)?;
    let scenario = helios_engine::scenario::load(scenario)
        .map_err(|e| anyhow::anyhow!("loading scenario: {e}"))?;
    let chain =
        helios_engine::simulate(&graph, &scenario).map_err(|e| anyhow::anyhow!("simulate: {e}"))?;
    print!("{}", chain.render_plain());
    if !chain.is_safe() {
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_verify(
    _input: &std::path::Path,
    _scenario: &std::path::Path,
    _fix: &std::path::Path,
) -> Result<()> {
    anyhow::bail!("`helios verify` lands Weekend 4 (fix generation + re-simulation)")
}
