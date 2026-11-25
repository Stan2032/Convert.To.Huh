//! Sanitization: rebuild a file from its parsed content, and report what did
//! not survive.
//!
//! ADR-0010. The experiment in `examples/path_channel.rs` showed that lossless
//! conversion is canonicalizing — seven different routes produced byte-identical
//! output. That property is useless for hiding information and ideal for
//! destroying it, which is the basis of Content Disarm and Reconstruction: parse
//! the content, discard the container, write a fresh one. Anything that was not
//! part of the content does not make the trip.
//!
//! What distinguishes this from existing CDR tools is the report. Commercial CDR
//! sanitizes and tells you it succeeded; the documented complaint is that output
//! is sometimes unusable and you cannot tell why (R§12). Here the loss vector
//! already says what a conversion destroys, so the sanitizer can hand back an
//! itemised account and a proof the content survived.

use crate::engines::{self, Budget, ConvertError};
use crate::fidelity;
use serde::Serialize;

/// One thing that was carried by the input and is not in the output.
#[derive(Debug, Clone, Serialize)]
pub struct Passenger {
    /// `ancillary_chunk` | `trailing_data` | `container_slack`
    pub kind: String,
    /// Format-specific label, e.g. a PNG chunk type.
    pub label: String,
    pub bytes: usize,
    /// Why a decoder never showed this to anyone.
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SanitizeReport {
    pub format: String,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub bytes_removed: i64,
    /// Non-content data found in the input and dropped.
    pub passengers: Vec<Passenger>,
    pub passenger_bytes: usize,
    /// Content equivalence proof. 1.0 means the pixels are untouched.
    pub content_ssim: Option<f64>,
    pub content_preserved: bool,
    /// Properties the rebuild destroyed, from the engine's own observation.
    pub properties_lost: Vec<String>,
    pub input_sha256: String,
    pub output_sha256: String,
    /// Generated, not hand-written.
    pub summary: String,
}

/// Walk PNG chunk structure to name what is riding along.
///
/// Ancillary chunks (lowercase first byte, per the PNG spec's case convention)
/// are optional by definition: a decoder that does not recognise one skips it.
/// That is exactly what makes them a convenient place to keep things.
fn png_passengers(bytes: &[u8]) -> Vec<Passenger> {
    let mut out = Vec::new();
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return out;
    }
    let mut i = 8usize;
    while i + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
        let tag = &bytes[i + 4..i + 8];
        let name = String::from_utf8_lossy(tag).to_string();

        // The declared length is attacker-controlled and is not yet known to be
        // real: this runs BEFORE any decode, so a crafted file reaches it.
        //
        // On any 32-bit target `usize` is 32 bits, and a declared length of
        // 0xFFFFFFF4 makes `12 + len` wrap to exactly 0 — the cursor stops
        // advancing and the loop never ends. In a debug build it panics first.
        // A length merely larger than the file is milder but still wrong: it
        // inflates `passenger_bytes` past the size of the input.
        let Some(next) = i.checked_add(12).and_then(|n| n.checked_add(len)) else {
            break;
        };
        if next > bytes.len() && tag != b"IEND" {
            break;
        }

        // Critical chunks are structure; ancillary chunks are cargo.
        let ancillary = tag[0].is_ascii_lowercase();
        if ancillary {
            let note = match &name[..] {
                "tEXt" | "zTXt" | "iTXt" => {
                    "free-form text metadata; displayed by nothing, copied by everything"
                }
                "eXIf" => "EXIF block: may carry camera serial, GPS coordinates, timestamps",
                "tIME" => "modification timestamp",
                _ => "private or unrecognised ancillary chunk; decoders skip it silently",
            };
            out.push(Passenger {
                kind: "ancillary_chunk".into(),
                label: name.clone(),
                bytes: len.saturating_add(12), // length + type + data + CRC
                note: note.into(),
            });
        }

