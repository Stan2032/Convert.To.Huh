//! Content-first format detection. ADR-0005: the extension is never
//! authoritative; it is a tie-breaker of last resort, recorded separately so a
//! mismatch is visible rather than silently trusted.

use crate::registry::Registry;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Detected {
    pub format: String,
    /// `signature` | `extension` | `unknown`
    pub detected_by: String,
    pub confidence: f64,
    /// Recorded separately from the detected identity on purpose (ADR-0005):
    /// an EPS carrying a `.jpg` extension is a real, in-the-wild attack.
    pub declared_extension: Option<String>,
    pub extension_mismatch: bool,
}

fn hex_matches(bytes: &[u8], offset: u64, magic_hex: &str) -> bool {
    let start = offset as usize;
    let n = magic_hex.len() / 2;
    if bytes.len() < start + n {
        return false;
    }
    for i in 0..n {
        let byte = match u8::from_str_radix(&magic_hex[i * 2..i * 2 + 2], 16) {
            Ok(b) => b,
            Err(_) => return false,
        };
        if bytes[start + i] != byte {
            return false;
        }
    }
    true
}

/// Detect a format from content, using the extension only to break ties.
pub fn detect(reg: &Registry, bytes: &[u8], filename: Option<&str>) -> Detected {
    let ext = filename
        .and_then(|f| f.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase());

    // Structural signatures first. Collect every format whose signatures all match.
    let mut hits: Vec<(&str, &str)> = Vec::new(); // (format id, strongest weight)
    for (id, f) in &reg.formats {
        if f.signatures.is_empty() {
            continue;
        }
        let all = f.signatures.iter().all(|s| hex_matches(bytes, s.offset, &s.magic));
        if !all {
            continue;
        }
        let weight = if f.signatures.iter().any(|s| s.weight == "decisive") {
            "decisive"
        } else if f.signatures.iter().any(|s| s.weight == "strong") {
            "strong"
        } else {
            "weak"
        };
        hits.push((id.as_str(), weight));
    }

    // PRONOM-style priority: a format that declares priority over another wins
    // when both match. Every DOCX is also a valid ZIP.
    let winners: Vec<&str> = hits
        .iter()
        .filter(|(id, _)| {
            !hits.iter().any(|(other, _)| {
                other != id
                    && reg.formats.get(*other)
                        .map(|f| f.detection.priority_over.iter().any(|p| p == id))
                        .unwrap_or(false)
            })
        })
        .map(|(id, _)| *id)
        .collect();

    let rank = |w: &str| match w { "decisive" => 2, "strong" => 1, _ => 0 };
    let best = winners
        .iter()
        .max_by_key(|id| hits.iter().find(|(h, _)| h == *id).map(|(_, w)| rank(w)).unwrap_or(0))
        .copied();

    if let Some(id) = best {
        let weight = hits.iter().find(|(h, _)| *h == id).map(|(_, w)| *w).unwrap_or("weak");
        let confidence = match weight { "decisive" => 1.0, "strong" => 0.85, _ => 0.5 };
        let mismatch = ext
            .as_ref()
            .map(|e| {
                reg.formats.get(id)
                    .map(|f| !f.extensions.iter().any(|x| x == e))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        return Detected {
            format: id.to_string(),
            detected_by: "signature".into(),
            confidence,
            declared_extension: ext,
            extension_mismatch: mismatch,
        };
    }

    // No signature matched. The extension is the last resort and says so.
    if let Some(e) = &ext {
        if let Some((id, _)) = reg.formats.iter().find(|(_, f)| f.extensions.iter().any(|x| x == e)) {
            return Detected {
                format: id.clone(),
                detected_by: "extension".into(),
                confidence: 0.3,
                declared_extension: ext.clone(),
                extension_mismatch: false,
            };
        }
    }

    Detected {
        format: "application/octet-stream".into(),
        detected_by: "unknown".into(),
        confidence: 0.0,
        declared_extension: ext,
        extension_mismatch: false,
    }
}
