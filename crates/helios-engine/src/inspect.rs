//! Combined `{graph, chain}` JSON document for the W5 web viewer.
//!
//! Hand-rolled flat shape: petgraph's native serde uses `NodeIndex` integers
//! that are unstable across builds, so we serialize Terraform addresses as
//! node IDs and rebuild edges as `{from, to, dep}` triples keyed on those IDs.

use helios_graph::{Dependency, ResourceGraph};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};

use crate::report::FailureChain;

/// Top-level document emitted by `helios inspect`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InspectDoc {
    pub scenario: String,
    pub graph: GraphDoc,
    pub chain: FailureChain,
}

/// Flat graph: node and edge lists, addressable by Terraform `id`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GraphDoc {
    pub nodes: Vec<NodeDoc>,
    pub edges: Vec<EdgeDoc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NodeDoc {
    pub id: String,
    pub kind: String,
    pub attrs: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EdgeDoc {
    pub from: String,
    pub to: String,
    pub dep: DepDoc,
}

/// Tagged form of [`Dependency`]: `{"kind": "Contains", "via": "vpc_id"}`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "via")]
pub enum DepDoc {
    Contains(String),
    MemberOf(String),
}

/// Attribute names whose values must never leave the machine in an inspect document. The
/// document is uploaded as a CI artifact and pasted into a viewer, and a real
/// `terraform show -json` carries plaintext passwords and keys under names like these.
const SENSITIVE_ATTR_FRAGMENTS: [&str; 8] = [
    "password",
    "passwd",
    "secret",
    "token",
    "private_key",
    "access_key",
    "credential",
    "api_key",
];

/// Placeholder written in place of a sensitive value.
pub const REDACTED: &str = "<redacted>";

/// Recursively replace the value of any attribute whose name contains a sensitive fragment.
/// Keys are kept so a reader can see the field existed; only the value is replaced.
pub fn scrub_sensitive_attrs(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let lower = k.to_ascii_lowercase();
                    let hit = SENSITIVE_ATTR_FRAGMENTS.iter().any(|f| lower.contains(f));
                    let scrubbed = if hit && !v.is_null() {
                        serde_json::Value::String(REDACTED.to_string())
                    } else {
                        scrub_sensitive_attrs(v)
                    };
                    (k.clone(), scrubbed)
                })
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(scrub_sensitive_attrs).collect())
        }
        other => other.clone(),
    }
}

/// Build an [`InspectDoc`] from a graph and a freshly-simulated chain. Every node's attrs are
/// passed through [`scrub_sensitive_attrs`] first.
pub fn build_inspect(graph: &ResourceGraph, chain: FailureChain) -> InspectDoc {
    let scenario = chain.scenario.clone();

    let nodes = graph
        .node_indices()
        .map(|idx| {
            let r = &graph[idx];
            NodeDoc {
                id: r.id.clone(),
                kind: format!("{:?}", r.kind),
                attrs: scrub_sensitive_attrs(&r.attrs),
            }
        })
        .collect();

    let edges = graph
        .edge_references()
        .map(|e| {
            let from = graph[e.source()].id.clone();
            let to = graph[e.target()].id.clone();
            let dep = match e.weight() {
                Dependency::Contains(via) => DepDoc::Contains((*via).to_string()),
                Dependency::MemberOf(via) => DepDoc::MemberOf((*via).to_string()),
            };
            EdgeDoc { from, to, dep }
        })
        .collect();

    InspectDoc {
        scenario,
        graph: GraphDoc { nodes, edges },
        chain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{simulate, Scenario, ScenarioKind};
    use helios_graph::from_json;

    const FIXTURE: &str = include_str!("../../../fixtures/three-tier-webapp/terraform-show.json");

    #[test]
    fn dep_doc_serializes_with_kind_and_via() {
        let dep = DepDoc::Contains("vpc_id".into());
        let json = serde_json::to_value(&dep).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "Contains", "via": "vpc_id"})
        );

        let dep = DepDoc::MemberOf("subnets".into());
        let json = serde_json::to_value(&dep).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "MemberOf", "via": "subnets"})
        );
    }

    #[test]
    fn build_inspect_emits_nodes_edges_and_chain() {
        let graph = from_json(FIXTURE).unwrap();
        let scenario = Scenario {
            name: "lose-us-east-1a".into(),
            kind: ScenarioKind::AzOutage {
                az: "us-east-1a".into(),
            },
        };
        let chain = simulate(&graph, &scenario).unwrap();
        let doc = build_inspect(&graph, chain);

        assert_eq!(doc.scenario, "lose-us-east-1a");
        assert!(!doc.graph.nodes.is_empty());
        assert!(!doc.graph.edges.is_empty());
        assert!(!doc.chain.failures.is_empty());

        // Every edge must reference an existing node id (no dangling edges).
        let ids: std::collections::HashSet<&str> =
            doc.graph.nodes.iter().map(|n| n.id.as_str()).collect();
        for e in &doc.graph.edges {
            assert!(ids.contains(e.from.as_str()), "dangling from: {}", e.from);
            assert!(ids.contains(e.to.as_str()), "dangling to: {}", e.to);
        }

        // At least one Subnet→Vpc Contains edge in the three-tier fixture.
        let has_contains = doc
            .graph
            .edges
            .iter()
            .any(|e| matches!(&e.dep, DepDoc::Contains(via) if via == "vpc_id"));
        assert!(has_contains, "expected at least one Contains(vpc_id) edge");
    }

    #[test]
    fn inspect_redacts_sensitive_attrs_and_keeps_the_rest() {
        let attrs = serde_json::json!({
            "identifier": "three-tier-db",
            "password": "changeme-in-real-life",
            "master_user_secret": {"kms_key_id": "abc"},
            "vpc_config": {"api_key": "k", "subnet_ids": ["subnet-0a1a"]},
            "tags": {"Name": "db"}
        });
        let out = scrub_sensitive_attrs(&attrs);
        assert_eq!(out["identifier"], "three-tier-db");
        assert_eq!(out["password"], REDACTED);
        assert_eq!(out["master_user_secret"], REDACTED);
        assert_eq!(out["vpc_config"]["api_key"], REDACTED);
        assert_eq!(out["vpc_config"]["subnet_ids"][0], "subnet-0a1a");
        assert_eq!(out["tags"]["Name"], "db");
    }

    #[test]
    fn inspect_doc_round_trips_via_json() {
        let graph = from_json(FIXTURE).unwrap();
        let scenario = Scenario {
            name: "lose-us-east-1a".into(),
            kind: ScenarioKind::AzOutage {
                az: "us-east-1a".into(),
            },
        };
        let chain = simulate(&graph, &scenario).unwrap();
        let doc = build_inspect(&graph, chain);

        let json = serde_json::to_string(&doc).unwrap();
        let restored: InspectDoc = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, restored);
    }
}
