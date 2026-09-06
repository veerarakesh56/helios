//! Z3 encoder. Covers all five scenario kinds.
//!
//! Every resource's `down` Boolean is *defined* — `down ⇔ availability-condition ∨ forced ∨
//! (any Contains-parent down)` — and a scenario then pins every AZ, region and `forced`
//! variable it does not name to `false`. The model is therefore unique for a scenario, not a
//! solver default. (v0.1.0 forced a targeted resource down *through* its availability rule, so
//! Z3 satisfied "the subnet is down" by taking its whole AZ down — and nothing pinned the zones
//! a scenario never mentioned. Found by reading the fixture output, fixed in 0.1.1.)

use std::collections::HashMap;

use helios_graph::{Dependency, Resource, ResourceGraph};
use helios_models::{availability_for, AvailabilityModel};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use z3::{ast::Bool, SatResult, Solver};

use crate::report::{FailedResource, FailureChain};

/// Smoke test the Z3 binding compiles and links.
#[doc(hidden)]
pub fn solver_smoke() -> SatResult {
    let solver = Solver::new();
    let a = Bool::new_const("a");
    solver.assert(&a);
    solver.check()
}

/// Extract the region name from an AZ name by stripping the trailing letter.
/// "us-east-1a" → "us-east-1". Handles any AZ suffix char.
pub(crate) fn region_of_az(az: &str) -> String {
    let bytes = az.as_bytes();
    if bytes
        .last()
        .map(|c| c.is_ascii_alphabetic())
        .unwrap_or(false)
    {
        az[..az.len() - 1].to_string()
    } else {
        az.to_string()
    }
}

/// Default region hard-coded for v0.1 (spec §6, matches `availability_for` default).
pub(crate) const DEFAULT_REGION: &str = "us-east-1";

