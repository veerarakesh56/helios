//! Z3 encoder (Weekend 2). Region-outage + az-outage scenarios only.

use std::collections::HashMap;

use helios_graph::{Resource, ResourceGraph};
use helios_models::{availability_for, AvailabilityModel};
use petgraph::graph::NodeIndex;
use z3::{ast::Bool, SatResult, Solver};

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
    if bytes.last().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
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

    /// For every node in the graph, declare its `resource_down` Bool and assert the
    /// biconditional that relates it to az_down / region_down.
    pub fn encode_availability(&mut self, graph: &ResourceGraph, solver: &Solver) {
        for idx in graph.node_indices() {
            let r: &Resource = &graph[idx];
            let model = availability_for(tf_type_of(&r.kind), &r.attrs, DEFAULT_REGION);
            let down = Bool::new_const(format!("down_{}", r.id));
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
            solver.assert(down.eq(cond));
            self.resource_down.insert(idx, down);
        }
    }
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
    use helios_graph::from_json;

    const FIXTURE: &str =
        include_str!("../../../fixtures/three-tier-webapp/terraform-show.json");

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

        assert_eq!(solver.check(), SatResult::Sat);
        let model = solver.get_model().unwrap();
        let ec2_val = model.eval(&ec2_down, true).unwrap().as_bool().unwrap();
        assert!(ec2_val, "EC2 in us-east-1a must be down when 1a is down");
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

        assert_eq!(solver.check(), SatResult::Sat);
        let model = solver.get_model().unwrap();
        let s3_val = model.eval(&s3_down, true).unwrap().as_bool().unwrap();
        assert!(!s3_val, "S3 must not be down from an AZ outage alone");
    }
}
