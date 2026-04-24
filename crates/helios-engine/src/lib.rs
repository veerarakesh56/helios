//! Z3-powered simulation core.

pub mod report;
pub mod scenario;
pub mod simulate;
pub mod smt;

pub use report::{FailedResource, FailureChain};
pub use scenario::{Scenario, ScenarioError, ScenarioKind};
pub use simulate::{simulate, SimulateError};