/// SMT encoding of a resource graph. One [`Encoder`] per simulation run.
pub struct Encoder {
    /// `Bool` per resource, true ⇔ resource is down.
    pub(crate) resource_down: HashMap<NodeIndex, Bool>,
    /// `Bool` per AZ, true ⇔ that AZ is down.
    pub(crate) az_down: HashMap<String, Bool>,
    /// `Bool` per region, true ⇔ that region is down.
    pub(crate) region_down: HashMap<String, Bool>,
    /// `Bool` per resource, true ⇔ a scenario forces this resource down directly (slow RDS
    /// failover, NAT death, IAM revocation) — independently of its AZ or region.
    pub(crate) forced_down: HashMap<NodeIndex, Bool>,
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Encoder {
    pub fn new() -> Self {
        Self {
            resource_down: HashMap::new(),
            az_down: HashMap::new(),
            region_down: HashMap::new(),
            forced_down: HashMap::new(),
        }
    }

    pub(crate) fn az_var(&mut self, az: &str) -> Bool {
        self.az_down
            .entry(az.to_string())
            .or_insert_with(|| Bool::new_const(format!("az_down_{az}")))
            .clone()
    }

    pub(crate) fn region_var(&mut self, region: &str) -> Bool {
        self.region_down
            .entry(region.to_string())
            .or_insert_with(|| Bool::new_const(format!("region_down_{region}")))
            .clone()
    }

    /// For every node in the graph, declare its `resource_down` and `forced_down` Bools and
    /// assert the definition `down ⇔ availability-condition ∨ forced ∨ (any Contains-parent down)`.
    ///
    /// The parent term is part of the *definition*, not a separate implication: with a plain
    /// `parent ⇒ child` on top of a biconditional, forcing a child down could only be satisfied
    /// by making the child's own AZ or region down — which is exactly the v0.1.0 defect.
    pub fn encode_availability(&mut self, graph: &ResourceGraph, solver: &Solver) {
        // Pass 1: declare every resource's variables so parent terms can be referenced.
        for idx in graph.node_indices() {
            let r: &Resource = &graph[idx];
            self.resource_down
                .insert(idx, Bool::new_const(format!("down_{}", r.id)));
            self.forced_down
                .insert(idx, Bool::new_const(format!("forced_{}", r.id)));
        }
        // Pass 2: assert the definition.
        for idx in graph.node_indices() {
            let r: &Resource = &graph[idx];
            let model = availability_for(tf_type_of(&r.kind), &r.attrs, DEFAULT_REGION);
            let down = self.resource_down[&idx].clone();
            let cond: Bool = match model {
                AvailabilityModel::SingleAz { az } => {
                    let region = region_of_az(&az);
                    let az_v = self.az_var(&az);
                    let region_v = self.region_var(&region);
                    Bool::or(&[az_v, region_v])
                }
                AvailabilityModel::MultiAz { azs, .. } => {
                    let region = azs
                        .first()
                        .map(|a| region_of_az(a))
                        .unwrap_or_else(|| DEFAULT_REGION.to_string());
                    let region_v = self.region_var(&region);
                    let az_vars: Vec<Bool> = azs.iter().map(|a| self.az_var(a)).collect();
                    let all_azs_down = Bool::and(&az_vars);
                    Bool::or(&[all_azs_down, region_v])
                }
                AvailabilityModel::Regional { region } => self.region_var(&region),
                AvailabilityModel::GlobalEdge => Bool::from_bool(false),
            };
            let mut terms: Vec<Bool> = vec![cond, self.forced_down[&idx].clone()];
            for edge in graph.edges_directed(idx, petgraph::Direction::Outgoing) {
                if matches!(edge.weight(), Dependency::Contains(_)) {
                    terms.push(self.resource_down[&edge.target()].clone());
                }
            }
            solver.assert(down.eq(Bool::or(&terms)));
        }
    }

    /// Pin every AZ, region and `forced` variable NOT listed to `false`, so the only things
    /// down are the ones the scenario names and what follows from them. Without this the
    /// unnamed variables are free and the result depends on the solver's default assignment.
    fn pin_everything_else(
        &self,
        solver: &Solver,
        azs_named: &[String],
        regions_named: &[String],
        forced_named: &[NodeIndex],
    ) {
        for (az, v) in &self.az_down {
            if !azs_named.contains(az) {
                solver.assert(v.not());
            }
        }
        for (region, v) in &self.region_down {
            if !regions_named.contains(region) {
                solver.assert(v.not());
            }
        }
        for (idx, v) in &self.forced_down {
            if !forced_named.contains(idx) {
                solver.assert(v.not());
            }
        }
    }

    /// For every strong containment edge, assert `parent_down ⇒ child_down`.
    ///
    /// Only [`Dependency::Contains`] edges propagate — they represent hard parent/child
    /// links (subnet→vpc, instance→subnet) where the child cannot survive the parent.
    /// [`Dependency::MemberOf`] edges (alb/lambda into a set of subnets) are loose: the
    /// resource's availability model already captures the per-AZ membership, so adding
    /// an implication here would over-constrain and falsely kill Regional services like
    /// Lambda when a single member subnet fails.
    pub fn encode_dependencies(&self, graph: &ResourceGraph, solver: &Solver) {
        for edge in graph.edge_references() {
            if !matches!(edge.weight(), Dependency::Contains(_)) {
                continue;
            }
            let child_idx = edge.source();
            let parent_idx = edge.target();
            let (Some(child), Some(parent)) = (
                self.resource_down.get(&child_idx),
                self.resource_down.get(&parent_idx),
            ) else {
                continue;
            };
            solver.assert(parent.implies(child));
        }
    }

    /// Apply a [`crate::Scenario`] to the solver by forcing the right Bool(s) true.
    ///
    /// Needs `graph` so targeted-resource kinds (slow-rds-failover,
    /// single-nat-death, iam-revocation) can look up the affected nodes.
    pub fn apply_scenario(
        &mut self,
        scenario: &crate::Scenario,
        graph: &ResourceGraph,
        solver: &Solver,
    ) {
        match &scenario.kind {
            crate::ScenarioKind::AzOutage { az } => {
                let v = self.az_var(az);
                solver.assert(&v);
                // Pin the region UP so we observe the AZ effect in isolation — and every
                // OTHER zone up too, so the other zones' survival is asserted, not defaulted.
                self.pin_everything_else(solver, std::slice::from_ref(az), &[], &[]);
            }
            crate::ScenarioKind::RegionOutage { region } => {
                let v = self.region_var(region);
                solver.assert(&v);
                // A region outage takes its zones with it (semantically, and so the model reads
                // consistently); every other region and zone stays up.
                let azs_in_region: Vec<String> = self
                    .az_down
                    .keys()
                    .filter(|az| &region_of_az(az) == region)
                    .cloned()
                    .collect();
                for az in &azs_in_region {
                    solver.assert(&self.az_down[az]);
                }
                self.pin_everything_else(solver, &azs_in_region, std::slice::from_ref(region), &[]);
            }
            crate::ScenarioKind::SlowRdsFailover { db_id }
            | crate::ScenarioKind::SingleNatDeath { subnet_id: db_id } => {
                // Force that specific resource down through its `forced` variable — NOT through
                // its availability rule — so its AZ stays up and only Contains-dependents follow.
                let mut named = Vec::new();
                if let Some(idx) = graph.node_indices().find(|i| &graph[*i].id == db_id) {
                    solver.assert(&self.forced_down[&idx]);
                    named.push(idx);
                }
                self.pin_everything_else(solver, &[], &[], &named);
            }
            crate::ScenarioKind::IamRevocation { principal_arn } => {
                // String match on the attributes that carry a principal: `iam_role_arn`,
                // `role_arn`, and `role` (the attribute name aws_lambda_function actually uses).
                let mut named = Vec::new();
                for idx in graph.node_indices() {
                    if names_principal(&graph[idx], principal_arn) {
                        solver.assert(&self.forced_down[&idx]);
                        named.push(idx);
                    }
                }
                self.pin_everything_else(solver, &[], &[], &named);
            }
        }
    }

    /// After `solver.check() == Sat`, build the failure chain from the model.
    pub fn extract_failures(
        &self,
        graph: &ResourceGraph,
        scenario: &crate::Scenario,
        solver: &Solver,
    ) -> FailureChain {
        let Some(model) = solver.get_model() else {
            return FailureChain {
                scenario: scenario.name.clone(),
                failures: vec![],
            };
        };
        let mut failures = Vec::new();
        for (idx, down_bool) in &self.resource_down {
            let is_down = model
                .eval(down_bool, true)
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !is_down {
                continue;
            }
            let r: &Resource = &graph[*idx];
            let reason = reason_for(r, scenario);
            failures.push(FailedResource {
                id: r.id.clone(),
                kind: format!("{:?}", r.kind),
                reason,
            });
        }
        failures.sort_by(|a, b| a.id.cmp(&b.id));
        FailureChain {
            scenario: scenario.name.clone(),
            failures,
        }
    }
}

/// Short per-resource explanation for why this scenario takes it down.
/// Purely derived from the availability model + scenario kind (no Z3 needed).
fn reason_for(r: &Resource, scenario: &crate::Scenario) -> String {
    let model = availability_for(tf_type_of(&r.kind), &r.attrs, DEFAULT_REGION);
    match (&model, &scenario.kind) {
        (AvailabilityModel::SingleAz { az }, crate::ScenarioKind::AzOutage { az: s_az })
            if az == s_az =>
        {
            format!("single-AZ in {az}, which is down")
        }
        (AvailabilityModel::SingleAz { az }, crate::ScenarioKind::RegionOutage { region })
            if &region_of_az(az) == region =>
        {
            format!("single-AZ in {az} (region {region} is down)")
        }
        (AvailabilityModel::MultiAz { azs, .. }, crate::ScenarioKind::RegionOutage { region })
            if azs.iter().all(|a| &region_of_az(a) == region) =>
        {
            format!("multi-AZ across {azs:?} — whole region {region} is down")
        }
        (
            AvailabilityModel::Regional { region },
            crate::ScenarioKind::RegionOutage { region: r },
        ) if region == r => format!("regional in {region}, which is down"),
        (_, crate::ScenarioKind::SlowRdsFailover { db_id }) if db_id == &r.id => {
            format!("RDS {db_id} failover window exceeded SLO — treated as unavailable")
        }
        (_, crate::ScenarioKind::SingleNatDeath { subnet_id }) if subnet_id == &r.id => {
            format!("NAT in subnet {subnet_id} is dead — subnet loses egress")
        }
        (_, crate::ScenarioKind::IamRevocation { principal_arn })
            if names_principal(r, principal_arn) =>
        {
            format!("principal {principal_arn} was revoked")
        }
        _ => "failure propagated from a dependency".to_string(),
    }
}

/// The attribute names under which a Terraform resource carries an IAM principal ARN.
/// `role` is what `aws_lambda_function` uses; the other two cover ECS tasks, EC2 instance
/// profiles and similar. v0.1 is a string match; modelling IAM as graph nodes is future work.
const PRINCIPAL_ATTRS: [&str; 3] = ["iam_role_arn", "role_arn", "role"];

/// Does this resource name `principal_arn` in any of the principal-carrying attributes?
pub(crate) fn names_principal(r: &Resource, principal_arn: &str) -> bool {
    PRINCIPAL_ATTRS
        .iter()
        .any(|key| r.attrs.get(key).and_then(|v| v.as_str()) == Some(principal_arn))
}

/// Map our `ResourceKind` back to the Terraform type string that `availability_for` expects.
fn tf_type_of(kind: &helios_graph::ResourceKind) -> &'static str {
    use helios_graph::ResourceKind::*;
    match kind {
        Vpc => "aws_vpc",
        Subnet => "aws_subnet",
        Instance => "aws_instance",
        Lb => "aws_lb",
        DbInstance => "aws_db_instance",
        ElasticacheCluster => "aws_elasticache_cluster",
        LambdaFunction => "aws_lambda_function",
        S3Bucket => "aws_s3_bucket",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn z3_links_and_solves_trivial() {
        assert_eq!(solver_smoke(), SatResult::Sat);
    }
}

#[cfg(test)]
mod region_tests {
    use super::region_of_az;

