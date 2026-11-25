//! Convert.To.Huh core: registry, detector, planner, execution, sanitization,
//! and the exporter that publishes the conversion-loss dataset (ADR-0011).

pub mod clean;
pub mod detect;
pub mod engines;
pub mod execute;
pub mod export;
pub mod fidelity;
pub mod loss;
pub mod planner;
pub mod registry;
pub mod sanitize;
pub mod subprocess;