        i = next;
        if tag == b"IEND" {
            // Everything past the end-of-image marker is invisible to decoders
            // and preserved by any byte-level copy. A file can be entirely
            // valid and still carry an arbitrary payload here.
            if i < bytes.len() {
                out.push(Passenger {
                    kind: "trailing_data".into(),
                    label: "after IEND".into(),
                    bytes: bytes.len() - i,
                    note: "appended past end-of-image; structurally invisible, fully persistent"
                        .into(),
                });
            }
            break;
        }
    }
    out
}

/// Exact equality of decoded content: geometry and every RGBA sample.
///
/// Compared as RGBA8 so that a change in any channel counts, including one an
/// alpha-ignoring or luma-only comparison would miss. Two images that differ
/// only in how they were encoded are equal here, which is the intent — this
/// asks whether the CONTENT survived the rebuild, not whether the bytes did.
fn pixels_identical(a: &image::DynamicImage, b: &image::DynamicImage) -> bool {
    if a.width() != b.width() || a.height() != b.height() {
        return false;
    }
    a.to_rgba8().as_raw() == b.to_rgba8().as_raw()
}

fn passengers_for(format: &str, bytes: &[u8]) -> Vec<Passenger> {
    match format {
        "image/png" => png_passengers(bytes),
        _ => Vec::new(),
    }
}

