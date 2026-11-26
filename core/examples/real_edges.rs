//! Run every verified raster edge over every real specimen of its source format.
//!
//!   cargo run --release --example real_edges -- corpus/govdocs1/files OUT.json
//!
//! `huh convert` exercises only the planner's cheapest route, so most edges
//! never meet a real file that way. This forces each one through `execute`,
//! which compares declared loss against measured loss, and reports per edge:
//! how many real files it ran on, how many exercised each declared property,
//! how many contradicted the registry, and SSIM.
//!
//! Specimens are identified by content (`detect`), never by name.
use huh_core::engines::{decode, Budget};
use huh_core::{detect, execute, planner, registry::Registry};
use std::collections::HashSet;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// Distinct RGBA colours, and whether any alpha is partial (not 0 or 255).
///
/// These see the two losses a GIF target causes that the receipt's own
/// checks cannot: palette reduction (more than 256 colours in, fewer out) and
/// one-bit transparency (partial alpha in, only on/off out). Observed on the
/// output, not inferred from the input.
fn colours(img: &image::DynamicImage) -> (usize, bool) {
    let rgba = img.to_rgba8();
    let mut seen = HashSet::new();
    let mut partial = false;
    for p in rgba.pixels() {
        // A fully transparent pixel is invisible whatever RGB it stores, and
        // encoders may legitimately rewrite that RGB (lossless WebP does).
        // Counting it made lossless WebP look like a palette reduction.
        seen.insert(if p.0[3] == 0 { [0, 0, 0, 0] } else { p.0 });
        partial |= p.0[3] != 0 && p.0[3] != 255;
    }
    (seen.len(), partial)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, out) = match args.as_slice() {
        [d, o] => (Path::new(d), Path::new(o)),
        _ => {
            eprintln!("usage: real_edges <specimen-dir> <out.json>");
            std::process::exit(2);
        }
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let reg = Registry::load(root).expect("registry loads");

    let mut specimens: Vec<(String, String, Vec<u8>)> = std::fs::read_dir(dir)
        .expect("specimen dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let bytes = std::fs::read(&p).unwrap();
            let fmt = detect::detect(&reg, &bytes, Some(&name)).format;
            (name, fmt, bytes)
        })
        .collect();
    specimens.sort_by(|a, b| a.0.cmp(&b.0));

    let mut report = Vec::new();
    for (i, e) in reg.edges.iter().enumerate() {
        if e.status != "verified" || !e.from.starts_with("image/") {
            continue;
        }
        let Some(engine) = reg.engines.get(&e.engine) else { continue };
        let Some(profile) = engine.license.build_profiles.first() else { continue };
        let pol = planner::Policy { build_profile: profile.clone(), ..Default::default() };
        let pl = planner::Planner::new(&reg, &pol);
        let Some(plan) = pl.plan_edge(i) else { continue };

        let (mut ran, mut failed) = (0, BTreeMap::<String, usize>::new());
        let mut exercised = BTreeMap::<String, BTreeMap<String, usize>>::new();
        let mut contradictions = Vec::new();
        let mut ssims = Vec::new();
        let (mut palette_reduced, mut partial_alpha_binarised) = (0usize, 0usize);
        for (name, fmt, bytes) in &specimens {
            if fmt != &e.from {
                continue;
            }
            match execute::execute(&plan, bytes, 85) {
                Ok((out, receipt)) => {
                    ran += 1;
                    let b = Budget::default();
                    if let (Ok(a), Ok(z)) = (decode(bytes, &e.from, &b), decode(&out, &e.to, &b)) {
                        let ((ca, pa), (cz, pz)) = (colours(&a), colours(&z));
                        palette_reduced += usize::from(cz < ca);
                        partial_alpha_binarised += usize::from(pa && !pz);
                    }
                    for m in &receipt.measurements {
                        *exercised.entry(m.property.clone()).or_default()
                            .entry(m.measured.clone()).or_default() += 1;
                        if m.contradicts {
                            contradictions.push(json!({
                                "specimen": name, "property": m.property,
                                "declared": m.declared, "measured": m.measured,
                                "detail": m.detail,
                            }));
                        }
                    }
                    if let Some(s) = receipt.ssim {
                        ssims.push((s, name.clone()));
                    }
                }
                Err(err) => {
                    *failed.entry(err.to_string()).or_default() += 1;
                }
            }
        }
        ssims.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut row = Map::new();
        row.insert("engine".into(), json!(e.engine));
        row.insert("invocation".into(), json!(e.invocation));
        row.insert("from".into(), json!(e.from));
        row.insert("to".into(), json!(e.to));
        row.insert("ran".into(), json!(ran));
        row.insert("failed".into(), json!(failed));
        row.insert("measured".into(), json!(exercised));
        row.insert("contradictions".into(), Value::Array(contradictions));
        // Files on which the output has fewer distinct colours than the input,
        // and files whose partial transparency came out on/off only.
        row.insert("observed".into(), json!({
            "palette_reduced": palette_reduced,
            "partial_alpha_binarised": partial_alpha_binarised,
        }));
        if !ssims.is_empty() {
            row.insert("ssim_min".into(), json!((ssims[0].0 * 1e4).round() / 1e4));
            row.insert("ssim_min_specimen".into(), json!(ssims[0].1));
            // Every file whose content came back mostly unrecognisable. A loss
            // vector can be consistent with that and still not explain it, so
            // these are listed for a person to look at, not scored.
            let low: Vec<_> = ssims.iter().filter(|(v, _)| *v < 0.5)
                .map(|(v, n)| json!({"specimen": n, "ssim": (v * 1e4).round() / 1e4}))
                .collect();
            row.insert("ssim_below_0_5".into(), Value::Array(low));
            row.insert("ssim_median".into(),
                       json!((ssims[ssims.len() / 2].0 * 1e4).round() / 1e4));
        }
        eprintln!(
            "{:<12} {:<11} -> {:<11} ran {:>3}  failed {:>2}  contradicted {:>2}  {}",
            e.engine, e.from, e.to, ran, failed.values().sum::<usize>(),
            row["contradictions"].as_array().unwrap().len(),
            e.invocation.as_deref().unwrap_or("(in-process)")
        );
        report.push(Value::Object(row));
    }
    std::fs::write(out, serde_json::to_string_pretty(&report).unwrap() + "\n").unwrap();
}
