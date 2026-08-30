# Helios

[![CI](https://github.com/veerarakesh56/helios/actions/workflows/ci.yml/badge.svg)](https://github.com/veerarakesh56/helios/actions/workflows/ci.yml)
[![Rust 1.75+](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

> Deterministic failure simulation for cloud infrastructure.
> Proves exactly which services break under a declared failure — *before* `terraform apply`.

![demo](docs/demo.gif)

**Status:** v0.1.0 — working and tested. 8 AWS resource kinds, 5 failure scenarios, a GitHub Action
that gates pull requests, and a web viewer. **74 tests (56 Rust, 18 Python), CI green.**

| | |
|---|---|
| **Engine** | Rust + **Z3 SMT** — every verdict is solved for, never estimated |
| **AI shell** | Python + Claude — narrates counter-examples and proposes Terraform fixes |
| **The boundary** | the model never produces a verdict; **every AI-proposed fix is re-simulated by the engine before it counts** |
| **Input** | `terraform show -json` output |
| **Scenarios** | AZ outage · region outage · IAM revocation · single-NAT death · slow RDS failover |
| **Resources** | VPC · Subnet · Instance · Load balancer · RDS instance · ElastiCache cluster · Lambda · S3 |
| **CI** | a composite GitHub Action posts one sticky PR comment with the verdict per scenario |

## The problem

You cannot test an availability-zone outage. You can reason about one, draw it on a whiteboard, and
be confident — and confidence is exactly the thing that fails at 3 a.m. The infrastructure that broke
was usually reviewed by someone competent who traced the dependency chain in their head and missed
one edge.

Asking an LLM instead does not fix it. Given a Terraform file and *"what breaks if we lose an AZ?"*, a
model will always produce a confident, plausible answer. Plausible is not the same as correct, and in
availability work the difference only shows up during an incident.

**Helios does not reason about the failure. It solves for it.** The graph and the scenario become an
SMT problem, Z3 executes the failure symbolically, and what comes back is a proof, not an opinion.

## What it does

```
terraform show -json  ─┐
                       ├─►  typed resource graph  ─►  Z3  ─►  failure chain
scenario.yaml         ─┘                                          │
                                                                  ▼
                                              Claude narrates it, proposes a fix
                                                                  │
                                                                  ▼
                                              engine RE-SIMULATES with the fix applied
                                              Resolved / Still failing / New failures introduced
```

The last step is the point. A fix the model suggests is not trusted because it sounds right — it is
applied to a clone of the graph, re-solved, and reported as `Resolved`, `Still failing` or
`New failures introduced`, exiting non-zero if anything still fails.

## Quickstart

```bash
git clone https://github.com/veerarakesh56/helios && cd helios
make demo
```

`make demo` simulates an AZ outage against the bundled three-tier webapp fixture, narrates the
failure chain through the AI shell, applies a structured fix proposal, and re-verifies.
Set `HELIOS_AI_MOCK=1` for a no-network run.

For a real run, set `ANTHROPIC_API_KEY` and point `HELIOS_AI_PYTHON` at the interpreter that has
`helios_ai` installed (e.g. `helios-ai/.venv/bin/python`).

## Commands

```bash
helios plan     <tf-json-dir>                             # resource and edge counts
helios simulate <tf-json> --scenario <yaml> [--json]      # run the engine, print the failure chain
helios explain  < chain.json                              # Claude narrates it, via the Python shell
helios verify   <tf-json> --scenario <yaml> --fix <json>  # re-simulate with the fix, diff the result
helios inspect  <tf-json> --scenario <yaml>               # {scenario, graph, chain} for viewer/Action
```

Pipe them:

```bash
helios simulate ./infra --scenario scenarios/az-outage.yaml --json | helios explain
```

## Scenarios

Five kinds ship in `fixtures/scenarios/`, each a small YAML document (schema in
[`docs/scenarios.md`](docs/scenarios.md)):

| Scenario | Asks |
|---|---|
| `az-outage` | one availability zone disappears for a stated duration |
| `region-outage` | an entire region goes |
| `iam-revocation` | a role or policy is pulled |
| `single-nat-death` | the one NAT gateway everything egresses through dies |
| `slow-rds-failover` | the database fails over, but not quickly |

Adding a scenario kind or an AWS resource type is the easiest first contribution — see
[`CONTRIBUTING.md`](./CONTRIBUTING.md).

## Fix generation, and why it is verified

`helios-ai propose-fix` reads `{chain, attrs_snapshot}` on stdin and returns a structured
`FixProposal` (`{scenario_name, explanation, edits[]}`) from Claude using `output_config.format` and
a two-breakpoint prompt cache — so the *shape* of a fix is enforced by the type, not by the prompt.

`helios verify` then applies those edits to a clone of the graph and re-runs the solver. It prints
`Pre-fix failures` / `Post-fix failures` and the three sections that matter — `Resolved`,
`Still failing`, `New failures introduced` — exiting non-zero if anything still fails.

**A fix that introduces a new failure is caught by the engine, not by a reviewer.**

## GitHub Action

[`action/`](./action) is a composite action. It runs every scenario in `fixtures/scenarios/*.yaml`,
optionally re-runs `helios verify` when a matching `fixes/<scenario>.json` is committed, uploads each
`inspect` JSON as a workflow artifact, and posts **one sticky PR comment** summarising the verdict —
a collapsible `<details>` per scenario. It caches the prebuilt binary on `Cargo.lock` + crate
sources, so the second run on a PR is fast.

```yaml
on: pull_request
permissions:
  contents: read
  pull-requests: write
jobs:
  helios:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: veerarakesh56/helios/action@v0.1.0
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

## Web viewer

[`web/`](./web) is a Vite + React + cytoscape.js single-page app. `npm run dev` for local dev,
`npm run build` for a static bundle. Drop a `helios inspect` JSON into the file picker: failed
resources render red, `Contains` edges thick and solid, `MemberOf` edges thin and dashed. Click any
node for its Terraform attributes and the reason it failed.

## Architecture

- **Rust + Z3 engine** — correctness is non-negotiable, so verdicts come from an SMT solver. Reads
  `terraform show -json` into a `petgraph::DiGraph`, encodes the scenario as constraints, and solves.
- **Python + Claude shell** — narration, fix proposals, natural-language scenario parsing.
  **The shell never decides what is safe.** It makes rigorous results readable.

[`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md) is the deep dive;
[`docs/ai-boundary.md`](./docs/ai-boundary.md) explains why the AI shell never produces a verdict.

## Tests

**74 tests, all green in CI:**

| | |
|---|---|
| Rust | **56** across the graph, engine, SMT encoding, verify loop and CLI |
| Python | **18** in `helios-ai/`, including syrupy snapshots of the model output |

CI runs `cargo test`, `cargo clippy`, `cargo fmt --check` and the Python suite on every push.

## Limits, stated plainly

- **AWS only**, and only the 8 resource kinds above. A resource Helios does not model is absent from
  the graph — it is not assumed healthy, it simply is not there.
- **It reads `terraform show -json`, not live AWS.** `crates/helios-aws` is a stub: live-state
  collection for drift detection is designed but **not implemented**, and its SDK dependencies are
  commented out. Nothing in Helios talks to an AWS account.
- **Availability models are approximations.** Multi-AZ RDS is modelled as surviving one AZ; a real
  failover takes time, and `slow-rds-failover` exists precisely because that assumption is the
  interesting one to break.
- **Five scenario kinds** is not the space of real outages. It covers ones that recur.
- **The AI shell needs an API key** for real narration. The engine does not — simulation and
  verification run entirely offline, and `HELIOS_AI_MOCK=1` exercises the whole pipeline with no
  network at all.

## Related

**[WARDEN](https://github.com/veerarakesh56/warden)** — the same principle applied to live incident
response: the model proposes, a deterministic verifier decides, nothing executes against
infrastructure.

## How it was built

Six weekends, in order: cargo workspace and the graph builder · the Z3 engine and the first two
scenarios · the Claude explain layer with prompt caching · engine-verified fix generation · the
GitHub Action and the web viewer · docs and the v0.1.0 release.

## License

Apache-2.0. See [`LICENSE`](./LICENSE).
