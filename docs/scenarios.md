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
