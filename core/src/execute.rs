//! Plan execution and the receipt.
//!
//! Hops run in sequence within one process (ADR-0015). The receipt
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
    /// The encoder quality this run used. It is an argument to `execute` rather
    /// than part of the plan, and it changes the output — so a receipt without
    /// it cannot reproduce the file it describes, which is the receipt's only
    /// job. Every execution-affecting input belongs here.
    ///
    /// Applies to IN-PROCESS hops only. A subprocess engine is run exactly as
    /// its edge's `invocation` records, because that string is what was
    /// measured (ADR-0013); quality settings baked into it are not overridden
    /// from here. Read this field together with `hops`.
    pub quality: u8,
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

/// Run one hop THROUGH THE ENGINE THE PLANNER CHOSE.
///
/// Execution used to call the in-process encoder for every hop, whatever the
/// plan said. Because the plan's loss vector and the receipt both come from the
/// selected edge, that made the tool contradict its own dataset: planning a
/// libvips PNG->JPEG hop printed "transparent areas become black"
/// (`composite_black`, which is what libvips does) and then produced image-rs
/// output, where a fully transparent pixel keeps the colour underneath. On
/// `corpus/raster/rgba-gradient.png` the transparent corner came out
/// `[254,254,130]` while real libvips gives `[0,0,0]`.
///
/// ADR-0012 exists precisely because that difference matters to whoever is
/// converting a logo, and ADR-0013 makes `(engine, version, invocation)` part of
/// an edge's identity. So the rule here is absolute: **run the engine named in
/// the plan, or fail.** Silently substituting an engine is worse than refusing,
/// because the receipt keeps claiming the one that was planned.
fn run_hop(
    hop: &crate::planner::Hop,
    bytes: &[u8],
    from_format: &str,
    quality: u8,
    budget: &Budget,
) -> Result<Vec<u8>, ConvertError> {
    match hop.engine.as_str() {
        // In-process, no invocation to honour: image-rs IS this crate's encoder.
        "image-rs" => {
            let img = engines::decode(bytes, from_format, budget)?;
            engines::encode(&img, &hop.to, quality)
        }
        "libvips" => run_libvips(hop, bytes, from_format),
        other => Err(ConvertError::InputUnsupported(format!(
            "plan selects engine `{other}`, which this build cannot execute. \
             Refusing rather than substituting a different engine: the plan's \
             loss vector and this receipt describe `{other}` (ADR-0013)."
        ))),
    }
}

/// Run libvips as the registry recorded it.
///
/// The edge's `invocation` is the recipe, not a description of one: running
/// anything else would measure a different edge. `IN` and `OUT` are the only
/// placeholders, substituted with scratch paths.
fn run_libvips(
    hop: &crate::planner::Hop,
    bytes: &[u8],
    from_format: &str,
) -> Result<Vec<u8>, ConvertError> {
    let Some(invocation) = hop.invocation.as_deref() else {
        return Err(ConvertError::InputUnsupported(format!(
            "the libvips edge {} -> {} records no invocation, so there is no \
             way to run the call that was measured (ADR-0013, rule 8)",
            hop.from, hop.to
        )));
    };

    let scratch = crate::subprocess::Scratch::new("exec")?;
    let in_path = scratch.join(&format!("in.{}", ext_for(from_format)));
    let out_path = scratch.join(&format!("out.{}", ext_for(&hop.to)));
    std::fs::write(&in_path, bytes)
        .map_err(|e| ConvertError::Internal(format!("write hop input: {e}")))?;

    let mut tokens = invocation.split_whitespace();
    let program = tokens.next().unwrap_or("vips");
    let mut cmd = crate::subprocess::External::new(program);
    for t in tokens {
        cmd = match t {
            "IN" => cmd.path_arg(&in_path),
            "OUT" => cmd.path_arg(&out_path),
            other => cmd.arg(other),
        };
    }
    cmd.run()?;

    std::fs::read(&out_path)
        .map_err(|e| ConvertError::Internal(format!("read hop output: {e}")))
}

/// File extension for a format id. Engines dispatch on it, so it has to be
/// right here even though ground rule 1 forbids trusting it on INPUT.
fn ext_for(format_id: &str) -> &'static str {
    match format_id {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/tiff" => "tif",
        "image/bmp" => "bmp",
        _ => "bin",
    }
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
        let out = run_hop(hop, &current_bytes, &current_format, quality, &budget)?;
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
            .map(|h| format!("{}@{}", h.engine, h.engine_version))
            .collect(),
        quality,
        contains_synthesis: plan.totals.contains_synthesis,
        declared_loss: declared.clone(),
        measurements,
        ssim,
        rationale: plan.rationale.clone(),
        wall_ms: started.elapsed().as_millis(),
    };

    Ok((current_bytes, receipt))
}
