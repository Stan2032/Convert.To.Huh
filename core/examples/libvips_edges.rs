//! Measure every libvips raster pair against the corpus, for the registry.
//!
//! Sizes alone are not loss. This scores each conversion against the original so
//! the edges can be written from measurement rather than from vips' docs.
use huh_core::engines::{decode, observe, Budget};
use huh_core::fidelity::ssim;
use std::process::Command;

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let corpus = root.join("corpus/raster");
    let tmp = std::path::Path::new("/tmp/huh-libvips");
    std::fs::create_dir_all(tmp).unwrap();
    let b = Budget::default();

    let targets = [("jpeg", "image/jpeg"), ("webp", "image/webp"),
                   ("png", "image/png"), ("tiff", "image/tiff")];

    let mut specimens: Vec<_> = std::fs::read_dir(&corpus).unwrap()
        .filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("png"))
        .collect();
    specimens.sort();

    for (suffix, mime) in targets {
        println!("\n=== PNG -> {mime} (libvips defaults) ===");
        println!("{:<20} {:>9} {:>9} {:<9} ALPHA", "SPECIMEN", "IN", "OUT", "SSIM");
        for spec in &specimens {
            let name = spec.file_stem().unwrap().to_string_lossy().to_string();
            let src = std::fs::read(spec).unwrap();
            let original = match decode(&src, "image/png", &b) { Ok(i) => i, Err(_) => continue };
            let before = observe(&original);
            let dst = tmp.join(format!("{name}.{suffix}"));
            let ok = Command::new("vips").arg(format!("{suffix}save"))
                .arg(spec).arg(&dst).status().map(|s| s.success()).unwrap_or(false);
            if !ok { println!("{name:<20} unsupported"); continue }
            let out = std::fs::read(&dst).unwrap();
            match decode(&out, mime, &b) {
                Ok(back) => {
                    let a = observe(&back);
                    let s = ssim(&original, &back).unwrap_or(-1.0);
                    let alpha = if !before.has_alpha { "unexercised".into() }
                                else if a.has_alpha { "preserved".to_string() }
                                else { "lost".to_string() };
                    println!("{name:<20} {:>9} {:>9} {s:<9.4} {alpha}", src.len(), out.len());
                }
                Err(e) => println!("{name:<20} {:>9} {:>9} unreadable: {e}", src.len(), out.len()),
            }
        }
    }
}
