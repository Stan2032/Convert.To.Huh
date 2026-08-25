//! Measure libvips invocations that differ only by a FLAG.
//!
//! ADR-0013: different flags on the same engine are different edges. These two
//! were spotted while falsifying ADR-0012 but never measured across the corpus,
//! so they were left out of the registry rather than guessed at.
use huh_core::engines::{decode, observe, Budget};
use huh_core::fidelity::{ssim, ssim_composited};
use huh_core::subprocess::{External, Scratch};

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let corpus = root.join("corpus/raster");
    let b = Budget::default();

    let mut specimens: Vec<_> = std::fs::read_dir(&corpus).unwrap()
        .filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("png"))
        .collect();
    specimens.sort();

    // (label, saver, extra args, target mime)
    let variants: Vec<(&str, &str, Vec<&str>, &str)> = vec![
        ("webpsave --lossless", "webpsave", vec!["--lossless"], "image/webp"),
        ("jpegsave --background 255", "jpegsave", vec!["--background", "255"], "image/jpeg"),
    ];

    for (label, saver, extra, mime) in variants {
        println!("\n=== vips {label} ===");
        println!("{:<20} {:>9} {:>9} {:<9} ALPHA", "SPECIMEN", "IN", "OUT", "SSIM");
        let scratch = Scratch::new("flags").unwrap();
        for spec in &specimens {
            let name = spec.file_stem().unwrap().to_string_lossy().to_string();
            let src = std::fs::read(spec).unwrap();
            let original = match decode(&src, "image/png", &b) { Ok(i) => i, Err(_) => continue };
            let before = observe(&original);
            let dst = scratch.join(&format!("{name}.out"));

            let cmd = External::new("vips").arg(saver).path_arg(spec).path_arg(&dst).args(&extra);
            if let Err(e) = cmd.run() { println!("{name:<20} {e}"); continue }

            let out = std::fs::read(&dst).unwrap();
            match decode(&out, mime, &b) {
                Ok(back) => {
                    let a = observe(&back);
                    // Comparing stored RGB is meaningless once alpha is in play;
                    // composite both onto the same background first.
                    let s = if before.has_alpha || a.has_alpha {
                        ssim_composited(&original, &back, [255, 255, 255]).unwrap_or(-1.0)
                    } else {
                        ssim(&original, &back).unwrap_or(-1.0)
                    };
                    let alpha = if !before.has_alpha { "unexercised".to_string() }
                                else if a.has_alpha { "preserved".into() } else { "lost".into() };
                    println!("{name:<20} {:>9} {:>9} {s:<9.4} {alpha}", src.len(), out.len());
                }
                Err(e) => println!("{name:<20} unreadable: {e}"),
            }
        }
    }
}
