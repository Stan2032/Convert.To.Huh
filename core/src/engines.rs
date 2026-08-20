//! Engine adapters.
//!
//! ADR-0003: engines are hostile and unreliable. Adapters build argument
//! vectors, never shell strings, and map failures to a taxonomy rather than
//! "non-zero exit". `image-rs` is in-process and memory-safe, so the sandbox
//! obligations that apply to FFmpeg or Ghostscript do not bite here — which is
//! exactly why it was chosen to go first.

use crate::fidelity;
use image::{DynamicImage, ImageFormat};
use std::io::Cursor;

/// The error taxonomy from `docs/specs/engine-manifest.md`.
#[derive(Debug)]
pub enum ConvertError {
    InputMalformed(String),
    InputUnsupported(String),
    BudgetExceeded(String),
    EngineCrash(String),
    Internal(String),
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertError::InputMalformed(m) => write!(f, "input_malformed: {m}"),
            ConvertError::InputUnsupported(m) => write!(f, "input_unsupported: {m}"),
            ConvertError::BudgetExceeded(m) => write!(f, "budget_exceeded: {m}"),
            ConvertError::EngineCrash(m) => write!(f, "engine_crash: {m}"),
            ConvertError::Internal(m) => write!(f, "internal: {m}"),
        }
    }
}

/// What the engine observed about the input. Feeds declared-vs-measured.
#[derive(Debug, Clone)]
pub struct Observed {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub has_alpha: bool,
}

pub struct Budget {
    pub max_input_bytes: u64,
    pub max_pixels: u64,
}

impl Default for Budget {
    fn default() -> Self {
        // Computed before execution and enforced during it (ADR-0003).
        // Exceeding is a normal reported outcome, not a crash.
        Budget { max_input_bytes: 512 * 1024 * 1024, max_pixels: 500_000_000 }
    }
}

fn fmt_of(id: &str) -> Option<ImageFormat> {
    match id {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" => Some(ImageFormat::Jpeg),
        "image/gif" => Some(ImageFormat::Gif),
        "image/bmp" => Some(ImageFormat::Bmp),
        "image/tiff" => Some(ImageFormat::Tiff),
        "image/webp" => Some(ImageFormat::WebP),
        _ => None,
    }
}

pub fn decode(bytes: &[u8], format_id: &str, budget: &Budget) -> Result<DynamicImage, ConvertError> {
    if bytes.len() as u64 > budget.max_input_bytes {
        return Err(ConvertError::BudgetExceeded(format!(
            "input is {} bytes, limit is {}",
            bytes.len(),
            budget.max_input_bytes
        )));
    }
    let f = fmt_of(format_id)
        .ok_or_else(|| ConvertError::InputUnsupported(format!("no image-rs decoder for {format_id}")))?;

    // Guard the pixel bomb before allocating: a small file can declare an
    // enormous grid (docs/RESEARCH.md §4.3).
    let reader = image::ImageReader::with_format(Cursor::new(bytes), f);
    if let Ok((w, h)) = reader.into_dimensions() {
        let px = w as u64 * h as u64;
        if px > budget.max_pixels {
            return Err(ConvertError::BudgetExceeded(format!(
                "{w}x{h} = {px} pixels, limit is {}",
                budget.max_pixels
            )));
        }
    }

    image::load_from_memory_with_format(bytes, f)
        .map_err(|e| ConvertError::InputMalformed(e.to_string()))
}

pub fn observe(img: &DynamicImage) -> Observed {
    Observed {
        width: img.width(),
        height: img.height(),
        bit_depth: fidelity::bit_depth(img),
        has_alpha: fidelity::has_transparency(img),
    }
}

/// Encode to the target format. `quality` applies to lossy targets only.
pub fn encode(img: &DynamicImage, format_id: &str, quality: u8) -> Result<Vec<u8>, ConvertError> {
    let mut out = Vec::new();
    match format_id {
        "image/jpeg" => {
            // JPEG has no alpha channel; compositing onto white is a decision,
            // not an accident, and it is what makes `alpha = lost` true.
            let rgb = img.to_rgb8();
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
            enc.encode(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
                .map_err(|e| ConvertError::EngineCrash(e.to_string()))?;
        }
        "image/gif" => {
            // GIF is palettised: at most 256 colours, and alpha is 1-bit.
            // The registry says `color_depth = degraded` for this reason, and
            // the fidelity harness confirms it against real pixels.
            let rgba = img.to_rgba8();
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            enc.encode(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
                .map_err(|e| ConvertError::EngineCrash(e.to_string()))?;
        }
        "image/png" | "image/bmp" | "image/tiff" | "image/webp" => {
            let f = fmt_of(format_id).unwrap();
            img.write_to(&mut Cursor::new(&mut out), f)
                .map_err(|e| ConvertError::EngineCrash(e.to_string()))?;
        }
        other => {
            return Err(ConvertError::InputUnsupported(format!(
                "no image-rs encoder for {other}"
            )))
        }
    }
    Ok(out)
}