    #[test]
    fn strips_az_suffix() {
        assert_eq!(region_of_az("us-east-1a"), "us-east-1");
        assert_eq!(region_of_az("eu-west-2c"), "eu-west-2");
    }

    #[test]
    fn leaves_region_only_alone() {
        assert_eq!(region_of_az("us-east-1"), "us-east-1");
    }
}

#[cfg(test)]
mod encode_tests {
    use super::*;
    use crate::{Scenario, ScenarioKind};
    use helios_graph::from_json;

    const FIXTURE: &str = include_str!("../../../fixtures/three-tier-webapp/terraform-show.json");

    fn build_graph() -> ResourceGraph {
        from_json(FIXTURE).expect("fixture parses")
    }

    #[test]
    fn single_az_ec2_is_down_when_its_az_is_down() {
        let graph = build_graph();
        let solver = Solver::new();
        let mut enc = Encoder::new();
        enc.encode_availability(&graph, &solver);

        let ec2_idx = graph
            .node_indices()
            .find(|i| graph[*i].id == "aws_instance.web")
            .expect("fixture contains aws_instance.web");
        let ec2_down = enc.resource_down[&ec2_idx].clone();

        solver.assert(enc.az_var("us-east-1a"));
        solver.assert(enc.region_var("us-east-1").not());
        enc.pin_everything_else(&solver, &["us-east-1a".into()], &[], &[]);

        assert_eq!(solver.check(), SatResult::Sat);
        let model = solver.get_model().unwrap();
        let ec2_val = model.eval(&ec2_down, true).unwrap().as_bool().unwrap();
        assert!(ec2_val, "EC2 in us-east-1a must be down when 1a is down");
    }

