//! `huh clean` — the single unified command.
//!
//! Detect what a file really is, rebuild it from its parsed content, and report
//! what that destroyed. One entry point rather than a pile of tools the user has
//! to sequence themselves.
//!
//! Raster files go through the in-process path (`sanitize`). PDFs go through
//! external engines, because rendering a page is not something to reimplement:
//! poppler rasterises, libvips re-encodes, and a fresh PDF is built around the
//! result. Nothing structural survives that trip — which is the point, and also
//! the cost, since the text stops being text.
//!
//! Honest scope: this is a *reference implementation* for producing and checking
//! registry data (ADR-0011), not a replacement for Dangerzone, whose isolation is
//! considerably better (`RESEARCH.md §12.3b`).

use crate::engines::ConvertError;
use crate::sanitize::{self, SanitizeReport};
use crate::subprocess::{External, Scratch};
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct CleanReport {
    pub format: String,
    pub strategy: String,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub input_sha256: String,
    pub output_sha256: String,
    /// Engine invocations used, in order. Empty for the in-process path.
    pub invocations: Vec<String>,
    /// Present for the raster path, which can measure content equivalence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sanitize: Option<SanitizeReport>,
    /// Properties this rebuild destroyed.
    pub properties_lost: Vec<String>,
    /// Non-content data found and dropped, described in plain language.
    pub removed: Vec<String>,
    pub summary: String,
    pub wall_ms: u128,
}

/// What a PDF rebuild produced: the new bytes, the engine invocations used, and
/// a plain-language list of what was left behind.
type PdfCleanResult = (Vec<u8>, Vec<String>, Vec<String>);

/// Rebuild a PDF from rendered pages, discarding all original structure.
///
/// Two independent parsers are used deliberately (poppler to render, libvips to
/// re-encode). A payload that survives one parser's model of the file is
/// unlikely to survive a second that models it differently — and neither is
/// asked to preserve anything but pixels.
fn clean_pdf(input: &[u8], dpi: u32) -> Result<PdfCleanResult, ConvertError> {
    let scratch = Scratch::new("clean-pdf")?;
    let src = scratch.join("in.pdf");
    std::fs::write(&src, input).map_err(|e| ConvertError::Internal(format!("write input: {e}")))?;

    let mut invocations = Vec::new();

    // 1. Render to raster through poppler. Everything that is not a drawn pixel
    //    is left behind here: actions, scripts, embedded files, annotations.
    let render = External::new("pdftoppm")
        .arg("-r")
        .arg(dpi.to_string())
        .arg("-png")
        .path_arg(&src)
        .path_arg(&scratch.join("page"))
        .timeout(Duration::from_secs(120));
    invocations.push(render.describe());
    render.run()?;

    let mut pages: Vec<std::path::PathBuf> = std::fs::read_dir(scratch.path())
        .map_err(|e| ConvertError::Internal(format!("read scratch: {e}")))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|s| s.to_str()) == Some("png")
                && p.file_name().map(|n| n.to_string_lossy().starts_with("page")).unwrap_or(false)
        })
        .collect();
    pages.sort();
    if pages.is_empty() {
        return Err(ConvertError::InputMalformed(
            "no pages rendered — the document may be empty or unreadable".into(),
        ));
    }

    // 2. Re-encode each page through a DIFFERENT parser family.
    let mut jpegs = Vec::new();
    for page in &pages {
        let jpg = page.with_extension("jpg");
        let enc = External::new("vips")
            .arg("jpegsave")
            .path_arg(page)
            .path_arg(&jpg)
            .arg("--Q")
            .arg("90")
            .timeout(Duration::from_secs(60));
        if jpegs.is_empty() {
            invocations.push(enc.describe());
        }
        enc.run()?;
        jpegs.push(
            std::fs::read(&jpg).map_err(|e| ConvertError::Internal(format!("read page: {e}")))?,
        );
    }

    // 3. Build a fresh PDF. Nothing is copied from the original — every object
    //    here is constructed from scratch around the rendered images.
    let pdf = build_pdf(&jpegs)?;
    let removed = vec![
        format!("all PDF structure ({} page(s) rebuilt from pixels)", jpegs.len()),
        "document metadata (title, author, producer, creation tool)".into(),
        "any /OpenAction, /JavaScript, /EmbeddedFile or annotation".into(),
    ];
    Ok((pdf, invocations, removed))
}

/// Minimal PDF containing one image per page. No structure is inherited.
fn build_pdf(jpegs: &[Vec<u8>]) -> Result<Vec<u8>, ConvertError> {
    let mut objs: Vec<(usize, Vec<u8>)> = Vec::new();
    let page_count = jpegs.len();

    // Object numbering: 1 catalog, 2 pages, then 2 objects per page
    // (page dict, content stream) plus 1 image each.
    let kids: Vec<String> = (0..page_count).map(|i| format!("{} 0 R", 3 + i * 3)).collect();
    objs.push((1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()));
    objs.push((
        2,
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), page_count).into_bytes(),
    ));

    for (i, jpg) in jpegs.iter().enumerate() {
        let (w, h) = jpeg_dimensions(jpg)?;
        let page_id = 3 + i * 3;
        let content_id = page_id + 1;
        let image_id = page_id + 2;

        objs.push((
            page_id,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Contents {content_id} 0 R \
                 /Resources << /XObject << /Im0 {image_id} 0 R >> >> >>"
            )
            .into_bytes(),
        ));

        let content = format!("q {w} 0 0 {h} 0 0 cm /Im0 Do Q");
        let mut cs = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        cs.extend_from_slice(content.as_bytes());
        cs.extend_from_slice(b"\nendstream");
        objs.push((content_id, cs));

        let mut img = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            jpg.len()
        )
        .into_bytes();
        img.extend_from_slice(jpg);
        img.extend_from_slice(b"\nendstream");
        objs.push((image_id, img));
    }

    let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets = std::collections::BTreeMap::new();
    for (n, body) in &objs {
        offsets.insert(*n, out.len());
        out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    let max_id = objs.iter().map(|(n, _)| *n).max().unwrap_or(0);
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", max_id + 1).as_bytes());
    for n in 1..=max_id {
        match offsets.get(&n) {
            Some(o) => out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes()),
            None => out.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", max_id + 1)
            .as_bytes(),
    );
    Ok(out)
}

