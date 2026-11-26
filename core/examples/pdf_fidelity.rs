//! How faithfully does `huh clean` rebuild a PDF's pages, visually?
//!
//!   cargo run --release --example pdf_fidelity -- corpus/govdocs1/files OUT.json [DPI]
//!
//! `huh clean` renders every page to pixels and builds a new PDF around them.
//! It is verified by the ABSENCE of active content; nothing measured whether
//! the rebuilt pages look like the originals. Two numbers per page, because a
//! single one conflated two different things (docs/RESEARCH.md §18.4):
//!
//!   stored  the page image embedded in the cleaned PDF (pdfimages) against
//!           the original rendered at the clean's own resolution. Isolates
//!           what `huh clean` does: in practice, JPEG at quality 90.
//!   viewed  both PDFs rendered at a screen resolution (default 72 dpi) and
//!           compared. Adds the viewer resampling a raster page that a
//!           vector original never needed. Real for a reader, but it depends
//!           on the renderer as much as on the clean.
//!
//! Re-rendering the cleaned PDF at 150 dpi, the obvious first design, scored
//! a page 0.87 whose stored image scores 0.995: the renderer resamples the
//! embedded image even at nominally 1:1 scale. That is the viewer, not the
//! clean, so it is reported as `viewed`, not as rebuild fidelity.
use huh_core::{clean, detect, fidelity, registry::Registry};
use serde_json::json;
use std::path::Path;
use std::process::Command;

const CLEAN_DPI: u32 = 150;

/// Rebuilding from pixels rounds each page to whole pixels at CLEAN_DPI, so
/// a 595 pt page comes back 595.2 pt and renders one pixel wider. Pages that
/// differ by no more than this are compared over their common area, and the
/// size change is reported separately rather than hidden.
const ROUNDING_PX: u32 = 2;

fn render(pdf: &Path, dir: &Path, prefix: &str, dpi: u32) -> Vec<image::DynamicImage> {
    let out = dir.join(prefix);
    let st = Command::new("pdftoppm")
        .args(["-r", &dpi.to_string(), "-gray", "-png"])
        .arg(pdf)
        .arg(&out)
        .status()
        .expect("pdftoppm runs");
    if !st.success() {
        return vec![];
    }
    let mut pages: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(&format!("{prefix}-")))
        .collect();
    pages.sort();
    pages.iter().filter_map(|p| image::open(p).ok()).collect()
}

/// Page images embedded in a PDF, in page order (one per page for `huh clean`).
fn embedded(pdf: &Path, dir: &Path) -> Vec<image::DynamicImage> {
    let st = Command::new("pdfimages").args(["-png"]).arg(pdf).arg(dir.join("e")).status();
    if !st.map(|s| s.success()).unwrap_or(false) {
        return vec![];
    }
    let mut imgs: Vec<_> = std::fs::read_dir(dir).unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("e-"))
        .collect();
    imgs.sort();
    imgs.iter().filter_map(|p| image::open(p).ok()).collect()
}

/// SSIM over the common area when sizes differ only by rounding.
fn compare(x: &image::DynamicImage, y: &image::DynamicImage) -> Result<(f64, bool), ()> {
    let (w, h) = (x.width().min(y.width()), x.height().min(y.height()));
    let off = x.width().abs_diff(y.width()).max(x.height().abs_diff(y.height()));
    if off > ROUNDING_PX {
        return Err(());
    }
    fidelity::ssim(&x.crop_imm(0, 0, w, h), &y.crop_imm(0, 0, w, h))
        .map(|s| (s, off > 0)).ok_or(())
}

fn summary(mut v: Vec<f64>) -> serde_json::Value {
    v.sort_by(|p, q| p.partial_cmp(q).unwrap());
    let r = |x: f64| (x * 1e4).round() / 1e4;
    json!({"pages": v.len(), "min": v.first().map(|x| r(*x)), "median": v.get(v.len() / 2).map(|x| r(*x))})
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, out, dpi) = match args.as_slice() {
        [d, o] => (Path::new(d), Path::new(o), 72),
        [d, o, r] => (Path::new(d), Path::new(o), r.parse().expect("DPI")),
        _ => {
            eprintln!("usage: pdf_fidelity <specimen-dir> <out.json> [compare-dpi]");
            std::process::exit(2);
        }
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let reg = Registry::load(root).expect("registry loads");
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.sort();

    let mut rows = vec![];
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if detect::detect(&reg, &bytes, Some(&name)).format != "application/pdf" {
            continue;
        }
        let work = std::env::temp_dir().join(format!("huh-pdffid-{}-{}", std::process::id(), name));
        std::fs::create_dir_all(&work).unwrap();
        let row = match clean::clean(&bytes, "application/pdf", CLEAN_DPI) {
            Err(e) => json!({"file": name, "error": e.to_string()}),
            Ok((cleaned, _)) => {
                let a = work.join("orig.pdf");
                let b = work.join("clean.pdf");
                std::fs::write(&a, &bytes).unwrap();
                std::fs::write(&b, &cleaned).unwrap();
                let orig_hi = render(&a, &work, "s", CLEAN_DPI);
                let stored_imgs = embedded(&b, &work);
                let pa = render(&a, &work, "o", dpi);
                let pb = render(&b, &work, "c", dpi);
                let (mut stored, mut viewed) = (vec![], vec![]);
                let (mut size_mismatch, mut resized) = (0, 0);
                for (x, y) in orig_hi.iter().zip(stored_imgs.iter()) {
                    match compare(x, y) {
                        Ok((s, _)) => stored.push(s),
                        Err(()) => size_mismatch += 1,
                    }
                }
                for (x, y) in pa.iter().zip(pb.iter()) {
                    match compare(x, y) {
                        Ok((s, r)) => { viewed.push(s); resized += usize::from(r); }
                        Err(()) => size_mismatch += 1,
                    }
                }
                json!({
                    "file": name,
                    "pages_original": pa.len(),
                    "pages_cleaned": pb.len(),
                    "page_images_embedded": stored_imgs.len(),
                    "pages_resized_by_rounding": resized,
                    "page_size_mismatch": size_mismatch,
                    "stored": summary(stored),
                    "viewed": summary(viewed),
                })
            }
        };
        std::fs::remove_dir_all(&work).ok();
        eprintln!("{row}");
        rows.push(row);
    }
    std::fs::write(out, serde_json::to_string_pretty(&json!({
        "clean_dpi": CLEAN_DPI, "compare_dpi": dpi, "files": rows,
    })).unwrap() + "\n").unwrap();
}