    #[test]
    fn subnet_down_propagates_to_ec2() {
        let graph = build_graph();
        let solver = Solver::new();
        let mut enc = Encoder::new();
        enc.encode_availability(&graph, &solver);
        enc.encode_dependencies(&graph, &solver);

        let ec2_idx = graph
            .node_indices()
            .find(|i| graph[*i].id == "aws_instance.web")
            .unwrap();
        let subnet_idx = graph
            .node_indices()
            .find(|i| graph[*i].id == "aws_subnet.public_a")
            .unwrap();

        let ec2_down = enc.resource_down[&ec2_idx].clone();
        let subnet_down = enc.resource_down[&subnet_idx].clone();

        solver.assert(&subnet_down);
        solver.assert(enc.region_var("us-east-1").not());

        assert_eq!(solver.check(), SatResult::Sat);
        let model = solver.get_model().unwrap();
        let ec2_val = model.eval(&ec2_down, true).unwrap().as_bool().unwrap();
        assert!(ec2_val, "EC2 must be down when its subnet is down");
    }

    #[test]
    fn regional_s3_unaffected_by_az_outage() {
        let graph = build_graph();
        let solver = Solver::new();
        let mut enc = Encoder::new();
        enc.encode_availability(&graph, &solver);

        let s3_idx = graph
            .node_indices()
            .find(|i| matches!(graph[*i].kind, helios_graph::ResourceKind::S3Bucket))
            .expect("fixture contains an S3 bucket");
        let s3_down = enc.resource_down[&s3_idx].clone();

        solver.assert(enc.az_var("us-east-1a"));
        solver.assert(enc.region_var("us-east-1").not());
        enc.pin_everything_else(&solver, &["us-east-1a".into()], &[], &[]);

        assert_eq!(solver.check(), SatResult::Sat);
        let model = solver.get_model().unwrap();
        let s3_val = model.eval(&s3_down, true).unwrap().as_bool().unwrap();
        assert!(!s3_val, "S3 must not be down from an AZ outage alone");
    }

