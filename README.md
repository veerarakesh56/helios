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

## Weekend 2 — Engine v0 ✅

- Z3-backed SMT encoding for `region-outage` + `az-outage` scenarios.
- First E2E: `helios simulate fixtures/three-tier-webapp --scenario fixtures/scenarios/az-outage.yaml` prints the failure chain.
- Scenario schema documented in [`docs/scenarios.md`](docs/scenarios.md).

## Weekend 3 — Claude explain layer ✅

- `helios simulate ... --json` emits the `FailureChain` as JSON on stdout.
- `helios-ai/` is a uv-managed Python package that reads that JSON on stdin and writes a human-readable markdown narrative on stdout via Claude, with prompt caching on the system prompt + availability-model glossary.
- `helios explain` shells out to `python -m helios_ai explain` so the user can pipe end-to-end:

  ```bash
  helios simulate ./infra --scenario scenarios/az-outage.yaml --json | helios explain
  ```

  Set `ANTHROPIC_API_KEY` in the environment, and set `HELIOS_AI_PYTHON` to point at the Python interpreter that has `helios_ai` installed (e.g. `helios-ai/.venv/bin/python`) if it isn't on `PATH`.

- See [`helios-ai/`](helios-ai/) for the Python side and [`docs/ai-boundary.md`](docs/ai-boundary.md) (Weekend 6) for why Claude only narrates and never decides.

## License

Apache-2.0. See [`LICENSE`](./LICENSE).
