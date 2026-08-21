//! Export the registry as a public dataset.
//!
//! ADR-0011: the registry is the deliverable. Nothing publishes machine-readable
//! conversion-loss data — PRONOM says what a file *is*, and nothing says what
//! converting it *costs* (`docs/RESEARCH.md §12.3c`). This module emits that
//! data in a form other projects can consume without depending on this one.
//!
//! The public schema is deliberately **decoupled from the internal structs**.
//! Internal representation should be free to change; the published contract
//! should not. Anything emitted here is a promise to downstream consumers.

use crate::loss::PROPERTIES;
use crate::registry::Registry;
use serde::Serialize;

/// Bump on any breaking change to the emitted shape. Consumers pin this.
pub const SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Serialize)]
pub struct PropertyDoc {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct FormatOut {
    pub id: String,
    pub name: String,
    pub kinds: Vec<String>,
    pub extensions: Vec<String>,
    pub mime_types: Vec<String>,
    /// Which tracked properties this format is capable of carrying.
    pub preserves: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub puid: Option<String>,
    /// Whether the PUID was checked against PRONOM's live API, or merely recorded.
    pub puid_verified: bool,
}

#[derive(Debug, Serialize)]
pub struct EngineOut {
    pub id: String,
    pub name: String,
    pub version_pin: String,
    pub license: String,
    /// Which distribution profiles may legally ship this engine.
    pub build_profiles: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct EdgeOut {
    pub from: String,
    pub to: String,
    pub from_kind: String,
    pub to_kind: String,
    pub engine: String,
    pub operation: String,
    /// Per-property outcome. Absent property = not applicable to this conversion.
    pub loss: std::collections::BTreeMap<String, String>,
    /// What REPLACED a lost property, where engines differ (ADR-0012).
    /// `alpha: composite_black` and `alpha: discard_channel` are both `lost`
    /// and produce visually incompatible output.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub loss_policy: std::collections::BTreeMap<String, String>,
    /// `verified` means measured against a specimen. `claimed` means asserted
    /// and NOT yet tested — consumers should treat the two differently.
    pub verification: String,
    /// True when repeated lossy application compounds (re-encoding generations).
    pub generational: bool,
}

#[derive(Debug, Serialize)]
pub struct Dataset {
    pub schema_version: String,
    pub registry_version: String,
    pub license: String,
    pub source: String,
    /// The closed property vocabulary. Loss vectors are only comparable
    /// because this list is fixed and versioned (ADR-0004).
    pub properties: Vec<PropertyDoc>,
    pub formats: Vec<FormatOut>,
    pub engines: Vec<EngineOut>,
    pub edges: Vec<EdgeOut>,
    pub counts: Counts,
}

#[derive(Debug, Serialize)]
pub struct Counts {
    pub formats: usize,
    pub engines: usize,
    pub edges_total: usize,
    pub edges_verified: usize,
    pub edges_claimed: usize,
}

fn describe(prop: &str) -> &'static str {
    match prop {
        "geometry" => "Pixel or sample dimensions, and aspect ratio.",
        "color_depth" => "Bits per channel, and the size of the representable colour set.",
        "color_profile" => "ICC profile or colour space; determines whether colours reproduce accurately.",
        "alpha" => "Per-pixel transparency. Note the difference between a format supporting alpha and a file using it.",
        "time" => "Temporal structure: duration, frame timing, animation, synchronisation.",
        "layers" => "Independently addressable planes or objects that have not been flattened together.",
        "text_as_text" => "Whether text remains selectable, searchable and machine-readable, rather than becoming a picture of text.",
        "structure" => "Document hierarchy: headings, lists, tables, reading order.",
        "metadata" => "Descriptive and embedded data: authorship, timestamps, EXIF, GPS, comments.",
        "fonts" => "Embedded font programs, and whether glyphs render as the author intended.",
        "interactivity" => "Links, form fields, scripts, and other active behaviour.",
        "audio_channels" => "Channel count and layout, e.g. stereo or surround.",
        "sample_rate" => "Audio samples per second.",
        "bit_depth" => "Bits per audio sample.",
        "precision" => "Numeric precision of stored values, e.g. float versus integer.",
        "topology" => "Mesh connectivity: which vertices form which faces.",
        "materials" => "Surface appearance: textures, shaders, material assignments.",
        "rigging" => "Skeleton, joints, weights and animation bindings.",
        "provenance" => "Recorded history of the file's origin and prior processing.",
        _ => "See docs/specs/loss-vector.md.",
    }
}

pub fn build(reg: &Registry) -> Dataset {
    let properties = PROPERTIES
        .iter()
        .map(|p| PropertyDoc { id: (*p).to_string(), description: describe(p).to_string() })
        .collect();

    let formats: Vec<FormatOut> = reg
        .formats
        .values()
        .map(|f| FormatOut {
            id: f.id.clone(),
            name: f.name.clone(),
            kinds: f.kinds.clone(),
            extensions: f.extensions.clone(),
            mime_types: f.mime_types.clone(),
            preserves: f.preserves.clone(),
            puid: f.puid.clone(),
            puid_verified: f.puid_verified,
        })
        .collect();

    let engines: Vec<EngineOut> = reg
        .engines
        .values()
        .map(|e| EngineOut {
            id: e.id.clone(),
            name: e.name.clone(),
            version_pin: e.version_pin.clone(),
            license: e.license.spdx.clone(),
            build_profiles: e.license.build_profiles.clone(),
        })
        .collect();

    let edges: Vec<EdgeOut> = reg
        .edges
        .iter()
        .map(|e| EdgeOut {
            from: e.from.clone(),
            to: e.to.clone(),
            from_kind: e.from_kind.clone(),
            to_kind: e.to_kind.clone(),
            engine: e.engine.clone(),
            operation: e.op.clone(),
            loss: e.loss.0.iter().map(|(k, v)| (k.clone(), v.as_str().to_string())).collect(),
            loss_policy: e.loss_policy.clone(),
            verification: e.status.clone(),
            generational: e.generational,
        })
        .collect();

    let verified = edges.iter().filter(|e| e.verification == "verified").count();
    let counts = Counts {
        formats: formats.len(),
        engines: engines.len(),
        edges_total: edges.len(),
        edges_verified: verified,
        edges_claimed: edges.len() - verified,
    };

    Dataset {
        schema_version: SCHEMA_VERSION.to_string(),
        registry_version: reg.version.clone(),
        license: "CC0-1.0".to_string(),
        source: "https://github.com/Stan2032/Convert.To.Huh".to_string(),
        properties,
        formats,
        engines,
        edges,
        counts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn registry() -> Registry {
        Registry::load(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()).unwrap()
    }

    #[test]
    fn every_property_is_documented() {
        // An undocumented property in a public dataset is not usable by anyone
        // outside this repository.
        for p in PROPERTIES {
            assert_ne!(
                describe(p),
                "See docs/specs/loss-vector.md.",
                "property `{p}` has no published description"
            );
        }
    }

    #[test]
    fn export_round_trips_as_json() {
        let d = build(&registry());
        let json = serde_json::to_string(&d).expect("serialize");
        let back: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(back["schema_version"], SCHEMA_VERSION);
        assert!(!back["edges"].as_array().unwrap().is_empty());
    }

    #[test]
    fn verified_and_claimed_are_distinguishable() {
        // The single most important thing a consumer must be able to tell.
        let d = build(&registry());
        assert_eq!(d.counts.edges_verified + d.counts.edges_claimed, d.counts.edges_total);
        assert!(d.counts.edges_verified > 0, "no verified edges to publish");
        for e in &d.edges {
            assert!(
                matches!(e.verification.as_str(), "verified" | "claimed" | "refuted"),
                "unknown verification status `{}` would confuse consumers",
                e.verification
            );
        }
    }

    #[test]
    fn loss_values_are_from_the_closed_vocabulary() {
        let d = build(&registry());
        for e in &d.edges {
            for (prop, val) in &e.loss {
                assert!(
                    PROPERTIES.contains(&prop.as_str()),
                    "edge {}->{} references unknown property `{prop}`", e.from, e.to
                );
                assert!(
                    matches!(val.as_str(), "preserved" | "degraded" | "lost" | "synthesized"),
                    "edge {}->{} has unknown loss value `{val}`", e.from, e.to
                );
            }
        }
    }
}