/// Rebuild `bytes` in canonical form for `format`, reporting what was dropped.
///
/// The output format defaults to the input format: sanitizing should not force
/// a format change on the user. Passing a different `target` is allowed and is
/// sometimes stronger, since a format switch cannot preserve container quirks
/// even accidentally.
pub fn sanitize(
    bytes: &[u8],
    format: &str,
    target: &str,
    quality: u8,
) -> Result<(Vec<u8>, SanitizeReport), ConvertError> {
    let budget = Budget::default();

    let found = passengers_for(format, bytes);
    let passenger_bytes: usize = found.iter().map(|p| p.bytes).sum();

    // Parse to content, then write a fresh container. Nothing is copied across;
    // the output is constructed from decoded pixels alone.
    let img = engines::decode(bytes, format, &budget)?;
    let before = engines::observe(&img);
    let out = engines::encode(&img, target, quality)?;

    // Prove the content survived. Sanitizing by destroying the file would score
    // perfectly on "no passengers remain" and be worthless.
    let rebuilt = engines::decode(&out, target, &budget)?;
    let after = engines::observe(&rebuilt);
    // Equivalence, not similarity-under-one-presentation: a sanitizer must not
    // report "content preserved" on the strength of a background it picked
    // itself (RESEARCH.md §13.9).
    let content_ssim = fidelity::content_equivalence(&img, &rebuilt);

    let mut properties_lost = Vec::new();
    if !found.is_empty() {
        properties_lost.push("metadata".to_string());
    }
    if before.has_alpha && !after.has_alpha {
        properties_lost.push("alpha".to_string());
    }
    if after.bit_depth < before.bit_depth {
        properties_lost.push("color_depth".to_string());
    }

    // `content_preserved` is a claim of EXACT equality, so it is decided by
    // exact equality — not by an SSIM threshold.
    //
    // SSIM is a perceptual similarity metric computed on luma, so it is blind
    // to changes that leave luminance alone (some chroma shifts) and can exceed
    // 0.9999 on images that genuinely differ. Deciding this flag from it, and
    // then calling the result "bit-faithful", is a stronger claim than the
    // measurement supports — the same overreach ADR-0010 records for
    // losslessness, in the one place a sanitizer must not overreach: telling
    // someone their content survived.
    //
    // SSIM stays, reported as what it is: a similarity score, useful for saying
    // HOW MUCH a lossy rebuild moved when the answer is not "not at all".
    let content_preserved = pixels_identical(&img, &rebuilt);

    let summary = {
        let mut parts = Vec::new();
        if found.is_empty() {
            parts.push("No non-content data found.".to_string());
        } else {
            parts.push(format!(
                "Removed {} passenger{} totalling {} bytes",
                found.len(),
                if found.len() == 1 { "" } else { "s" },
                passenger_bytes
            ));
        }
        // Say which question was answered. "Every pixel is identical" is an
        // exact claim; an SSIM score is a similarity claim, and the two must not
        // be worded as if they were the same evidence.
        if content_preserved {
            parts.push("every pixel is identical".into());
        } else {
            match content_ssim {
                Some(s) => parts.push(format!("image content changed (SSIM {s:.4})")),
                None => parts.push("content could not be compared — geometry changed".into()),
            }
        }
        format!("{}.", parts.join("; "))
    };

    let report = SanitizeReport {
        format: format.to_string(),
        input_bytes: bytes.len(),
        output_bytes: out.len(),
        bytes_removed: bytes.len() as i64 - out.len() as i64,
        passenger_bytes,
        passengers: found,
        content_ssim,
        content_preserved,
        properties_lost,
        input_sha256: crate::execute::sha256(bytes),
        output_sha256: crate::execute::sha256(&out),
        summary,
    };
    Ok((out, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specimen(name: &str) -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("corpus/raster")
            .join(name);
        std::fs::read(p).expect("specimen")
    }

    #[test]
    fn strips_passengers_and_keeps_content() {
        let src = specimen("png-with-passengers.png");
        let (out, r) = sanitize(&src, "image/png", "image/png", 85).unwrap();

        // Every passenger the specimen carries must be found.
        assert!(r.passengers.iter().any(|p| p.label == "tEXt"), "{:?}", r.passengers);
        assert!(r.passengers.iter().any(|p| p.label == "prVt"), "{:?}", r.passengers);
        assert!(
            r.passengers.iter().any(|p| p.kind == "trailing_data"),
            "{:?}",
            r.passengers
        );

        // And none may survive into the output.
        assert!(!out.windows(8).any(|w| w == b"TRAILER:"), "trailing data survived");
        assert!(
            !out.windows(9).any(|w| w == b"PASSENGER"),
            "private chunk payload survived"
        );
        assert!(
            !out.windows(5).any(|w| w == b"Alice"),
            "text metadata survived"
        );

        // Sanitizing by wrecking the image would be trivially "safe" and useless.
        assert!(r.content_preserved, "content not preserved: {:?}", r.content_ssim);
    }

    #[test]
    fn sanitizing_is_idempotent() {
        // A canonical form must be a fixed point, or the output is not canonical.
        let src = specimen("png-with-passengers.png");
        let (once, _) = sanitize(&src, "image/png", "image/png", 85).unwrap();
        let (twice, r2) = sanitize(&once, "image/png", "image/png", 85).unwrap();
        assert_eq!(once, twice, "second pass changed the bytes");
        assert!(r2.passengers.is_empty(), "clean file reported passengers");
    }

    #[test]
    fn a_lying_chunk_length_neither_hangs_nor_overcounts() {
        // 0xFFFFFFF4 is the value that makes `12 + len` wrap to 0 on a 32-bit
        // usize, as on any 32-bit target. The scan must stop,
        // not spin, and must not claim more passenger bytes than the file has.
        for declared in [0xFFFF_FFF4u32, 0xFFFF_FFFF, 5_000_000] {
            let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
            png.extend_from_slice(&declared.to_be_bytes());
            png.extend_from_slice(b"tEXt");
            png.extend_from_slice(b"short");

            let found = png_passengers(&png);
            let total: usize = found.iter().map(|p| p.bytes).sum();
            assert!(
                total <= png.len(),
                "declared {declared:#x}: claimed {total} passenger bytes from a \
                 {}-byte file",
                png.len()
            );
        }
    }

    #[test]
    fn truncated_png_is_scanned_without_panicking() {
        // Every prefix of a real specimen: no slice may go out of bounds.
        let src = specimen("png-with-passengers.png");
        for cut in (0..src.len()).step_by(7) {
            let _ = png_passengers(&src[..cut]);
        }
    }

    #[test]
    fn clean_file_reports_nothing_removed() {
        let src = specimen("rgb-photo.png");
        let (_, r) = sanitize(&src, "image/png", "image/png", 85).unwrap();
        assert!(r.passengers.is_empty(), "{:?}", r.passengers);
        assert!(r.content_preserved);
    }
}
