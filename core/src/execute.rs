//! Plan execution and the receipt.
//!
//! ADR-0008: execution is a state machine over per-hop artifacts. The receipt
//! is a component rather than a log line for two reasons — users deserve to
//! know when a model wrote their content, and declared-vs-measured divergence
//! is how registry bugs get found automatically.

use crate::engines::{self, Budget, ConvertError};
use crate::fidelity;
use crate::loss::LossValue;
use crate::planner::Plan;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
pub struct Measurement {
    pub property: String,
    pub declared: String,
    pub measured: String,
    /// True when measurement contradicts the registry — a registry bug,
    /// assigned to capability-cartographer (loss-vector.md §5).
    pub contradicts: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Receipt {
    pub registry_version: String,
    pub input_sha256: String,
    pub output_sha256: String,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub source_format: String,
    pub target_format: String,
    pub hops: Vec<String>,
    pub engine_versions: Vec<String>,
    pub contains_synthesis: bool,
    pub declared_loss: crate::loss::LossVector,
    pub measurements: Vec<Measurement>,
    pub ssim: Option<f64>,
    pub rationale: String,
    pub wall_ms: u128,
}

pub fn sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// Execute a plan over raster hops, then measure what actually happened.
pub fn execute(plan: &Plan, input: &[u8], quality: u8) -> Result<(Vec<u8>, Receipt), ConvertError> {
    let started = std::time::Instant::now();
    let budget = Budget::default();

    let original = engines::decode(input, &plan.source, &budget)?;
    let before = engines::observe(&original);

    let mut current_bytes = input.to_vec();
    let mut current_format = plan.source.clone();
    let mut hop_labels = Vec::new();

    for hop in &plan.hops {
        let img = engines::decode(&current_bytes, &current_format, &budget)?;
        let out = engines::encode(&img, &hop.to, quality)?;
        hop_labels.push(format!("{} [{}] {} -> {}", hop.engine, hop.op, hop.from, hop.to));
        current_bytes = out;
        current_format = hop.to.clone();
    }

    let result = engines::decode(&current_bytes, &current_format, &budget)?;
    let after = engines::observe(&result);

    // --- Declared vs measured. This is the falsification step. ---
    let mut measurements = Vec::new();
    let declared = &plan.totals.loss;

    // alpha
    if let Some(d) = declared.get("alpha") {
        let measured_lost = before.has_alpha && !after.has_alpha;
        let measured = if !before.has_alpha {
            "not-present-in-input"
        } else if measured_lost {
            "lost"
        } else {
            "preserved"
        };
        // Only a contradiction if the input actually exercised the property.
        let contradicts = before.has_alpha
            && ((d == LossValue::Lost && !measured_lost)
                || (d == LossValue::Preserved && measured_lost));
        measurements.push(Measurement {
            property: "alpha".into(),
            declared: d.as_str().into(),
            measured: measured.into(),
            contradicts,
            detail: Some(format!(
                "input alpha present: {}, output alpha present: {}",
                before.has_alpha, after.has_alpha
            )),
        });
    }

    // geometry
    if let Some(d) = declared.get("geometry") {
        let same = before.width == after.width && before.height == after.height;
        let contradicts = (d == LossValue::Preserved && !same) || (d == LossValue::Lost && same);
        measurements.push(Measurement {
            property: "geometry".into(),
            declared: d.as_str().into(),
            measured: if same { "preserved".into() } else { "changed".to_string() },
            contradicts,
            detail: Some(format!(
                "{}x{} -> {}x{}",
                before.width, before.height, after.width, after.height
            )),
        });
    }

    // color_depth
    //
    // An 8-bit input cannot demonstrate depth degradation: the property was
    // never exercised, so "preserved" would be a misleading measurement rather
    // than evidence. Same reasoning as alpha on a fully-opaque RGBA input —
    // distinguishing "the format supports it" from "this file uses it" is what
    // keeps the harness honest.
    if let Some(d) = declared.get("color_depth") {
        let reduced = after.bit_depth < before.bit_depth;
        let exercised = before.bit_depth > 8;
        let measured = if !exercised {
            "not-exercised"
        } else if reduced {
            "degraded"
        } else {
            "preserved"
        };
        let contradicts = exercised
            && ((d == LossValue::Preserved && reduced)
                || (d == LossValue::Degraded && !reduced));
        measurements.push(Measurement {
            property: "color_depth".into(),
            declared: d.as_str().into(),
            measured: measured.into(),
            contradicts,
            detail: Some(format!(
                "{} bit -> {} bit{}",
                before.bit_depth,
                after.bit_depth,
                if exercised { "" } else { " (input was already 8-bit)" }
            )),
        });
    }

    // Worst case across backgrounds. A conversion that only looks faithful
    // against a flattering background is not faithful (RESEARCH.md §13.9).
    let ssim = fidelity::content_equivalence(&original, &result);

    let receipt = Receipt {
        registry_version: plan.registry_version.clone(),
        input_sha256: sha256(input),
        output_sha256: sha256(&current_bytes),
        input_bytes: input.len(),
        output_bytes: current_bytes.len(),
        source_format: plan.source.clone(),
        target_format: plan.target.clone(),
        hops: hop_labels,
        engine_versions: plan
            .hops
            .iter()
            .map(|h| format!("{}@{}", h.engine, env!("CARGO_PKG_VERSION")))
            .collect(),
        contains_synthesis: plan.totals.contains_synthesis,
        declared_loss: declared.clone(),
        measurements,
        ssim,
        rationale: plan.rationale.clone(),
        wall_ms: started.elapsed().as_millis(),
    };

    Ok((current_bytes, receipt))
}
