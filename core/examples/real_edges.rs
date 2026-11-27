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

/// The input's metadata payloads: EXIF/XMP/IPTC blocks and comments in JPEG,
/// text and EXIF chunks in PNG, comment and XMP extensions in GIF. Payloads
/// under 8 bytes are skipped, since a byte search for them in the output would
/// match by chance.
///
/// Used to observe `metadata = lost` directly: did any of THIS file's metadata
/// reach the output, in any wrapper? A byte search finds EXIF carried from a
/// JPEG APP1 into a PNG eXIf chunk, which a "does the output have metadata"
/// check would miss, and it ignores metadata an encoder adds of its own.
fn metadata_payloads(b: &[u8], fmt: &str) -> Vec<Vec<u8>> {
    let mut out = vec![];
    match fmt {
        "image/png" if b.len() > 8 => {
            let mut i = 8;
            while i + 12 <= b.len() {
                let len = u32::from_be_bytes(b[i..i + 4].try_into().unwrap()) as usize;
                let Some(end) = (i + 8).checked_add(len) else { break };
                if end + 4 > b.len() { break; }
                let tag = &b[i + 4..i + 8];
                let body = &b[i + 8..end];
                match tag {
                    // tEXt is keyword\0value: the value is the payload.
                    b"tEXt" => out.extend(body.iter().position(|&c| c == 0).map(|z| body[z + 1..].to_vec())),
                    // zTXt is keyword\0method\0zlib(value). Searched for in
                    // its DECOMPRESSED form: ImageMagick carries a compressed
                    // PNG comment into a GIF as plain text, which a search for
                    // the compressed bytes miscounted as lost (189036.png).
                    b"zTXt" => out.extend(body.iter().position(|&c| c == 0)
                        .and_then(|z| inflate(body.get(z + 2..)?))),
                    // iTXt: keyword\0 flag method lang\0 key\0 text, text
                    // zlib-compressed when flag is 1.
                    b"iTXt" => out.extend(itxt_text(body)),
                    b"eXIf" => out.push(body.to_vec()),
                    _ => {}
                }
                i = end + 4;
            }
        }
        "image/jpeg" => {
            let mut i = 2;
            while i + 4 <= b.len() && b[i] == 0xFF {
                let m = b[i + 1];
                if m == 0xDA || m == 0xD9 { break; } // image data / end
                let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
                let Some(end) = (i + 2).checked_add(len) else { break };
                if len < 2 || end > b.len() { break; }
                let body = &b[i + 4..end];
                // APP1 (EXIF/XMP), APP13 (IPTC), COM.
                if m == 0xE1 || m == 0xED || m == 0xFE { out.push(body.to_vec()); }
                i = end;
            }
        }
        "image/gif" if b.len() > 13 => {
            // Comment (0xFE) and application (0xFF, e.g. XMP) extensions.
            let mut i = 13 + if b[10] & 0x80 != 0 { 3usize << ((b[10] & 7) + 1) } else { 0 };
            while i + 2 < b.len() {
                match b[i] {
                    0x21 => {
                        let kind = b[i + 1];
                        let mut j = i + 2;
                        let mut payload = vec![];
                        while j < b.len() && b[j] != 0 {
                            let n = b[j] as usize;
                            payload.extend_from_slice(&b[(j + 1).min(b.len())..(j + 1 + n).min(b.len())]);
                            j += n + 1;
                        }
                        if (kind == 0xFE || kind == 0xFF) && !payload.starts_with(b"NETSCAPE") {
                            out.push(payload);
                        }
                        i = j + 1;
                    }
                    _ => break, // image data: metadata extensions precede it
                }
            }
        }
        _ => {}
    }
    out.retain(|p| p.len() >= 8);
    out
}

fn inflate(z: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut v = vec![];
    flate2::read::ZlibDecoder::new(z).take(1 << 20).read_to_end(&mut v).ok()?;
    Some(v)
}

fn itxt_text(body: &[u8]) -> Option<Vec<u8>> {
    let k = body.iter().position(|&c| c == 0)?;
    let (flag, rest) = (*body.get(k + 1)?, body.get(k + 3..)?);
    let lang = rest.iter().position(|&c| c == 0)?;
    let rest = rest.get(lang + 1..)?;
    let key = rest.iter().position(|&c| c == 0)?;
    let text = rest.get(key + 1..)?;
    if flag == 1 { inflate(text) } else { Some(text.to_vec()) }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
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
        let (mut metadata_present, mut metadata_lost, mut metadata_survived) = (0usize, 0usize, 0usize);
        let mut metadata_some_missing = 0usize;
        let mut metadata_survived_in: Vec<String> = vec![];
        let mut size_ratios: Vec<f64> = vec![];
        for (name, fmt, bytes) in &specimens {
            if fmt != &e.from {
                continue;
            }
            match execute::execute(&plan, bytes, 85) {
                Ok((out, receipt)) => {
                    ran += 1;
                    size_ratios.push(out.len() as f64 / bytes.len().max(1) as f64);
                    let meta = metadata_payloads(bytes, &e.from);
                    if !meta.is_empty() {
                        metadata_present += 1;
                        metadata_some_missing += usize::from(meta.iter().any(|p| !contains(&out, p)));
                        if meta.iter().any(|p| contains(&out, p)) {
                            metadata_survived += 1;
                            metadata_survived_in.push(name.clone());
                        } else {
                            metadata_lost += 1;
                        }
                    }
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
        // Output bytes over input bytes, per real file. A cost, not a loss:
        // no tracked property says a conversion made the file 23x larger.
        size_ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if !size_ratios.is_empty() {
            let r3 = |x: f64| (x * 1000.0).round() / 1000.0;
            row.insert("size_ratio".into(), json!({
                "median": r3(size_ratios[size_ratios.len() / 2]),
                "min": r3(size_ratios[0]),
                "max": r3(*size_ratios.last().unwrap()),
            }));
        }
        // Files on which the output has fewer distinct colours than the input,
        // and files whose partial transparency came out on/off only.
        row.insert("observed".into(), json!({
            "palette_reduced": palette_reduced,
            "partial_alpha_binarised": partial_alpha_binarised,
            // Files whose input carried metadata; of those, none of it in the
            // output vs at least one payload found in the output.
            "metadata_present": metadata_present,
            "metadata_lost": metadata_lost,
            "metadata_survived": metadata_survived,
            // At least one of the input's payloads absent from the output: the
            // observation behind `metadata = degraded` (partly kept).
            "metadata_some_missing": metadata_some_missing,
            "metadata_survived_in": metadata_survived_in,
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
