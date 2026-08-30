use std::collections::HashMap;

use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::tfjson::{Module, RawResource};

pub type ResourceId = String;

/// A typed AWS resource node in the graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resource {
    /// Terraform address, e.g. `aws_vpc.main`. Unique within a plan.
    pub id: ResourceId,
    pub kind: ResourceKind,
    /// Raw attrs, service-specific. Parsed further by helios-models.
    pub attrs: serde_json::Value,
}

/// The 8 resource types helios v0.1 understands.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceKind {
    Vpc,
    Subnet,
    Instance,
    Lb,
    DbInstance,
    ElasticacheCluster,
    LambdaFunction,
    S3Bucket,
}

impl ResourceKind {
    pub fn from_tf_type(tf_type: &str) -> Option<Self> {
        Some(match tf_type {
            "aws_vpc" => Self::Vpc,
            "aws_subnet" => Self::Subnet,
            "aws_instance" => Self::Instance,
            "aws_lb" => Self::Lb,
            "aws_db_instance" => Self::DbInstance,
            "aws_elasticache_cluster" => Self::ElasticacheCluster,
            "aws_lambda_function" => Self::LambdaFunction,
            "aws_s3_bucket" => Self::S3Bucket,
            _ => return None,
        })
    }
}

/// Edge kind between two resources.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Dependency {
    /// Child contains parent via an explicit `*_id` attr (subnet.vpc_id, instance.subnet_id, ...).
    Contains(&'static str),
    /// Load-balancer-style many-to-many membership (alb.subnets[]).
    MemberOf(&'static str),
}

pub(crate) fn build_graph(root: Module) -> Result<DiGraph<Resource, Dependency>, Error> {
    let raw_resources = root.flatten();
    let mut graph = DiGraph::<Resource, Dependency>::new();
    // attr-id (e.g. a VPC's `id` attribute) → node index, so we can resolve `vpc_id = "vpc-123"`.
    let mut by_attr_id: HashMap<String, NodeIndex> = HashMap::new();

    for raw in raw_resources {
        let RawResource {
            address,
            tf_type,
            values,
            ..
        } = raw;
        let Some(kind) = ResourceKind::from_tf_type(&tf_type) else {
            tracing::warn!(tf_type = %tf_type, "skipping unsupported resource type");
            continue;
        };
        let node = graph.add_node(Resource {
            id: address,
            kind,
            attrs: values.clone(),
        });
        if let Some(attr_id) = values.get("id").and_then(|v| v.as_str()) {
            by_attr_id.insert(attr_id.to_string(), node);
        }
    }

    // Second pass: derive edges from attrs.
    let edge_specs = collect_edges(&graph, &by_attr_id);
    for (from, to, dep) in edge_specs {
        graph.add_edge(from, to, dep);
    }

    Ok(graph)
}

fn collect_edges(
    graph: &DiGraph<Resource, Dependency>,
    by_attr_id: &HashMap<String, NodeIndex>,
) -> Vec<(NodeIndex, NodeIndex, Dependency)> {
    let mut out = Vec::new();
    for idx in graph.node_indices() {
        let resource = &graph[idx];
        match resource.kind {
            ResourceKind::Subnet => {
                push_id_ref(
                    &mut out,
                    resource,
                    idx,
                    by_attr_id,
                    "vpc_id",
                    Dependency::Contains("vpc_id"),
                );
            }
            ResourceKind::Instance => {
                push_id_ref(
                    &mut out,
                    resource,
                    idx,
                    by_attr_id,
                    "subnet_id",
                    Dependency::Contains("subnet_id"),
                );
            }
            ResourceKind::Lb => {
                if let Some(subnets) = resource.attrs.get("subnets").and_then(|v| v.as_array()) {
                    for s in subnets {
                        if let Some(sid) = s.as_str() {
                            if let Some(&target) = by_attr_id.get(sid) {
                                out.push((idx, target, Dependency::MemberOf("subnets")));
                            }
                        }
                    }
                }
            }
            ResourceKind::DbInstance => {
                // RDS: attrs vary, so subnet-group matching on `db_subnet_group_name`
                // is deferred. v0.1 links RDS→VPC via `vpc_security_group_ids` where
                // resolvable; fuller edge modelling is out of scope for the 8-resource set.
            }
            ResourceKind::ElasticacheCluster => {
                // Similar to RDS — full edge wiring not yet implemented.
            }
            ResourceKind::LambdaFunction => {
                if let Some(vpc_config) = resource.attrs.get("vpc_config") {
                    if let Some(subnets) = vpc_config.get("subnet_ids").and_then(|v| v.as_array()) {
                        for s in subnets {
                            if let Some(sid) = s.as_str() {
                                if let Some(&target) = by_attr_id.get(sid) {
                                    out.push((idx, target, Dependency::MemberOf("subnet_ids")));
                                }
                            }
                        }
                    }
                }
            }
            ResourceKind::Vpc | ResourceKind::S3Bucket => {
                // Root-ish — no outbound structural edges in v0.1.
            }
        }
    }
    out
}

fn push_id_ref(
    out: &mut Vec<(NodeIndex, NodeIndex, Dependency)>,
    resource: &Resource,
    idx: NodeIndex,
    by_attr_id: &HashMap<String, NodeIndex>,
    field: &str,
    dep: Dependency,
) {
    if let Some(target_id) = resource.attrs.get(field).and_then(|v| v.as_str()) {
        if let Some(&target) = by_attr_id.get(target_id) {
            out.push((idx, target, dep));
        }
    }
}
