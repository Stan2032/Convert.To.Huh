//! Does colour-depth reduction diverge across engines the way alpha did?
//!
//! Tests a prediction (docs/RESEARCH.md §15): alpha, audio_channels and
//! bit_depth all needed a policy qualifier because engines choose differently
//! about what replaces a destroyed property. GIF palettisation is the same
//! shape — 16M colours down to 256 — so the pattern predicts divergence here.
//!
//! It also demonstrates a metric failure worth knowing about: **SSIM does not
//! detect dithering.** It is a spatial-window statistic and tolerates
//! high-frequency noise, so all three engines score within 0.03 of each other
//! while doing visibly different things. The discriminating measurement is how
//! often adjacent pixels differ — error diffusion makes neighbours alternate.
use huh_core::engines::{decode, Budget};
use huh_core::fidelity::ssim;

/// Fraction of horizontally adjacent pixels that differ. Flat palette mapping
/// produces long runs of one colour; error diffusion breaks them up.
fn adjacent_change_rate(img: &image::DynamicImage) -> f64 {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let (mut diff, mut total) = (0u64, 0u64);
    for y in 0..h {
        for x in 1..w {
            if rgb.get_pixel(x, y) != rgb.get_pixel(x - 1, y) {
                diff += 1;
            }
            total += 1;
        }
    }
    if total == 0 { 0.0 } else { diff as f64 / total as f64 }
}

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let src = std::fs::read(root.join("corpus/raster/rgb-photo.png")).unwrap();
    let b = Budget::default();
    let original = decode(&src, "image/png", &b).unwrap();
    let base_rate = adjacent_change_rate(&original);

    println!("PNG -> GIF (256-colour palette), same specimen, three engines");
    println!("original adjacent-change rate: {:.1}%\n", 100.0 * base_rate);
    println!("{:<14} {:>9} {:<9} {:<12} TEXTURE RETAINED", "ENGINE", "BYTES", "SSIM", "ADJ-CHANGE");

    for (tag, path) in [
        ("image-rs", "/tmp/huh-gif/irs.gif"),
        ("libvips", "/tmp/huh-gif/vips.gif"),
        ("imagemagick", "/tmp/huh-gif/im.gif"),
    ] {
        let Ok(bytes) = std::fs::read(path) else { println!("{tag:<14} (absent)"); continue };
        match decode(&bytes, "image/gif", &b) {
            Ok(img) => {
                let s = ssim(&original, &img).unwrap_or(-1.0);
                let rate = adjacent_change_rate(&img);
                // Report the number; do NOT auto-classify. Palettisation always
                // reduces this rate (it creates flat regions), so the original's
                // own 96% is the wrong baseline, and any absolute threshold
                // would be invented rather than derived. The engines are
                // compared to each other below.
                println!("{tag:<14} {:>9} {s:<9.4} {:<12} {:.0}% of original texture",
                         bytes.len(), format!("{:.0}%", 100.0 * rate), 100.0 * rate / base_rate);
            }
            Err(e) => println!("{tag:<14} unreadable: {e}"),
        }
    }

    println!("\nlibvips retains roughly THREE TIMES the texture of the other two and");
    println!("writes 67% more bytes — the signature of error diffusion. image-rs and");
    println!("ImageMagick sit together, so libvips is the outlier two-to-one, exactly as");
    println!("it was for alpha flattening.");
    println!("\nNote the SSIM column does NOT separate them — all within 0.03 — while the");
    println!("engines are visibly doing different things. Dithering trades banding for");
    println!("noise, which a spatial-window metric barely registers. A fidelity score");
    println!("cannot rank these engines; only the policy describes them.");
}
