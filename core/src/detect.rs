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

/// Names of the streams and storages directly under an OLE2 file's root.
///
/// Root level only. A Word document with an embedded Excel chart carries a
/// `Workbook` stream inside its `ObjectPool` storage; counting nested names
/// made such a file match Excel as well (Govdocs1 098807.doc, RESEARCH §19.1).
///
/// Runs on untrusted bytes before anything else has looked at them, so every
/// offset is bounds-checked and every chain walk is capped at the number of
/// sectors the file can hold: a FAT cycle ends the walk instead of the process.
/// Returns an empty list for anything that is not a well-formed OLE2 header.
pub fn ole2_names(b: &[u8]) -> Vec<String> {
    const MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    const END: u32 = 0xFFFF_FFFE;
    let u16_at = |o: usize| b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]));
    let u32_at = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
    if b.len() < 512 || b[..8] != MAGIC {
        return Vec::new();
    }
    let Some(shift) = u16_at(0x1E) else { return Vec::new() };
    if !(7..=16).contains(&shift) {
        return Vec::new();
    }
    let ss = 1usize << shift;
    let max_sectors = b.len() / ss + 1;
    let sector = |n: u32| -> Option<&[u8]> {
        let start = (n as usize).checked_add(1)?.checked_mul(ss)?;
        b.get(start..start.checked_add(ss)?)
    };

    // FAT sector numbers: 109 in the header, the rest in a DIFAT chain.
    let mut fat_sectors: Vec<u32> = (0..109)
        .filter_map(|i| u32_at(0x4C + i * 4))
        .filter(|&n| n < END)
        .collect();
    let mut difat = u32_at(0x44).unwrap_or(END);
    let mut guard = 0;
    while difat < END && guard < max_sectors {
        let Some(sec) = sector(difat) else { break };
        let per = ss / 4 - 1;
        fat_sectors.extend((0..per)
            .map(|i| u32::from_le_bytes(sec[i * 4..i * 4 + 4].try_into().unwrap()))
            .filter(|&n| n < END));
        difat = u32::from_le_bytes(sec[per * 4..per * 4 + 4].try_into().unwrap());
        guard += 1;
    }
    let fat: Vec<u32> = fat_sectors.iter()
        .filter_map(|&n| sector(n))
        .flat_map(|sec| sec.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)))
        .collect();

    // Collect the directory: 128-byte entries along the directory chain.
    let mut entries: Vec<&[u8]> = Vec::new();
    let mut next = u32_at(0x30).unwrap_or(END);
    let mut steps = 0;
    while next < END && steps < max_sectors {
        let Some(sec) = sector(next) else { break };
        entries.extend(sec.as_chunks::<128>().0.iter().map(|e| e.as_slice()));
        next = *fat.get(next as usize).unwrap_or(&END);
        steps += 1;
    }
    let link = |e: &[u8], o: usize| u32::from_le_bytes(e[o..o + 4].try_into().unwrap());
    let name = |e: &[u8]| -> Option<String> {
        let len = u16::from_le_bytes([e[64], e[65]]) as usize;
        if !(2..=64).contains(&len) || e[66] == 0 {
            return None;
        }
        let units: Vec<u16> = e[..len - 2].as_chunks::<2>().0.iter()
            .map(|c| u16::from_le_bytes(*c)).collect();
        Some(String::from_utf16_lossy(&units))
    };

    // The root's children form a tree linked by left (68) and right (72)
    // siblings; its `child` (76) is the tree's top. Walk siblings only, never
    // into a child storage. Each entry is visited at most once.
    let mut names = Vec::new();
    let Some(root) = entries.first() else { return names };
    let mut stack = vec![link(root, 76)];
    let mut seen = vec![false; entries.len()];
    while let Some(i) = stack.pop() {
        let Some(e) = entries.get(i as usize) else { continue };
        if std::mem::replace(&mut seen[i as usize], true) {
            continue;
        }
        if let Some(n) = name(e) {
            names.push(n);
        }
        stack.push(link(e, 68));
        stack.push(link(e, 72));
    }
    names
}

