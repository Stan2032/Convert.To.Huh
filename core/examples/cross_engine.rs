//! Do two independent engines lose the same things on the same conversion?
//!
//! ADR-0011 names this the most valuable data the registry can collect. Loss is
//! recorded per `(engine, from, to)` precisely because the engine is expected to
//! matter — but "expected" is not "measured", and nobody publishes the answer.
//!
//! Compares image-rs (Rust, in-process) against libvips (C, subprocess) on
//! PNG -> JPEG at the same nominal quality, scoring both against the original.
//!
//! Requires libvips: apt-get install libvips-tools
use huh_core::engines::{decode, encode, observe, Budget};
use huh_core::fidelity::{ssim, ssim_composited};
use std::process::Command;

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let corpus = root.join("corpus/raster");
    let tmp = std::path::Path::new("/tmp/huh-xengine");
    std::fs::create_dir_all(tmp).expect("tmpdir");

    let budget = Budget::default();
    const QUALITY: u8 = 85;

    let mut specimens: Vec<std::path::PathBuf> = std::fs::read_dir(&corpus)
        .expect("corpus")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("png"))
        .collect();
    specimens.sort();

    println!("PNG -> JPEG at nominal quality {QUALITY}, both scored against the original\n");
    println!("{:<20} {:>10} {:>10}   {:>9} {:>9}   ALPHA",
             "SPECIMEN", "vips B", "image-rs B", "vips SSIM", "irs SSIM");

    let mut agree_alpha = true;
    for spec in &specimens {
        let name = spec.file_stem().unwrap().to_string_lossy().to_string();
        let src = std::fs::read(spec).expect("read specimen");
        let original = match decode(&src, "image/png", &budget) {
            Ok(i) => i,
            Err(e) => { println!("{name:<20} decode failed: {e}"); continue }
        };
        let before = observe(&original);

        // --- libvips, via subprocess. Argument vector, never a shell string. ---
        let vips_out = tmp.join(format!("{name}-vips.jpg"));
        let ok = Command::new("vips")
            .arg("jpegsave")
            .arg(spec)
            .arg(&vips_out)
            .arg("--Q")
            .arg(QUALITY.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            println!("{name:<20} vips unavailable — install libvips-tools");
            continue;
        }
        let vips_bytes = std::fs::read(&vips_out).expect("vips output");
        let vips_img = decode(&vips_bytes, "image/jpeg", &budget).expect("vips jpeg");

        // --- image-rs, in process ---
        let irs_bytes = encode(&original, "image/jpeg", QUALITY).expect("image-rs encode");
        let irs_img = decode(&irs_bytes, "image/jpeg", &budget).expect("image-rs jpeg");

        // Alpha in play: composite everything onto the SAME background before
        // comparing. Comparing stored RGB rewards engines that leave RGB under
        // transparent pixels untouched, which is a fact about the comparison
        // rather than about the conversion (RESEARCH.md §13.8).
        let (vs, is) = if before.has_alpha {
            (ssim_composited(&original, &vips_img, [255, 255, 255]).unwrap_or(-1.0),
             ssim_composited(&original, &irs_img, [255, 255, 255]).unwrap_or(-1.0))
        } else {
            (ssim(&original, &vips_img).unwrap_or(-1.0),
             ssim(&original, &irs_img).unwrap_or(-1.0))
        };

        // Both should drop alpha — JPEG cannot carry it. Disagreement here
        // would be a registry-level finding, not a quality difference.
        let va = observe(&vips_img).has_alpha;
        let ia = observe(&irs_img).has_alpha;
        let alpha = if !before.has_alpha {
            "unexercised".to_string()
        } else if va == ia {
            format!("both {}", if va { "kept" } else { "lost" })
        } else {
            agree_alpha = false;
            format!("DISAGREE vips={va} irs={ia}")
        };

        println!("{name:<20} {:>10} {:>10}   {:>9.4} {:>9.4}   {alpha}",
                 vips_bytes.len(), irs_bytes.len(), vs, is);
    }

    println!("\nSame nominal quality does NOT mean the same conversion. Quality numbers");
    println!("are encoder-specific: they select quantisation tables, and two encoders");
    println!("map the same integer to different tables. Compare fidelity, not settings.");
    println!("\nPer-property agreement on alpha: {}",
             if agree_alpha { "yes — both engines lose it, as JPEG requires" }
             else { "NO — investigate, this is a registry finding" });
}
