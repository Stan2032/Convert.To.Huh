//! Does the comparison background decide which engine "wins"?
//!
//! RESEARCH.md §13.3 warned that a fidelity metric encodes a preference. This
//! demonstrates it rather than asserting it: the same two conversions, scored
//! against the same original, twice — once composited onto white, once onto
//! black. If the ranking flips, the number was never measuring engine quality.
use huh_core::engines::{decode, encode, Budget};
use huh_core::fidelity::ssim_composited;
use huh_core::subprocess::{External, Scratch};

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let b = Budget::default();
    let scratch = Scratch::new("bg").unwrap();

    println!("PNG -> JPEG, alpha lost. Same files, two comparison backgrounds.\n");
    println!("{:<20} {:>12} {:>12}   {:>12} {:>12}",
             "SPECIMEN", "vips/white", "irs/white", "vips/black", "irs/black");

    for name in ["rgba-gradient", "rgba-hard-edge"] {
        let spec = root.join(format!("corpus/raster/{name}.png"));
        let src = std::fs::read(&spec).unwrap();
        let original = decode(&src, "image/png", &b).unwrap();

        let vout = scratch.join(&format!("{name}.jpg"));
        External::new("vips").arg("jpegsave").path_arg(&spec).path_arg(&vout)
            .arg("--Q").arg("85").run().unwrap();
        let vimg = decode(&std::fs::read(&vout).unwrap(), "image/jpeg", &b).unwrap();
        let iimg = decode(&encode(&original, "image/jpeg", 85).unwrap(), "image/jpeg", &b).unwrap();

        let w = [255u8, 255, 255];
        let k = [0u8, 0, 0];
        println!("{name:<20} {:>12.4} {:>12.4}   {:>12.4} {:>12.4}",
                 ssim_composited(&original, &vimg, w).unwrap_or(-1.0),
                 ssim_composited(&original, &iimg, w).unwrap_or(-1.0),
                 ssim_composited(&original, &vimg, k).unwrap_or(-1.0),
                 ssim_composited(&original, &iimg, k).unwrap_or(-1.0));
    }

    println!("\nIf the ranking flips between the two halves, the metric was scoring");
    println!("the CHOICE OF BACKGROUND, not the engines. Report the policy, not a winner.");
}
