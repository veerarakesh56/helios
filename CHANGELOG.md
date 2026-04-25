# Changelog

All notable changes to Helios are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-04-25

First public release. Eight AWS resource kinds, five scenario kinds, a
deterministic Z3-backed simulator, a Claude-powered explanation and
fix-proposal shell, a composite GitHub Action, and a cytoscape.js web
viewer.

### Added

#### Engine and graph (Weekends 1-2)
- Cargo workspace with five crates: `helios-cli`, `helios-graph`,
  `helios-models`, `helios-engine`, `helios-aws` (stub).
- `helios-graph` parses `terraform show -json` for eight AWS resource kinds:
  VPC, Subnet, EC2 instance, ALB, RDS, ElastiCache, Lambda, S3.
- `helios-models::availability_for` returns an `AvailabilityModel` per
  resource (`SingleAz`, `MultiAz`, `Regional`, `GlobalEdge`).
- `helios-engine` Z3-backed SMT encoder. One `Bool` per resource / AZ /
  region; biconditionals from the availability model; `Contains` edges
  propagate failure downward.
- Scenario kinds `RegionOutage` and `AzOutage`; `helios simulate`
  CLI entry point that prints the failure chain and exits non-zero on
  any failure.
- Pre-compiled Z3 4.16.0 via the `z3 0.20` `gh-release` feature; no
  system Z3 install needed on Linux or Windows.

#### AI shell and structured fixes (Weekends 3-4)
- `helios-ai/` uv-managed Python 3.12 package. Pydantic models mirror the
  Rust `FailureChain`, `FailedResource`, `FixProposal`, `FixEdit` byte
  for byte (`extra="forbid"` keeps schema drift loud).
- `helios-ai explain` -- reads `FailureChain` JSON on stdin, returns a
  markdown narrative on stdout via Claude with prompt caching on the
  system prompt and the availability-model glossary.
- `helios-ai propose-fix` -- reads `{chain, attrs_snapshot}` on stdin,
  returns a structured `FixProposal` via Claude `output_config.format`
  with the same two cache breakpoints reused.
- `MockAnthropic` (`HELIOS_AI_MOCK=1`) for offline tests; ASCII-only
  output so Windows cp1252 stdout does not mangle JSON.
- New scenario kinds: `IamRevocation`, `SlowRdsFailover`, `SingleNatDeath`.
- `helios verify <tf-json> --scenario <yaml> --fix <json>` -- engine
  re-runs the simulation with the fix applied and reports
  `Resolved` / `Still failing` / `New failures introduced`. Exits non-zero
  if any failure remains.
- Structured `set_attr` edit op (only edit op in v0.1).

#### Action, viewer, and combined inspect (Weekend 5)
- `helios inspect <tf-json> --scenario <yaml>` emits a single JSON
  document `{scenario, graph: {nodes, edges}, chain}`. Hand-rolled flat
  graph shape (not petgraph's native serde, whose `NodeIndex` integers
  are unstable across builds and meaningless to a viewer).
- Composite GitHub Action at `action/`. `actions/cache@v4` keys on
  `Cargo.lock` plus crate sources; cache miss builds and copies the
  `helios` binary. Loops every scenario in `fixtures/scenarios/*.yaml`,
  optionally runs `verify` if `fixes/<stem>.json` exists, uploads
  per-scenario artifacts, upserts a single sticky PR comment marked with
  `<!-- helios-action -->`.
- Vite + React + TypeScript + cytoscape 3.30 web viewer at `web/`.
  File-picker, paste-textarea, and "Load sample" entry points.
- Three-layer schema mirror: Rust source of truth in
  `helios-engine::inspect`, Pydantic mirror in
  `helios-ai/src/helios_ai/models.py`, TypeScript mirror in
  `web/src/types.ts`.

#### Docs, release scaffolding, and demo (Weekend 6)
- `docs/ai-boundary.md` -- canonical "AI never produces a safety verdict"
  essay with the differential-testing rationale and a Cedar prior-art
  reference.
- `docs/ARCHITECTURE.md` -- per-crate map, end-to-end data flow, and
  rationale for every locked design choice.
- `Makefile` -- `make demo`, `make test`, `make fmt`, `make check`.
  `make demo` runs the spec choreography with `HELIOS_AI_MOCK=1`.
- `docs/demo.gif` -- animated demo embedded at the top of the README.
- `CONTRIBUTING.md` and `.github/ISSUE_TEMPLATE/{bug,feature}.yml`.
- `launch/` -- Show-HN draft, SRECon CFP abstract, outreach DM templates.

### Changed

- `simulate` reports the failure chain in a stable plain renderer; JSON
  emission via `--json` for downstream tooling.
- CI matrix grew from one `check` job to four jobs: `check`,
  `python-check`, `web-check`, `action-smoke`.
- `.gitattributes` pins LF on `*.sh`, `*.yml`, `*.yaml` so Windows-side
  edits round-trip on Linux runners.

### Fixed

- `helios-graph` rustdoc: bare URL wrapped in angle brackets to satisfy
  `rustdoc -D warnings`.

## Pre-0.1.0

Internal-only. The public history starts with this release.

[0.1.0]: https://github.com/veerarakesh56/helios/releases/tag/v0.1.0
