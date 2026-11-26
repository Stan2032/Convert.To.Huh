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
use huh_core::{detect, execute, planner, registry::Registry};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

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
        for (name, fmt, bytes) in &specimens {
            if fmt != &e.from {
                continue;
            }
            match execute::execute(&plan, bytes, 85) {
                Ok((_, receipt)) => {
                    ran += 1;
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