/// Detect a format from content, using the extension only to break ties.
pub fn detect(reg: &Registry, bytes: &[u8], filename: Option<&str>) -> Detected {
    let ext = filename
        .and_then(|f| f.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase());

    // Structural signatures first. Collect every format whose signatures all match.
    let mut hits: Vec<(&str, &str)> = Vec::new(); // (format id, strongest weight)
    let mut ole2: Option<Vec<String>> = None; // parsed once, only if a format asks
    for (id, f) in &reg.formats {
        if f.signatures.is_empty() {
            continue;
        }
        // Group signatures: same group = OR, different groups = AND.
        // Ungrouped signatures are each their own group (i.e. required).
        let mut groups: std::collections::BTreeMap<i64, Vec<&crate::registry::Signature>> =
            std::collections::BTreeMap::new();
        for (idx, sig) in f.signatures.iter().enumerate() {
            let key = match sig.group {
                Some(g) => g as i64,
                None => -(idx as i64 + 1),
            };
            groups.entry(key).or_default().push(sig);
        }
        let all = groups
            .values()
            .all(|alts| alts.iter().any(|s| hex_matches(bytes, s.offset, &s.magic)));
        if !all {
            continue;
        }
        // A shared container signature says only "OLE2". The format is
        // identified by a stream it declares, or it does not match at all.
        if !f.detection.ole2_streams.is_empty() {
            let names = ole2.get_or_insert_with(|| ole2_names(bytes));
            if !f.detection.ole2_streams.iter().any(|want| names.iter().any(|n| n == want)) {
                continue;
            }
        }
        let matched: Vec<&crate::registry::Signature> = f
            .signatures
            .iter()
            .filter(|s| hex_matches(bytes, s.offset, &s.magic))
            .collect();
        let weight = if matched.iter().any(|s| s.weight == "decisive") {
            "decisive"
        } else if matched.iter().any(|s| s.weight == "strong") {
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

#[cfg(test)]
mod ole2_tests {
    use super::ole2_names;

    const END: u32 = 0xFFFF_FFFE;
    const FREE: u32 = 0xFFFF_FFFF;

    /// A minimal OLE2 file: header, one FAT sector (0), one directory sector
    /// (1). Directory: Root -> {WordDocument, ObjectPool -> {Workbook}}, the
    /// shape of a Word document carrying an embedded Excel object.
    fn word_with_embedded_workbook(dir_next: u32) -> Vec<u8> {
        let mut b = vec![0u8; 512 * 3];
        b[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
        b[0x1A..0x1C].copy_from_slice(&3u16.to_le_bytes());
        b[0x1C..0x1E].copy_from_slice(&0xFFFEu16.to_le_bytes());
        b[0x1E..0x20].copy_from_slice(&9u16.to_le_bytes()); // 512-byte sectors
        b[0x2C..0x30].copy_from_slice(&1u32.to_le_bytes()); // one FAT sector
        b[0x30..0x34].copy_from_slice(&1u32.to_le_bytes()); // directory at sector 1
        b[0x44..0x48].copy_from_slice(&END.to_le_bytes()); // no DIFAT chain
        for i in 0..109 {
            let v = if i == 0 { 0 } else { FREE };
            b[0x4C + i * 4..0x50 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        // FAT (sector 0 at offset 512): sector 0 is FAT, sector 1 -> dir_next.
        b[512..516].copy_from_slice(&0xFFFF_FFFDu32.to_le_bytes());
        b[516..520].copy_from_slice(&dir_next.to_le_bytes());
        // Directory (sector 1 at offset 1024).
        let entry = |b: &mut Vec<u8>, i: usize, name: &str, kind: u8, l: u32, r: u32, c: u32| {
            let o = 1024 + i * 128;
            let units: Vec<u16> = name.encode_utf16().collect();
            for (k, u) in units.iter().enumerate() {
                b[o + k * 2..o + k * 2 + 2].copy_from_slice(&u.to_le_bytes());
            }
            b[o + 64..o + 66].copy_from_slice(&(((units.len() + 1) * 2) as u16).to_le_bytes());
            b[o + 66] = kind;
            b[o + 68..o + 72].copy_from_slice(&l.to_le_bytes());
            b[o + 72..o + 76].copy_from_slice(&r.to_le_bytes());
            b[o + 76..o + 80].copy_from_slice(&c.to_le_bytes());
        };
        entry(&mut b, 0, "Root Entry", 5, FREE, FREE, 1);
        entry(&mut b, 1, "WordDocument", 2, FREE, 2, FREE);
        entry(&mut b, 2, "ObjectPool", 1, FREE, FREE, 3);
        entry(&mut b, 3, "Workbook", 2, FREE, FREE, FREE);
        b
    }

    #[test]
    fn only_root_level_names_count_not_embedded_objects() {
        let mut names = ole2_names(&word_with_embedded_workbook(END));
        names.sort();
        assert_eq!(names, ["ObjectPool", "WordDocument"],
                   "an embedded Workbook must not make a Word file look like Excel");
    }

    #[test]
    fn a_directory_chain_that_loops_terminates() {
        // Sector 1's FAT entry points back at sector 1.
        let mut names = ole2_names(&word_with_embedded_workbook(1));
        names.sort();
        names.dedup();
        assert_eq!(names, ["ObjectPool", "WordDocument"]);
    }

    #[test]
    fn truncated_or_foreign_input_yields_nothing() {
        let full = word_with_embedded_workbook(END);
        assert!(ole2_names(&full[..600]).is_empty(), "directory sector is cut off");
        assert!(ole2_names(&full[..100]).is_empty());
        assert!(ole2_names(b"%PDF-1.7 not an OLE file at all, but long enough? no").is_empty());
        let mut bad_shift = full.clone();
        bad_shift[0x1E] = 40;
        assert!(ole2_names(&bad_shift).is_empty());
    }
}
