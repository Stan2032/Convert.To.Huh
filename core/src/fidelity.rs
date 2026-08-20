//! Fidelity measurement. `docs/specs/loss-vector.md §5`.
//!
//! A declared loss vector is a HYPOTHESIS. This module tests it against the
//! actual bytes, which is what makes the registry falsifiable rather than
//! merely asserted — the loop the Polyglot prior art never closed.

use image::DynamicImage;

/// Structural similarity on luma, 8x8 sliding windows, stride 4.
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
