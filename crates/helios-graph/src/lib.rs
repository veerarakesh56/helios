//! Parses `terraform show -json` output into a typed resource graph.
//!
//! The public entry point is [`load`]. All other items in this module are building blocks.

use std::path::Path;

mod error;
mod resource;
mod tfjson;

pub use error::Error;
pub use resource::{Dependency, Resource, ResourceId, ResourceKind};

/// A directed graph of typed AWS resources with dependency edges.
pub type ResourceGraph = petgraph::graph::DiGraph<Resource, Dependency>;

/// Load a Terraform JSON file (or a directory containing `terraform-show.json`) and build the graph.
pub fn load<P: AsRef<Path>>(path: P) -> Result<ResourceGraph, Error> {
    let path = path.as_ref();
    let json_path = if path.is_dir() {
        path.join("terraform-show.json")
    } else {
        path.to_path_buf()
    };
    let raw = std::fs::read_to_string(&json_path).map_err(|e| Error::ReadFile {
        path: json_path.clone(),
        source: e,
    })?;
    from_json(&raw)
}

/// Parse a raw Terraform JSON string into a resource graph.
pub fn from_json(raw: &str) -> Result<ResourceGraph, Error> {
    let parsed: tfjson::TerraformShow = serde_json::from_str(raw)?;
    resource::build_graph(parsed.values.root_module)
}
