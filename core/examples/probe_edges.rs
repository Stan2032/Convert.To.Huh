//! Measure every candidate format pair across every corpus specimen.
//!
//! This is `capability-cartographer`'s instrument. It answers, empirically:
//! does this pair work, what does it do to each tracked property, and how
//! faithful is the result? The registry is written FROM this output, never
//! from the engine's documentation (docs/RESEARCH.md §3.5).
use huh_core::engines::{decode, encode, observe, Budget};
use huh_core::fidelity::ssim;

fn main() {
    let ids = ["image/png", "image/jpeg", "image/gif", "image/bmp", "image/tiff", "image/webp"];
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("corpus/raster");
    let mut specimens: Vec<(String, Vec<u8>)> = std::fs::read_dir(&corpus)
        .expect("corpus")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("png"))
        .map(|p| (p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read(&p).unwrap()))
        .collect();
    specimens.sort_by(|a, b| a.0.cmp(&b.0));

    let b = Budget::default();
    println!("TARGET       SPECIMEN               IN      OUT SSIM      ALPHA       DEPTH");
    for target in ids {
        if target == "image/png" { continue; }
        let mut ssims = Vec::new();
        for (name, src) in &specimens {
            let img = match decode(src, "image/png", &b) { Ok(i) => i, Err(e) => { println!("  decode {name}: {e}"); continue } };
            let before = observe(&img);
            match encode(&img, target, 85) {
                Ok(out) => {
                    let back = decode(&out, target, &b);
                    let (s, alpha, depth) = match &back {
                        Ok(bk) => {
                            let a = observe(bk);
                            let sv = ssim(&img, bk).unwrap_or(-1.0);
                            let alpha = if !before.has_alpha { "unexercised".to_string() }
                                        else if a.has_alpha { "preserved".to_string() }
                                        else { "lost".to_string() };
                            let depth = if before.bit_depth <= 8 { "unexercised".to_string() }
                                        else if a.bit_depth < before.bit_depth { "degraded".to_string() }
                                        else { "preserved".to_string() };
                            (sv, alpha, depth)
                        }
                        Err(e) => { println!("  {target} {name}: UNREADABLE {e}"); continue }
                    };
                    ssims.push(s);
                    println!("{:<12} {:<16} {:>8} {:>8} {:<9.4} {:<11} {}",
                             target, name, src.len(), out.len(), s, alpha, depth);
                }
                Err(e) => println!("{:<12} {:<16} ENCODE FAILED: {e}", target, name),
            }
        }
        if !ssims.is_empty() {
            let mean: f64 = ssims.iter().sum::<f64>() / ssims.len() as f64;
            let min = ssims.iter().cloned().fold(f64::INFINITY, f64::min);
            println!("{:<12} {:<16} {:>8} {:>8} mean={:.4} min={:.4}\n", target, "-- SUMMARY --", "", "", mean, min);
        }
    }
}
