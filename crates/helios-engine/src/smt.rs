//! Z3 encoder (Weekend 2). Region-outage + az-outage scenarios only.

use z3::{ast::Bool, SatResult, Solver};

/// Smoke test the Z3 binding compiles and links.
#[doc(hidden)]
pub fn solver_smoke() -> SatResult {
    let solver = Solver::new();
    let a = Bool::new_const("a");
    solver.assert(&a);
    solver.check()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn z3_links_and_solves_trivial() {
        assert_eq!(solver_smoke(), SatResult::Sat);
    }
}