fn jpeg_dimensions(jpg: &[u8]) -> Result<(u32, u32), ConvertError> {
    let mut i = 2usize;
    while i + 9 < jpg.len() {
        if jpg[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = jpg[i + 1];
        if matches!(marker, 0xC0..=0xC2) {
            let h = u16::from_be_bytes([jpg[i + 5], jpg[i + 6]]) as u32;
            let w = u16::from_be_bytes([jpg[i + 7], jpg[i + 8]]) as u32;
            return Ok((w, h));
        }
        if (0xD8..=0xD9).contains(&marker) {
            i += 2;
            continue;
        }
        i += 2 + u16::from_be_bytes([jpg[i + 2], jpg[i + 3]]) as usize;
    }
    Err(ConvertError::Internal("no JPEG frame header".into()))
}

/// Clean `input`, which has already been identified as `format`.
pub fn clean(input: &[u8], format: &str, dpi: u32) -> Result<(Vec<u8>, CleanReport), ConvertError> {
    let started = std::time::Instant::now();

    let (output, strategy, invocations, sanitize_report, removed, properties_lost) = match format {
        "application/pdf" => {
            let (pdf, invocations, removed) = clean_pdf(input, dpi)?;
            (
                pdf,
                "render-and-rebuild".to_string(),
                invocations,
                None,
                removed,
                vec!["text_as_text".into(), "structure".into(), "interactivity".into(),
                     "metadata".into(), "fonts".into()],
            )
        }
        raster => {
            let (out, r) = sanitize::sanitize(input, raster, raster, 95)?;
            let removed = r
                .passengers
                .iter()
                .map(|p| format!("{} ({}, {} bytes) — {}", p.label, p.kind, p.bytes, p.note))
                .collect();
            let lost = r.properties_lost.clone();
            (out, "parse-and-reencode".to_string(), Vec::new(), Some(r), removed, lost)
        }
    };

    let summary = if removed.is_empty() {
        "No non-content data found; file rebuilt in canonical form.".to_string()
    } else {
        format!(
            "Rebuilt from parsed content. Removed {} item(s). Destroyed: {}.",
            removed.len(),
            if properties_lost.is_empty() { "nothing tracked".into() } else { properties_lost.join(", ") }
        )
    };

    let report = CleanReport {
        format: format.to_string(),
        strategy,
        input_bytes: input.len(),
        output_bytes: output.len(),
        input_sha256: crate::execute::sha256(input),
        output_sha256: crate::execute::sha256(&output),
        invocations,
        sanitize: sanitize_report,
        properties_lost,
        removed,
        summary,
        wall_ms: started.elapsed().as_millis(),
    };
    Ok((output, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specimen(rel: &str) -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join(rel);
        std::fs::read(p).expect("specimen")
    }

    fn have(prog: &str) -> bool {
        std::process::Command::new("which")
            .arg(prog)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn raster_path_strips_passengers_and_keeps_content() {
        let src = specimen("corpus/raster/png-with-passengers.png");
        let (out, r) = clean(&src, "image/png", 150).unwrap();
        assert_eq!(r.strategy, "parse-and-reencode");
        assert!(r.invocations.is_empty(), "raster path must not shell out");
        assert!(!out.windows(8).any(|w| w == b"TRAILER:"), "trailing data survived");
        assert!(r.sanitize.as_ref().unwrap().content_preserved);
    }

    #[test]
    fn pdf_path_removes_active_content() {
        // Skipped rather than failed where the engines are absent: a missing
        // engine is a capability gap, not a broken build.
        if !have("pdftoppm") || !have("vips") {
            eprintln!("skipping: pdftoppm/vips not installed");
            return;
        }
        let src = specimen("corpus/document/active-content.pdf");
        let (out, r) = clean(&src, "application/pdf", 150).unwrap();

        assert_eq!(r.strategy, "render-and-rebuild");
        assert_eq!(r.invocations.len(), 2, "should use two independent parsers");

        for marker in [
            &b"JavaScript"[..], b"OpenAction", b"PAYLOAD-EXECUTED",
            b"Alice Example", b"SECRET-WORKSTATION-07",
        ] {
            assert!(
                !out.windows(marker.len()).any(|w| w == marker),
                "`{}` survived cleaning",
                String::from_utf8_lossy(marker)
            );
        }

        // Rebuilding must be honest about its cost, or the report is worthless.
        assert!(r.properties_lost.contains(&"text_as_text".to_string()));
        assert!(out.starts_with(b"%PDF-"), "output is not a PDF");
    }

    #[test]
    fn report_records_every_invocation() {
        if !have("pdftoppm") || !have("vips") {
            return;
        }
        let src = specimen("corpus/document/active-content.pdf");
        let (_, r) = clean(&src, "application/pdf", 150).unwrap();
        // ADR-0013: an edge without its invocation is under-specified, and the
        // same applies to a receipt.
        assert!(r.invocations.iter().any(|i| i.starts_with("pdftoppm ")), "{:?}", r.invocations);
        assert!(r.invocations.iter().any(|i| i.starts_with("vips ")), "{:?}", r.invocations);
    }
}
