# Helios

> Deterministic failure simulation for cloud infrastructure.
> Proves exactly which services break under declared failure scenarios — *before* `terraform apply`.

**Status:** pre-v0.1. Active development. Not yet usable.

## What it does

Point Helios at your Terraform code. Describe a failure as YAML (e.g. *"us-east-1 loses one AZ for 45 minutes"*). Helios parses everything into a typed resource graph, uses Z3 to symbolically execute the failure through the graph, and returns the exact failure chains with auto-generated Terraform fixes that it has **proved** resolve the failure (by re-simulating).

## Architecture at a glance

- **Rust + Z3** engine — correctness is non-negotiable, so verdicts come from an SMT solver.
- **Python + Claude** shell — narration, fix proposals, natural-language scenario parsing. The shell never decides what's safe; it only makes rigorous results human-readable.

See [`ARCHITECTURE.md`](./ARCHITECTURE.md) (coming Weekend 2) and [`docs/ai-boundary.md`](./docs/ai-boundary.md) (coming Weekend 6).

## Roadmap (v0.1)

| Weekend | Milestone |
|---|---|
| 1 | Cargo workspace, graph builder, 8 AWS resource types |
| 2 | Z3 engine, region + AZ outage scenarios |
| 3 | Claude-powered explanation layer with prompt caching |
| 4 | Claude-proposed Terraform fixes, engine-verified |
| 5 | GitHub Action + cytoscape.js web UI |
| 6 | Demo GIF, docs, v0.1.0 release |

## License

Apache-2.0. See [`LICENSE`](./LICENSE).
