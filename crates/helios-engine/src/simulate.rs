//! Top-level entry point the CLI uses.

use helios_graph::ResourceGraph;
use thiserror::Error;
use z3::{SatResult, Solver};

use crate::report::FailureChain;
use crate::scenario::Scenario;
use crate::smt::Encoder;

#[derive(Debug, Error)]
pub enum SimulateError {
    #[error("solver returned unsat — this should not happen for a well-formed scenario")]
    Unsat,
    #[error("solver returned unknown — Z3 timed out or hit a resource limit")]
    Unknown,
}

/// Run one scenario against one graph. Returns the failure chain.
pub fn simulate(
    graph: &ResourceGraph,
    scenario: &Scenario,
) -> Result<FailureChain, SimulateError> {
    let solver = Solver::new();
    let mut enc = Encoder::new();
    enc.encode_availability(graph, &solver);
    enc.encode_dependencies(graph, &solver);
    enc.apply_scenario(scenario, &solver);

    match solver.check() {
        SatResult::Sat => Ok(enc.extract_failures(graph, scenario, &solver)),
        SatResult::Unsat => Err(SimulateError::Unsat),
        SatResult::Unknown => Err(SimulateError::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Scenario, ScenarioKind};
    use helios_graph::from_json;

    const FIXTURE: &str =
        include_str!("../../../fixtures/three-tier-webapp/terraform-show.json");

    #[test]
    fn simulate_az_outage_returns_failure_chain() {
        let graph = from_json(FIXTURE).unwrap();
        let scenario = Scenario {
            name: "lose-us-east-1a".into(),
            kind: ScenarioKind::AzOutage {
                az: "us-east-1a".into(),
            },
        };
        let chain = simulate(&graph, &scenario).unwrap();
        assert!(!chain.is_safe());
        assert_eq!(chain.scenario, "lose-us-east-1a");
    }
}
