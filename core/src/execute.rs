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
    /// Why `measurements` is empty, when no in-process check applies to this
    /// pair of formats. Absent when the result was measured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unmeasured: Option<String>,
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
        other => match programs_for(other) {
            Some(programs) => run_invocation(hop, programs, bytes, from_format),
            None => Err(ConvertError::InputUnsupported(format!(
                "plan selects engine `{other}`, which this build cannot execute. \
                 Refusing rather than substituting a different engine: the plan's \
                 loss vector and this receipt describe `{other}` (ADR-0013)."
            ))),
        },
    }
}

/// The programs an engine's invocation may start with. The invocation is
/// registry data; this check keeps a typo there (or an edge filed under the
/// wrong engine) from running a program the plan never named.
fn programs_for(engine: &str) -> Option<&'static [&'static str]> {
    Some(match engine {
        "libvips" => &["vips"],
        "ffmpeg" => &["ffmpeg"],
        "sox" => &["sox"],
        "gstreamer" => &["gst-launch-1.0"],
        "imagemagick" => &["convert", "magick"],
        _ => return None,
    })
}

/// Run an external engine exactly as the registry recorded it.
///
/// The edge's `invocation` is the recipe, not a description of one: running
/// anything else would measure a different edge. Placeholders are `IN` and
/// `OUT` as whole tokens, `key=IN` / `key=OUT` (GStreamer's `location=`), and
/// `OUT.<ext>` where the extension matches the target. Anything else carrying
/// a placeholder is refused rather than guessed at.
fn run_invocation(
    hop: &crate::planner::Hop,
    programs: &[&str],
    bytes: &[u8],
    from_format: &str,
) -> Result<Vec<u8>, ConvertError> {
    let Some(invocation) = hop.invocation.as_deref() else {
        return Err(ConvertError::InputUnsupported(format!(
            "the {} edge {} -> {} records no invocation, so there is no \
             way to run the call that was measured (ADR-0013, rule 8)",
            hop.engine, hop.from, hop.to
        )));
    };

    let mut tokens = invocation.split_whitespace();
    let program = tokens.next().unwrap_or_default();
    if !programs.contains(&program) {
        return Err(ConvertError::InputUnsupported(format!(
            "the {} edge {} -> {} starts with `{program}`, expected one of {:?}; \
             refusing to run it",
            hop.engine, hop.from, hop.to, programs
        )));
    }

    let scratch = crate::subprocess::Scratch::new("exec")?;
    let in_ext = ext_for(from_format);
    let out_ext = ext_for(&hop.to);
    let in_path = scratch.join(&format!("in.{in_ext}"));
    let out_path = scratch.join(&format!("out.{out_ext}"));
    std::fs::write(&in_path, bytes)
        .map_err(|e| ConvertError::Internal(format!("write hop input: {e}")))?;

    let out_with_ext = format!("OUT.{out_ext}");
    let mut cmd = crate::subprocess::External::new(program);
    for t in tokens {
        cmd = match t {
            "IN" => cmd.path_arg(&in_path),
            "OUT" => cmd.path_arg(&out_path),
            t if t == out_with_ext => cmd.path_arg(&out_path),
            t if t.ends_with("=IN") => {
                cmd.arg(format!("{}{}", &t[..t.len() - 2], in_path.display()))
            }
            t if t.ends_with("=OUT") => {
                cmd.arg(format!("{}{}", &t[..t.len() - 3], out_path.display()))
            }
            t if t.split(['=', '.', ',', ':']).any(|p| p == "IN" || p == "OUT") => {
                return Err(ConvertError::InputUnsupported(format!(
                    "invocation token `{t}` in the {} edge {} -> {} carries a \
                     placeholder this executor does not understand",
                    hop.engine, hop.from, hop.to
                )));
            }
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
        "audio/wav" => "wav",
        "audio/mpeg" => "mp3",
        "audio/flac" => "flac",
        "application/pdf" => "pdf",
        _ => "bin",
    }
}

/// Execute a plan, then measure what actually happened where this crate can.
pub fn execute(plan: &Plan, input: &[u8], quality: u8) -> Result<(Vec<u8>, Receipt), ConvertError> {
    let started = std::time::Instant::now();
    let budget = Budget::default();
    if input.len() as u64 > budget.max_input_bytes {
        return Err(ConvertError::BudgetExceeded(format!(
            "input is {} bytes, limit is {}",
            input.len(),
            budget.max_input_bytes
        )));
    }

    let mut current_bytes = input.to_vec();
    let mut current_format = plan.source.clone();
    let mut hop_labels = Vec::new();

    for hop in &plan.hops {
        let out = run_hop(hop, &current_bytes, &current_format, quality, &budget)?;
        hop_labels.push(format!("{} [{}] {} -> {}", hop.engine, hop.op, hop.from, hop.to));
        current_bytes = out;
        current_format = hop.to.clone();
    }

    // --- Declared vs measured. This is the falsification step. ---
    //
    // Raster in and out: decoded and compared in-process. An empty route is a
    // copy and has nothing to measure. Anything else is run but not measured
    // here, and the receipt says so rather than implying it was checked.
    let declared = &plan.totals.loss;
    let (measurements, ssim, unmeasured) =
        if engines::is_raster(&plan.source) && engines::is_raster(&current_format) {
            let (m, s) = measure_raster(plan, input, &current_bytes, &current_format, &budget)?;
            (m, s, None)
        } else if plan.hops.is_empty() {
            (Vec::new(), None, None)
        } else {
            (Vec::new(), None, Some(format!(
                "{} -> {} is not measured in-process; audio entries in the dataset \
                 are measured with tools/measure_audio.py",
                plan.source, current_format
            )))
        };

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
        unmeasured,
        rationale: plan.rationale.clone(),
        wall_ms: started.elapsed().as_millis(),
    };

    Ok((current_bytes, receipt))
}

