//! Minimal typed mirror of `terraform show -json` output.
//!
//! Only the subset helios-graph actually reads. Schema reference:
//! <https://developer.hashicorp.com/terraform/internals/json-format>

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Debug)]
pub struct TerraformShow {
    pub values: Values,
}

#[derive(Deserialize, Debug)]
pub struct Values {
    pub root_module: Module,
}

#[derive(Deserialize, Debug, Default)]
pub struct Module {
    #[serde(default)]
    pub resources: Vec<RawResource>,
    #[serde(default)]
    pub child_modules: Vec<Module>,
}

/// A single resource entry from the `resources` array. The `values` field is service-specific;
/// we keep it opaque here and let each `Resource` variant deserialize its own slice.
#[derive(Deserialize, Debug)]
pub struct RawResource {
    pub address: String,
    #[serde(rename = "type")]
    pub tf_type: String,
    #[serde(default)]
    pub values: Value,
}

impl Module {
    /// Flatten `root_module` + `child_modules` recursively into a single vec of resources.
    pub fn flatten(self) -> Vec<RawResource> {
        let Module {
            mut resources,
            child_modules,
        } = self;
        for child in child_modules {
            resources.extend(child.flatten());
        }
        resources
    }
}
