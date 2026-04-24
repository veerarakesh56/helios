# Scenario YAML schema

A scenario is a declarative failure the engine simulates against a resource graph.

## Top-level

```yaml
name: <string>       # human id for the scenario, echoed in the report
kind: <ScenarioKind> # the failure itself (see below)
```

## Kinds supported in v0.1

### `az-outage`

```yaml
kind:
  type: az-outage
  az: us-east-1a
```

Takes a single availability zone offline. Regional and multi-AZ resources survive.

### `region-outage`

```yaml
kind:
  type: region-outage
  region: us-east-1
```

Takes an entire region offline. Only `GlobalEdge` resources survive.

## Kinds coming in Weekend 4

- `iam-revocation` — revoke a principal's permissions
- `slow-rds-failover` — RDS failover exceeds its SLO window
- `single-nat-death` — the single NAT gateway in an AZ dies

Add a new scenario by creating a YAML file in `fixtures/scenarios/` and running:

```bash
helios simulate <tf-dir> --scenario fixtures/scenarios/<name>.yaml
```

## JSON output

Pass `--json` to emit the `FailureChain` as JSON on stdout instead of the default pretty text:

```bash
helios simulate <tf-dir> --scenario fixtures/scenarios/<name>.yaml --json
```

Shape:

```json
{
  "scenario": "lose-us-east-1a",
  "failures": [
    { "id": "aws_instance.web", "kind": "Instance", "reason": "single-AZ in us-east-1a, which is down" }
  ]
}
```

Authoritative schema: `helios_engine::report::{FailureChain, FailedResource}` in [`crates/helios-engine/src/report.rs`](../crates/helios-engine/src/report.rs). Consumed by [`helios-ai`](../helios-ai/) to produce human-readable narratives.