/// Decode both ends in-process and compare the raster properties.
fn measure_raster(
    plan: &Plan,
    input: &[u8],
    output: &[u8],
    output_format: &str,
    budget: &Budget,
) -> Result<(Vec<Measurement>, Option<f64>), ConvertError> {
    let original = engines::decode(input, &plan.source, budget)?;
    let before = engines::observe(&original);
    let result = engines::decode(output, output_format, budget)?;
    let after = engines::observe(&result);

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
    Ok((measurements, ssim))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hop(engine: &str, invocation: &str) -> crate::planner::Hop {
        crate::planner::Hop {
            index: 0,
            engine: engine.into(),
            op: "Transcode".into(),
            from: "audio/wav".into(),
            to: "audio/flac".into(),
            from_kind: "AudioPCM".into(),
            to_kind: "AudioPCM".into(),
            loss: crate::loss::LossVector::new(),
            isolation: "gvisor".into(),
            estimated_ms: 0,
            verification_status: "verified".into(),
            engine_version: "test".into(),
            invocation: Some(invocation.into()),
        }
    }

    #[test]
    fn an_invocation_naming_another_program_is_refused() {
        let h = hop("ffmpeg", "sox IN OUT");
        let e = run_invocation(&h, programs_for("ffmpeg").unwrap(), b"x", "audio/wav")
            .unwrap_err();
        assert!(e.to_string().contains("expected one of"), "{e}");
    }

    #[test]
    fn an_unrecognised_placeholder_is_refused_not_passed_through() {
        // `IN:0` is not a form this executor substitutes. Passing it through
        // would hand the engine a literal "IN:0" and measure nothing real.
        let h = hop("ffmpeg", "ffmpeg -i IN:0 OUT");
        let e = run_invocation(&h, programs_for("ffmpeg").unwrap(), b"x", "audio/wav")
            .unwrap_err();
        assert!(e.to_string().contains("placeholder"), "{e}");
    }

    #[test]
    fn engines_without_a_known_program_are_not_run() {
        assert!(programs_for("whisper-cpp").is_none());
    }
}