    #[test]
    fn az_outage_takes_single_az_resources_down() {
        let graph = build_graph();
        let solver = Solver::new();
        let mut enc = Encoder::new();
        enc.encode_availability(&graph, &solver);
        enc.encode_dependencies(&graph, &solver);

        let scenario = Scenario {
            name: "lose-1a".into(),
            kind: ScenarioKind::AzOutage {
                az: "us-east-1a".into(),
            },
        };
        enc.apply_scenario(&scenario, &graph, &solver);

        assert_eq!(solver.check(), SatResult::Sat);
        let chain = enc.extract_failures(&graph, &scenario, &solver);

        let ids: Vec<&str> = chain.failures.iter().map(|f| f.id.as_str()).collect();
        assert!(
            ids.contains(&"aws_subnet.public_a"),
            "subnet in 1a must fail"
        );
        assert!(ids.contains(&"aws_instance.web"), "ec2 in 1a must fail");
        assert!(
            !ids.contains(&"aws_subnet.public_b"),
            "subnet in 1b must survive"
        );
    }

    #[test]
    fn slow_rds_failover_forces_the_db_down() {
        let graph = build_graph();
        let solver = Solver::new();
        let mut enc = Encoder::new();
        enc.encode_availability(&graph, &solver);
        enc.encode_dependencies(&graph, &solver);

        let scenario = Scenario {
            name: "rds-slow".into(),
            kind: ScenarioKind::SlowRdsFailover {
                db_id: "aws_db_instance.primary".into(),
            },
        };
        enc.apply_scenario(&scenario, &graph, &solver);

        assert_eq!(solver.check(), SatResult::Sat);
        let chain = enc.extract_failures(&graph, &scenario, &solver);
        assert!(
            chain
                .failures
                .iter()
                .any(|f| f.id == "aws_db_instance.primary"),
            "rds must be down; got {:?}",
            chain.failures
        );
    }

