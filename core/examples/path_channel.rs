//! EXPERIMENT: does the conversion PATH leave a recoverable trace in the output?
//!
//! This is the load-bearing question for "a key based on the path of
//! conversions taken". Two outcomes, both decisive:
//!
//!   - If lossless paths produce byte-IDENTICAL output, the path is perfectly
//!     hidden by the output. Good for secrecy, but the path then carries zero
//!     information downstream — it cannot be a covert channel.
//!   - If they differ, the path is RECOVERABLE from the artifact. That kills
//!     path secrecy, and simultaneously creates a real signalling channel.
use huh_core::engines::{decode, encode, Budget};
use huh_core::execute::sha256;
use huh_core::fidelity::ssim;

fn run(src: &[u8], path: &[&str], b: &Budget) -> Option<Vec<u8>> {
    let mut cur = src.to_vec();
    let mut fmt = "image/png";
    for step in path {
        let img = decode(&cur, fmt, b).ok()?;
        cur = encode(&img, step, 85).ok()?;
        fmt = step;
    }
    // Land every route back on PNG so we compare like with like.
    let img = decode(&cur, fmt, b).ok()?;
    encode(&img, "image/png", 85).ok()
}

fn main() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("corpus/raster");
    let src = std::fs::read(corpus.join("rgba-gradient.png")).expect("specimen");
    let b = Budget::default();
    let original = decode(&src, "image/png", &b).unwrap();

    // Every one of these is lossless at every hop, per measurement.
    let lossless: Vec<Vec<&str>> = vec![
        vec!["image/png"],
        vec!["image/bmp", "image/png"],
        vec!["image/tiff", "image/png"],
        vec!["image/webp", "image/png"],
        vec!["image/bmp", "image/tiff", "image/png"],
        vec!["image/webp", "image/bmp", "image/tiff", "image/png"],
        vec!["image/tiff", "image/webp", "image/bmp", "image/png"],
    ];

    println!("=== Q1: do DIFFERENT LOSSLESS PATHS give DIFFERENT BYTES? ===");
    println!("{:<52} {:>7} {:<18} {}", "PATH", "BYTES", "SHA256[..16]", "SSIM");
    let mut digests = std::collections::BTreeMap::new();
    for p in &lossless {
        match run(&src, p, &b) {
            Some(out) => {
                let d = sha256(&out);
                let back = decode(&out, "image/png", &b).unwrap();
                let s = ssim(&original, &back).unwrap_or(-1.0);
                println!("{:<52} {:>7} {:<18} {:.4}",
                         p.join(" -> "), out.len(), &d[..16], s);
                digests.entry(d).or_insert_with(Vec::new).push(p.join(" -> "));
            }
            None => println!("{:<52} FAILED", p.join(" -> ")),
        }
    }
    println!("\ndistinct outputs: {} across {} lossless paths", digests.len(), lossless.len());
    if digests.len() == 1 {
        println!("=> The path is UNRECOVERABLE from the output. Lossless routes are");
        println!("   observationally identical. No covert channel; no path leak.");
    } else {
        println!("=> The path IS recoverable from the output. Groups:");
        for (d, ps) in &digests {
            println!("   {}  {:?}", &d[..12], ps);
        }
    }

    println!("\n=== Q2: does a LOSSY hop leave a trace? ===");
    let lossy: Vec<Vec<&str>> = vec![
        vec!["image/jpeg", "image/png"],
        vec!["image/gif", "image/png"],
        vec!["image/jpeg", "image/webp", "image/png"],
    ];
    for p in &lossy {
        if let Some(out) = run(&src, p, &b) {
            let back = decode(&out, "image/png", &b).unwrap();
            let s = ssim(&original, &back).unwrap_or(-1.0);
            println!("{:<52} {:>7} {:<18} {:.4}", p.join(" -> "), out.len(), &sha256(&out)[..16], s);
        }
    }
    println!("\n=> Lossy hops are trivially detectable: they change the PIXELS.");
    println!("   A path containing one is not hidden at all.");
}
