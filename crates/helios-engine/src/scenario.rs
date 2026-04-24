//! Scenario = a declarative failure to simulate. Weekend 2 supports two kinds.

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

/// A failure scenario to apply to the resource graph.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Scenario {
    /// Human-friendly scenario id (e.g. "az-1a-outage").
    pub name: String,
    /// What's failing.
    pub kind: ScenarioKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ScenarioKind {
    /// An availability zone goes dark.
    AzOutage { az: String },
    /// An entire AWS region goes dark.
    RegionOutage { region: String },
    /// An IAM principal (role ARN) is revoked. Resources whose attrs name
    /// that principal (`iam_role_arn` or `role_arn`) lose access and fail.
    /// v0.1 is a string match — v0.2 models IAM in the graph directly.
    IamRevocation { principal_arn: String },
    /// A multi-AZ RDS's failover window stretches past SLO — during that
    /// window the DB is unreachable. Modeled as forcing that specific
    /// resource down; dependents propagate via Contains edges.
    SlowRdsFailover { db_id: String },
    /// A single NAT gateway dies. Modeled via the subnet it sits in: the
    /// subnet loses egress, which knocks out every instance inside it.
    SingleNatDeath { subnet_id: String },
}

#[derive(Debug, Error)]
pub enum ScenarioError {
    #[error("read scenario file {path:?}: {source}")]
    Read {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse scenario YAML: {0}")]
    Parse(#[from] serde_yaml_ng::Error),
}

/// Load a scenario from a YAML file on disk.
pub fn load(path: &Path) -> Result<Scenario, ScenarioError> {
    let raw = std::fs::read_to_string(path).map_err(|source| ScenarioError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(serde_yaml_ng::from_str(&raw)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_az_outage() {
        let yaml = "name: lose-1a\nkind:\n  type: az-outage\n  az: us-east-1a\n";
        let s: Scenario = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(s.name, "lose-1a");
        assert_eq!(
            s.kind,
            ScenarioKind::AzOutage {
                az: "us-east-1a".into()
            }
        );
    }

    #[test]
    fn parses_region_outage() {
        let yaml = "name: useast1-down\nkind:\n  type: region-outage\n  region: us-east-1\n";
        let s: Scenario = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(
            s.kind,
            ScenarioKind::RegionOutage {
                region: "us-east-1".into()
            }
        );
    }

    #[test]
    fn unknown_kind_errors() {
        let yaml = "name: x\nkind:\n  type: solar-flare\n  radius: 5000\n";
        let res: Result<Scenario, _> = serde_yaml_ng::from_str(yaml);
        assert!(res.is_err());
    }

    #[test]
    fn parses_iam_revocation() {
        let yaml = "name: revoke-web\nkind:\n  type: iam-revocation\n  principal_arn: arn:aws:iam::1:role/web\n";
        let s: Scenario = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(s.kind, ScenarioKind::IamRevocation { .. }));
    }

    #[test]
    fn parses_slow_rds_failover() {
        let yaml = "name: rds-slow\nkind:\n  type: slow-rds-failover\n  db_id: aws_db_instance.primary\n";
        let s: Scenario = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(
            s.kind,
            ScenarioKind::SlowRdsFailover {
                db_id: "aws_db_instance.primary".into()
            }
        );
    }

    #[test]
    fn parses_single_nat_death() {
        let yaml = "name: nat-dead\nkind:\n  type: single-nat-death\n  subnet_id: aws_subnet.public_a\n";
        let s: Scenario = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(
            s.kind,
            ScenarioKind::SingleNatDeath {
                subnet_id: "aws_subnet.public_a".into()
            }
        );
    }

    #[test]
    fn loads_az_outage_fixture_from_disk() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/scenarios/az-outage.yaml");
        let s = load(&path).unwrap();
        assert_eq!(s.name, "lose-us-east-1a");
        assert!(matches!(s.kind, ScenarioKind::AzOutage { .. }));
    }
}
