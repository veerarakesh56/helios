//! `verify` re-runs [`crate::simulate()`] against a fix-patched graph and diffs
//! the failure chains. Used by the `helios verify` CLI command.

use std::collections::BTreeSet;

use helios_graph::ResourceGraph;
use thiserror::Error;

use crate::{apply_fix, simulate, FailureChain, FixError, FixProposal, Scenario, SimulateError};

/// What [`verify`] reports: pre-fix chain, post-fix chain, and the diff.
#[derive(Debug, Clone)]
pub struct VerifyReport {
    pub pre_fix: FailureChain,
    pub post_fix: FailureChain,
    /// Resource ids that failed pre-fix but survive post-fix.
    pub resolved: Vec<String>,
    /// Resource ids that the fix newly broke.
    pub new_failures: Vec<String>,
    /// Resource ids that still fail after the fix.
    pub remaining: Vec<String>,
}

impl VerifyReport {
    /// `true` iff no resource fails under the post-fix simulation.
    pub fn is_safe(&self) -> bool {
        self.post_fix.is_safe()
    }
}

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("apply fix: {0}")]
    Apply(#[from] FixError),
    #[error("simulate: {0}")]
    Simulate(#[from] SimulateError),
}

/// Run [`simulate()`] on `graph` before and after applying `fix`, then diff
/// the failure id sets into a [`VerifyReport`].
pub fn verify(
    graph: &ResourceGraph,
    scenario: &Scenario,
    fix: &FixProposal,
) -> Result<VerifyReport, VerifyError> {
    let pre_fix = simulate(graph, scenario)?;
    let patched = apply_fix(graph, fix)?;
    let post_fix = simulate(&patched, scenario)?;

    let pre_ids: BTreeSet<String> = pre_fix.failures.iter().map(|f| f.id.clone()).collect();
    let post_ids: BTreeSet<String> = post_fix.failures.iter().map(|f| f.id.clone()).collect();

    Ok(VerifyReport {
        resolved: pre_ids.difference(&post_ids).cloned().collect(),
        new_failures: post_ids.difference(&pre_ids).cloned().collect(),
        remaining: pre_ids.intersection(&post_ids).cloned().collect(),
        pre_fix,
        post_fix,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FixEdit, FixProposal, ScenarioKind};
    use helios_graph::from_json;

    const FIXTURE: &str = include_str!("../../../fixtures/three-tier-webapp/terraform-show.json");

    fn az_1a_scenario() -> Scenario {
        Scenario {
            name: "lose-us-east-1a".into(),
            kind: ScenarioKind::AzOutage {
                az: "us-east-1a".into(),
            },
        }
    }

    #[test]
    fn verify_moves_elasticache_out_of_failing_az() {
        // Pre-fix: az-outage 1a kills subnet.public_a, instance.web, elasticache.cache.
        // Fix: move elasticache to 1b. Post-fix: elasticache is resolved; subnet+instance remain.
        let graph = from_json(FIXTURE).unwrap();
        let scenario = az_1a_scenario();
        let fix = FixProposal {
            scenario_name: scenario.name.clone(),
            explanation: "move cache to 1b".into(),
            edits: vec![FixEdit::SetAttr {
                resource_id: "aws_elasticache_cluster.cache".into(),
                key: "availability_zone".into(),
                value: serde_json::json!("us-east-1b"),
            }],
        };

        let report = verify(&graph, &scenario, &fix).unwrap();

        assert!(
            report
                .resolved
                .contains(&"aws_elasticache_cluster.cache".to_string()),
            "cache should be resolved; got resolved={:?}",
            report.resolved
        );
        assert!(
            report.new_failures.is_empty(),
            "fix must not introduce new failures; got {:?}",
            report.new_failures
        );
        assert!(
            report
                .remaining
                .contains(&"aws_subnet.public_a".to_string()),
            "subnet.public_a still fails; got remaining={:?}",
            report.remaining
        );
    }

    #[test]
    fn verify_noop_fix_preserves_failures() {
        // Setting an irrelevant attr doesn't change the simulation outcome.
        let graph = from_json(FIXTURE).unwrap();
        let scenario = az_1a_scenario();
        let fix = FixProposal {
            scenario_name: scenario.name.clone(),
            explanation: "cosmetic".into(),
            edits: vec![FixEdit::SetAttr {
                resource_id: "aws_s3_bucket.assets".into(),
                key: "versioning_enabled".into(),
                value: serde_json::json!(true),
            }],
        };

        let report = verify(&graph, &scenario, &fix).unwrap();
        assert_eq!(report.resolved, Vec::<String>::new());
        assert_eq!(report.new_failures, Vec::<String>::new());
        assert_eq!(
            report.pre_fix.failures.len(),
            report.post_fix.failures.len()
        );
    }

    #[test]
    fn verify_propagates_unknown_resource_error_from_apply() {
        let graph = from_json(FIXTURE).unwrap();
        let scenario = az_1a_scenario();
        let fix = FixProposal {
            scenario_name: scenario.name.clone(),
            explanation: "bad".into(),
            edits: vec![FixEdit::SetAttr {
                resource_id: "aws_nope.ghost".into(),
                key: "foo".into(),
                value: serde_json::json!(1),
            }],
        };
        assert!(matches!(
            verify(&graph, &scenario, &fix),
            Err(VerifyError::Apply(FixError::UnknownResource(_)))
        ));
    }
}
