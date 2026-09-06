# Changelog

All notable changes to Helios are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.2] - 2026-09-06

Three correctness bugs, all found by generating synthetic `terraform show -json` documents and
**running** them — not by reading the code. None of them affect the shipped fixture, which is exactly
why they survived: every existing test uses that one document.

### Fixed

- ⛔ **A multi-AZ resource with no `availability_zones` was reported down in EVERY scenario.**
  `azs_from_subnets` returns an empty vec when the attribute is absent, and the encoder built
  `Bool::and(&[])` — Z3's *empty conjunction*, which is `true`. So "all of its zones are down" was
  vacuously satisfied. An `aws_lb` with no declared zones failed even under a scenario naming a zone
  nothing lives in, and its reason string read `multi-AZ across []`. An unknown AZ set now
  contributes nothing (the resource still falls with its region) and the run warns that the
  resource's AZ behaviour is not modelled.
- ⛔ **A data source of a modelled type was ingested as infrastructure.** `RawResource` never read
  Terraform's `mode` field, so `data.aws_subnet.selected` became a subnet and was reported as a
  failed service — a false positive about infrastructure Terraform does not even own. `mode` is now
  read (defaulting to `managed`, so hand-written fixtures stay valid) and anything not managed is
  skipped with a warning.
- ⛔ **Diagnostics went to stdout, which corrupted the JSON the GitHub Action parses.**
  `tracing_subscriber::fmt()` defaults to stdout; `action/scripts/run-scenarios.sh` redirects the
  command's stdout into the document that `build-comment.sh` reads with `jq`. One skipped resource
  would therefore break the PR comment on any real repository. Logs now go to stderr.
- The `IamRevocation` doc comment still listed only `iam_role_arn` and `role_arn`; v0.1.1 added
  `role`.

### Changed

- **The default Claude model id is now `claude-opus-5`** (was `claude-opus-4-7`). v0.1.1 made the id
  configurable via `HELIOS_AI_MODEL` but shipped a default that was already behind the current model
  family, while this author's other project was on `claude-sonnet-5` — the mechanism was fixed and
  the value was left stale. Making a stale value overridable is not the same as updating it.
- **The README no longer claims the GitHub Action "gates" pull requests** (it reports), nor that the
  Python shell does "natural-language scenario parsing" (it has two subcommands and no scenario
  module; YAML is parsed in Rust).

### Testing

- **83 tests** (63 Rust, 20 Python) plus 6 for the viewer. Two new regression tests: one asserts a
  zone-less multi-AZ resource does **not** fail an AZ outage while the subnet actually in the dead
  zone still does; one asserts a data source never enters the graph.

## [0.1.1] - 2026-09-06

Correctness release. Every item below was found by running the shipped fixture
through every shipped scenario and reading the output, not the code.

### Fixed

- **The SMT encoding was under-constrained.** A scenario pinned only the zone
  or region it named; every other `az_down_*` / `region_down_*` variable was
  free, so the other zones' survival was Z3's default assignment for a free
  Boolean rather than a property of the encoding. Every unnamed AZ, region and
  forced variable is now pinned `false`, and a test asserts the other zone
  *cannot* be chosen down (`Unsat`).
- **Forcing a resource down took its whole zone with it.** `slow-rds-failover`
  and `single-nat-death` forced the target's `down` Boolean, which is *defined*
  by its availability rule, so the solver satisfied it by taking the AZ (or both
  AZs of a multi-AZ database) down — six failures for one slow failover, and an
  unrelated cache failing on a NAT death. Each resource now has a dedicated
  `forced_*` variable, and `down ⇔ availability ∨ forced ∨ (any Contains-parent
  down)` is the definition, so only the target and what it contains fail.
- **`iam-revocation` never matched the shipped fixture.** The matcher read
  `iam_role_arn` / `role_arn`; `aws_lambda_function` carries its principal under
  `role`, and the fixture's Terraform JSON did not include the attribute at all,
  so the bundled scenario reported "resilient" for the wrong reason. The matcher
  now reads `role` too, the fixture carries the Lambda's role as `main.tf`
  declares it, and the scenario names it.
- **`helios inspect` leaked raw attributes.** The document is uploaded as a CI
  artifact; a real `terraform show -json` carries plaintext passwords and keys.
  Attribute values whose name contains `password`, `passwd`, `secret`, `token`,
  `private_key`, `access_key`, `credential` or `api_key` are now replaced with
  `<redacted>` (keys kept, nested objects included).
- **The Claude model id was hard-coded** in two files. `HELIOS_AI_MODEL` now
  overrides the default without a code change. (The default value itself was
  brought current after this release — see Unreleased.)
- Workspace and package versions were still `0.0.1` under a `v0.1.0` release;
  both now carry the release version.

### Changed

- Reason strings: under `iam-revocation`, only the matched resource reads
  "principal … was revoked"; resources that fail through containment read
  "failure propagated from a dependency".
- Rust tests 56 → 61, Python tests 18 → 20.

## [0.1.0] - 2026-04-25

First public release. Eight AWS resource kinds, five scenario kinds, a
deterministic Z3-backed simulator, a Claude-powered explanation and
fix-proposal shell, a composite GitHub Action, and a cytoscape.js web
viewer.

### Added

#### Engine and graph
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

#### AI shell and structured fixes
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

#### Action, viewer, and combined inspect
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

#### Docs, release scaffolding, and demo
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

[0.1.1]: https://github.com/veerarakesh56/helios/releases/tag/v0.1.1
[0.1.0]: https://github.com/veerarakesh56/helios/releases/tag/v0.1.0
