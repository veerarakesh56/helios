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

### `iam-revocation`

```yaml
kind:
  type: iam-revocation
  principal_arn: arn:aws:iam::123456789012:role/lambda-worker
```

Fails any resource whose `attrs.iam_role_arn`, `attrs.role_arn` or `attrs.role`
(the attribute `aws_lambda_function` actually uses) matches the principal, plus
everything those resources contain. v0.1 is a string match over the
Terraform-JSON attr set; modelling IAM as graph nodes so multi-hop policy chains
propagate is future work.

### `slow-rds-failover`

```yaml
kind:
  type: slow-rds-failover
  db_id: aws_db_instance.primary
```

Models a multi-AZ RDS whose failover takes longer than expected: during the
window the DB is treated as unavailable and dependents inherit the failure via
`Contains` edges. The DB is forced down *directly* — its availability zones stay
up, so nothing else in those zones is affected. (Before 0.1.1 the DB was forced
down through its availability rule, which made the solver take both zones down.)

### `single-nat-death`

```yaml
kind:
  type: single-nat-death
  subnet_id: aws_subnet.public_a
```

Treats the subnet as having lost egress. Every resource inside it (`Contains`
edges) fails; the subnet's availability zone itself stays up, so unrelated
resources in the same zone survive. NAT itself is not yet a graph node.

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
