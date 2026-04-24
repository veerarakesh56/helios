"""Static reference text describing how Helios models AWS resource availability
and the scenarios the engine supports.

Included verbatim in every explain() and propose_fix() prompt, cache-marked.
The text must stay aligned with the `AvailabilityModel` enum in
`helios/crates/helios-models/src/lib.rs` and the `ScenarioKind` enum in
`helios/crates/helios-engine/src/scenario.rs` — when new variants or
failover semantics are added, update here in the same PR.
"""

AVAILABILITY_MODEL_GLOSSARY = """\
# Helios Availability Model Glossary

Every AWS resource in a Helios graph carries an `AvailabilityModel` that
tells the SMT engine what it takes to make that resource unavailable.
There are four variants.

## SingleAz { az }

Lives in exactly one availability zone. Unavailable iff that AZ is
unavailable. Example kinds: aws_subnet (single-AZ), aws_instance,
aws_elasticache_cluster (non-replicated).

## MultiAz { azs, failover_seconds }

Spans two or more AZs. Available as long as at least one of its AZs is
available. `failover_seconds` is the expected window during which a
failover is visible to clients; for the purposes of SMT availability
this is treated as "temporarily unavailable" during an AZ loss but
recovers within the window. Example kinds: aws_db_instance with
multi-AZ standby, aws_lb spanning subnets in 2+ AZs.

## Regional { region }

Control-plane resource scoped to a region, not a specific AZ. Available
iff the region is available. Note: a "region outage" scenario takes out
every Regional resource in that region, even if individually each AZ
might still be up. Example kinds: aws_lambda_function, aws_vpc,
aws_db_instance (primary region view).

## GlobalEdge

Edge / global resource. Treated as always available for current
scenario semantics — not affected by region or AZ outages. Example
kinds: aws_s3_bucket (global namespace), CloudFront (future).

## Propagation rules

- `Contains` edges propagate availability: if a resource's container
  fails, the contained resource fails. (Example: subnet fails → every
  instance in that subnet fails.)
- `MemberOf` edges do NOT propagate (over-constrains Regional
  resources like Lambda-in-VPC).

## Scenario kinds (v0.1)

- **az-outage** { az }: a single availability zone is offline. SingleAz
  resources in that AZ fail; MultiAz resources survive if any of their AZs
  is still up; Regional/GlobalEdge unaffected.
- **region-outage** { region }: an entire region offline. Only GlobalEdge
  resources survive.
- **iam-revocation** { principal_arn }: a role/principal is revoked.
  v0.1 is a string match on `attrs.iam_role_arn` or `attrs.role_arn`; any
  resource naming that principal fails (dependents cascade via Contains).
- **slow-rds-failover** { db_id }: a multi-AZ RDS's failover exceeds its
  SLO window. The target DB is treated as unavailable; dependents fail.
- **single-nat-death** { subnet_id }: the subnet's NAT gateway dies.
  Subnet loses egress; every instance inside it fails.

When proposing fixes, propose the minimal set of `set_attr` edits that
resolve the failure chain while preserving AWS semantics (e.g. enabling
`multi_az` on an RDS, moving a SingleAz service to a different AZ, or
widening an ALB's `availability_zones` list).

## Reading a FailureChain

A `FailureChain` has a scenario name and a list of failed resources.
Each failed resource has:

- `id`: the Terraform address (e.g. `aws_instance.web`)
- `kind`: the AWS kind (e.g. `Instance`, `DbInstance`, `ElasticacheCluster`)
- `reason`: a short human-readable reason from the SMT counter-example

When narrating, tie each failure back to the scenario's root cause via
the Contains chain. Avoid restating internal model semantics unless the
user asks. Describe observable impact, not SMT mechanics.
"""
