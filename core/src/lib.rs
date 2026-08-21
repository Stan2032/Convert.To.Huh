//! Convert.To.Huh core: registry, detector, planner, execution.
//!
//! One implementation, compiled to native and `wasm32` (ADR-0002), so the
//! browser's estimate and the server's cannot drift.

pub mod detect;
pub mod engines;
pub mod execute;
pub mod fidelity;
pub mod loss;
pub mod planner;
pub mod registry;
pub mod sanitize;
