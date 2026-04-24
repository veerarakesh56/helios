//! Z3-powered simulation core.

pub mod scenario;
pub mod smt;

pub use scenario::{Scenario, ScenarioError, ScenarioKind};