    #[test]
    fn single_nat_death_takes_subnet_and_children_down() {
        let graph = build_graph();
        let solver = Solver::new();
        let mut enc = Encoder::new();
        enc.encode_availability(&graph, &solver);
        enc.encode_dependencies(&graph, &solver);

        let scenario = Scenario {
            name: "nat-1a-dead".into(),
            kind: ScenarioKind::SingleNatDeath {
                subnet_id: "aws_subnet.public_a".into(),
            },
        };
        enc.apply_scenario(&scenario, &graph, &solver);

        assert_eq!(solver.check(), SatResult::Sat);
        let chain = enc.extract_failures(&graph, &scenario, &solver);
        let ids: Vec<&str> = chain.failures.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"aws_subnet.public_a"));
        assert!(
            ids.contains(&"aws_instance.web"),
            "web depends on public_a; must fall; got {ids:?}"
        );
    }

    #[test]
    fn iam_revocation_hits_resources_with_matching_role_arn() {
        // Build a fixture graph, then inject an iam_role_arn into one node
        // to prove the string-match path. We roll a mini in-memory graph so
        // we don't need to touch the shipped fixture.
        let mut graph = build_graph();
        let role_arn = "arn:aws:iam::123:role/web";
        let web_idx = graph
            .node_indices()
            .find(|i| graph[*i].id == "aws_instance.web")
            .unwrap();
        graph[web_idx]
            .attrs
            .as_object_mut()
            .unwrap()
            .insert("iam_role_arn".into(), serde_json::json!(role_arn));

        let solver = Solver::new();
        let mut enc = Encoder::new();
        enc.encode_availability(&graph, &solver);
        enc.encode_dependencies(&graph, &solver);

        let scenario = Scenario {
            name: "revoke-web".into(),
            kind: ScenarioKind::IamRevocation {
                principal_arn: role_arn.into(),
            },
        };
        enc.apply_scenario(&scenario, &graph, &solver);

        assert_eq!(solver.check(), SatResult::Sat);
        let chain = enc.extract_failures(&graph, &scenario, &solver);
        let ids: Vec<&str> = chain.failures.iter().map(|f| f.id.as_str()).collect();
        assert!(
            ids.contains(&"aws_instance.web"),
            "web should fail after its role is revoked; got {ids:?}"
        );
    }

    #[test]
    fn az_outage_pins_the_other_zone_up_not_by_default() {
        // v0.1.0 never constrained az_down_us-east-1b; the other zone's survival was Z3's
        // default assignment for a free Boolean, not a property of the encoding.
        let graph = build_graph();
        let solver = Solver::new();
        let mut enc = Encoder::new();
        enc.encode_availability(&graph, &solver);
        enc.encode_dependencies(&graph, &solver);
        enc.apply_scenario(
            &Scenario {
                name: "lose-1a".into(),
                kind: ScenarioKind::AzOutage {
                    az: "us-east-1a".into(),
                },
            },
            &graph,
            &solver,
        );
        assert_eq!(solver.check(), SatResult::Sat);
        let model = solver.get_model().unwrap();
        let b_down = model
            .eval(&enc.az_down["us-east-1b"], true)
            .unwrap()
            .as_bool()
            .unwrap();
        assert!(!b_down, "us-east-1b must be pinned UP by the encoding");
        // And the solver cannot choose otherwise: asserting it down must be UNSAT.
        solver.assert(&enc.az_down["us-east-1b"]);
        assert_eq!(solver.check(), SatResult::Unsat);
    }

    #[test]
    fn single_nat_death_does_not_take_the_zone_down() {
        // v0.1.0 forced the subnet down THROUGH its availability rule, so Z3 took the whole
        // AZ down and the unrelated cache (no edge to the subnet) fell with it.
        let graph = build_graph();
        let solver = Solver::new();
        let mut enc = Encoder::new();
        enc.encode_availability(&graph, &solver);
        enc.encode_dependencies(&graph, &solver);
        let scenario = Scenario {
            name: "nat-1a-dead".into(),
            kind: ScenarioKind::SingleNatDeath {
                subnet_id: "aws_subnet.public_a".into(),
            },
        };
        enc.apply_scenario(&scenario, &graph, &solver);
        assert_eq!(solver.check(), SatResult::Sat);
        let chain = enc.extract_failures(&graph, &scenario, &solver);
        let ids: Vec<&str> = chain.failures.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["aws_instance.web", "aws_subnet.public_a"],
            "only the subnet and what it CONTAINS may fall; got {ids:?}"
        );
    }

    #[test]
    fn slow_rds_failover_fails_only_the_database() {
        // v0.1.0 forced the multi-AZ DB down through `(a ∧ b) ∨ region`, so Z3 took BOTH zones
        // down and the ALB and both subnets fell — six failures for one slow failover.
        let graph = build_graph();
        let solver = Solver::new();
        let mut enc = Encoder::new();
        enc.encode_availability(&graph, &solver);
        enc.encode_dependencies(&graph, &solver);
        let scenario = Scenario {
            name: "rds-slow".into(),
            kind: ScenarioKind::SlowRdsFailover {
                db_id: "aws_db_instance.primary".into(),
            },
        };
        enc.apply_scenario(&scenario, &graph, &solver);
        assert_eq!(solver.check(), SatResult::Sat);
        let chain = enc.extract_failures(&graph, &scenario, &solver);
        let ids: Vec<&str> = chain.failures.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, vec!["aws_db_instance.primary"], "got {ids:?}");
    }

    #[test]
    fn iam_revocation_reads_the_lambda_role_attribute() {
        // The shipped fixture's Lambda carries its principal under `role`, which v0.1.0 never
        // read — so the bundled iam-revocation scenario reported "resilient" for the wrong reason.
        let graph = build_graph();
        let solver = Solver::new();
        let mut enc = Encoder::new();
        enc.encode_availability(&graph, &solver);
        enc.encode_dependencies(&graph, &solver);
        let scenario = Scenario {
            name: "revoke-lambda-role".into(),
            kind: ScenarioKind::IamRevocation {
                principal_arn: "arn:aws:iam::123456789012:role/lambda-worker".into(),
            },
        };
        enc.apply_scenario(&scenario, &graph, &solver);
        assert_eq!(solver.check(), SatResult::Sat);
        let chain = enc.extract_failures(&graph, &scenario, &solver);
        let ids: Vec<&str> = chain.failures.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, vec!["aws_lambda_function.worker"], "got {ids:?}");
        assert!(chain.failures[0].reason.contains("was revoked"));
    }

    #[test]
    fn region_outage_takes_everything_down_except_global() {
        let graph = build_graph();
        let solver = Solver::new();
        let mut enc = Encoder::new();
        enc.encode_availability(&graph, &solver);
        enc.encode_dependencies(&graph, &solver);

        let scenario = Scenario {
            name: "lose-useast1".into(),
            kind: ScenarioKind::RegionOutage {
                region: "us-east-1".into(),
            },
        };
        enc.apply_scenario(&scenario, &graph, &solver);

        assert_eq!(solver.check(), SatResult::Sat);
        let chain = enc.extract_failures(&graph, &scenario, &solver);

        assert!(
            chain.failures.len() >= graph.node_count() - 1,
            "at least all non-edge resources must fail"
        );
    }
}
