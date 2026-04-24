"""Static reference text describing how Helios models AWS resource availability.

Included verbatim in every explain() prompt, cache-marked. The text must
stay aligned with the `AvailabilityModel` enum in
`helios/crates/helios-models/src/lib.rs` — when new variants or
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
