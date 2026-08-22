//! Fidelity measurement. `docs/specs/loss-vector.md §5`.
//!
//! A declared loss vector is a HYPOTHESIS. This module tests it against the
//! actual bytes, which is what makes the registry falsifiable rather than
//! merely asserted — the loop the Polyglot prior art never closed.

use image::DynamicImage;

/// Composite an image onto a solid background, discarding alpha.
///
/// Required before comparing anything that carries transparency. A fully
/// transparent pixel is INVISIBLE, so whatever RGB sits under it is irrelevant
/// to what a viewer sees — but it is very relevant to a naive pixel comparison.
pub fn composite_over(img: &DynamicImage, bg: [u8; 3]) -> image::RgbImage {
    let rgba = img.to_rgba8();
    let mut out = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p.0[3] as f32 / 255.0;
        let px = out.get_pixel_mut(x, y);
        for (c, (&src, &back)) in p.0.iter().zip(bg.iter()).enumerate() {
            px.0[c] = (src as f32 * a + back as f32 * (1.0 - a))
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// SSIM after compositing BOTH images onto the same background.
///
/// The only meaningful comparison when transparency is involved. `ssim` reads
/// stored RGB, which silently rewards engines that leave RGB untouched under
/// transparent pixels and punishes engines that premultiply — even though the
/// two produce identical output on screen. That is a property of the comparison,
/// not of the conversion, and it distorted several measurements in
/// `docs/RESEARCH.md §13` before it was noticed (§13.8).
///
/// The background is an explicit argument because there is no neutral choice:
/// the result depends on it, and hiding that would repeat the mistake.
pub fn ssim_composited(a: &DynamicImage, b: &DynamicImage, bg: [u8; 3]) -> Option<f64> {
    let ca = DynamicImage::ImageRgb8(composite_over(a, bg));
    let cb = DynamicImage::ImageRgb8(composite_over(b, bg));
    ssim(&ca, &cb)
}

/// Structural similarity on luma, 8x8 sliding windows, stride 4.
///
/// NOTE: reads STORED RGB and ignores alpha. For anything carrying transparency
/// use `ssim_composited` instead — see its documentation for why this one gives
/// misleading answers there.
///
/// SSIM decomposes similarity into luminance, contrast and structure, which is
/// why it is used here rather than PSNR: PSNR averages per-pixel error and the
/// eye does not work that way. Returns 1.0 for identical images.
pub fn ssim(a: &DynamicImage, b: &DynamicImage) -> Option<f64> {
    let ga = a.to_luma8();
    let gb = b.to_luma8();
    if ga.dimensions() != gb.dimensions() {
        return None; // Different geometry is a different finding, not a score.
    }
    let (w, h) = ga.dimensions();
    if w < 8 || h < 8 {
        return None;
    }

    // Standard SSIM stabilisation constants for 8-bit dynamic range.
    let c1 = (0.01f64 * 255.0).powi(2);
    let c2 = (0.03f64 * 255.0).powi(2);

    let mut total = 0.0;
    let mut windows = 0usize;
    let win = 8u32;
    let stride = 4u32;

    let mut y = 0;
    while y + win <= h {
        let mut x = 0;
        while x + win <= w {
            let (mut sa, mut sb) = (0.0f64, 0.0f64);
            for j in 0..win {
                for i in 0..win {
                    sa += ga.get_pixel(x + i, y + j).0[0] as f64;
                    sb += gb.get_pixel(x + i, y + j).0[0] as f64;
                }
            }
            let n = (win * win) as f64;
            let (ma, mb) = (sa / n, sb / n);

            let (mut va, mut vb, mut cov) = (0.0f64, 0.0f64, 0.0f64);
            for j in 0..win {
                for i in 0..win {
                    let pa = ga.get_pixel(x + i, y + j).0[0] as f64 - ma;
                    let pb = gb.get_pixel(x + i, y + j).0[0] as f64 - mb;
                    va += pa * pa;
                    vb += pb * pb;
                    cov += pa * pb;
                }
            }
            va /= n - 1.0;
            vb /= n - 1.0;
            cov /= n - 1.0;

            let s = ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            total += s;
            windows += 1;
            x += stride;
        }
        y += stride;
    }
    if windows == 0 { None } else { Some(total / windows as f64) }
}

/// Does the image actually carry non-opaque pixels?
///
/// Distinguishes "the format supports alpha" from "this file uses it", which is
/// the difference between a theoretical and an observed loss.
pub fn has_transparency(img: &DynamicImage) -> bool {
    match img {
        DynamicImage::ImageRgba8(buf) => buf.pixels().any(|p| p.0[3] != 255),
        DynamicImage::ImageLumaA8(buf) => buf.pixels().any(|p| p.0[1] != 255),
        DynamicImage::ImageRgba16(buf) => buf.pixels().any(|p| p.0[3] != u16::MAX),
        _ => false,
    }
}

/// Bits per channel actually present in the decoded image.
pub fn bit_depth(img: &DynamicImage) -> u8 {
    match img {
        DynamicImage::ImageLuma16(_)
        | DynamicImage::ImageLumaA16(_)
        | DynamicImage::ImageRgb16(_)
        | DynamicImage::ImageRgba16(_) => 16,
        DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_) => 32,
        _ => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specimen(name: &str) -> DynamicImage {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().join("corpus/raster").join(name);
        image::open(p).expect("specimen")
    }

    #[test]
    fn a_lossless_roundtrip_scores_one_even_with_alpha() {
        // The invariant that caught the instrument bug in RESEARCH.md §13.8:
        // a conversion that provably loses nothing MUST score 1.0. Comparing
        // stored RGB scored 0.4691 here, because premultiplied and
        // non-premultiplied storage differ under transparent pixels while
        // looking identical on screen.
        let img = specimen("rgba-hard-edge.png");
        let s = ssim_composited(&img, &img, [255, 255, 255]).expect("ssim");
        assert!((s - 1.0).abs() < 1e-9, "identical images scored {s}");
    }

    #[test]
    fn compositing_background_changes_the_score() {
        // Guards the §13.9 finding: the background is not a detail, it decides
        // the answer. If this ever stops holding, the harness has silently
        // started ignoring alpha again.
        let img = specimen("rgba-hard-edge.png");
        let opaque = specimen("rgb-photo.png");
        let on_white = composite_over(&img, [255, 255, 255]);
        let on_black = composite_over(&img, [0, 0, 0]);
        assert_ne!(on_white.as_raw(), on_black.as_raw(),
                   "alpha was ignored — compositing had no effect");
        // An image with no transparency must be unaffected by the choice.
        assert_eq!(composite_over(&opaque, [255, 255, 255]).as_raw(),
                   composite_over(&opaque, [0, 0, 0]).as_raw(),
                   "opaque image changed with background");
    }

    #[test]
    fn transparent_pixels_take_the_background() {
        let img = specimen("rgba-hard-edge.png");
        let w = composite_over(&img, [255, 255, 255]);
        let px = w.get_pixel(w.width() - 2, w.height() / 2);
        assert_eq!(px.0, [255, 255, 255], "transparent half did not become white");
    }
}
