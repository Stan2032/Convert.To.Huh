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
pub const SCHEMA_VERSION: &str = "1.2.0";

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
    /// Licences of bundled components, which can be stricter than `license`
    /// and are what actually decide profile eligibility (ADR-0007).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spdx_components: Vec<String>,
    /// ADR-0007 treats patent exposure as first class; publishing
    /// `build_profiles` without it asks consumers to trust our conclusion
    /// instead of applying their own policy.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub patent_exposure: String,
    /// Whether the licence text was read, or the entry is an inference.
    pub license_verified: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EdgeOut {
    pub from: String,
    pub to: String,
    pub from_kind: String,
    pub to_kind: String,
    pub engine: String,
    /// The exact invocation measured. Loss is a property of HOW the engine was
    /// called (ADR-0013), so an edge without this is under-specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocation: Option<String>,
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
    /// How far to trust content a model invented. Required on every
    /// `Synthesize` edge by ADR-0009 and enforced in the source by validator
    /// rule 3 — but it was dropped between the loader and here, so the only
    /// synthesis edge in the dataset published no way to judge its output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// The specimens the measurement ran against.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub specimens: Vec<String>,
    /// The measurement behind `verification`: the engine build, the metric and
    /// its value. Shapes differ by domain, deliberately (ADR-0014 requires each
    /// `signal_fidelity` claim to name its metric, and SSIM does not apply to
    /// audio). Published because an edge that says a property was degraded, and
    /// will not say by how much or how anyone knows, is asking to be taken on
    /// trust — which is the one thing this dataset exists not to do.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub measured: std::collections::BTreeMap<String, serde_json::Value>,
    /// How the edge behaved on real files: how many it ran on, how often each
    /// declared loss actually occurred, and whether anything contradicted it.
    /// Added in schema 1.1.0 (`size_ratio` in 1.2.0); absent where no
    /// real-file run exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_world: Option<crate::registry::RealWorld>,
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
    /// Tested and found not to work. Counted separately because `claimed` was
    /// computed as `total - verified`, which silently filed every refuted edge
    /// under `claimed` — the opposite of what it means. Latent only because no
    /// refuted edge exists yet, and CONTRIBUTING.md asks for exactly those.
    pub edges_refuted: usize,
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
        "signal_fidelity" => "Whether the sample values themselves survived, as distinct from how many there are or how wide each one is. This is the property lossy compression destroys. Measured with a domain metric — SSIM for raster, band energy or SNR for audio.",
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

/// Structural problems that must never reach a published dataset.
///
/// The exporter is the last gate before data leaves this repository, and it was
/// briefly not acting like one: an invalid registry exported cleanly, because
/// validation lived only in `tools/validate_registry.py` and nothing made
/// `huh export` consult it. A consumer cannot tell a checked dataset from an
/// unchecked one, so the export must refuse rather than warn.
///
/// This deliberately does NOT reimplement the Python validator — that would be
/// two sources of truth. It checks the handful of invariants a *consumer*
/// depends on, and the full rule set stays in one place.
pub fn integrity_problems(d: &Dataset) -> Vec<String> {
    let mut problems = Vec::new();
    let known: std::collections::BTreeSet<&str> = PROPERTIES.iter().copied().collect();

    for e in &d.edges {
        let id = format!("{} -> {} via {}", e.from, e.to, e.engine);

        if !matches!(e.verification.as_str(), "verified" | "claimed" | "refuted") {
            problems.push(format!("{id}: unknown verification `{}`", e.verification));
        }
        for (prop, val) in &e.loss {
            if !known.contains(prop.as_str()) {
                problems.push(format!("{id}: unknown property `{prop}`"));
            }
            if !matches!(val.as_str(), "preserved" | "degraded" | "lost" | "synthesized") {
                problems.push(format!("{id}: unknown loss value `{val}` for `{prop}`"));
            }
        }
        // A policy describing a property the edge does not mention is
        // meaningless to a consumer trying to interpret it.
        for prop in e.loss_policy.keys() {
            if !e.loss.contains_key(prop) {
                problems.push(format!("{id}: policy for `{prop}` but no loss entry for it"));
            }
        }

        // ADR-0009: synthesis is never silent. Publishing invented content
        // without saying how far to trust it is exactly the silence that ADR
        // forbids, and the source-side rule could not catch it because the
        // value was being dropped after validation, on the way out.
        let synthesizes = e.operation == "Synthesize"
            || e.loss.values().any(|v| v == "synthesized");
        if synthesizes && e.confidence.is_none() {
            problems.push(format!(
                "{id}: synthesizes content but publishes no `confidence` (ADR-0009)"
            ));
        }
        if !d.formats.iter().any(|f| f.id == e.from) {
            problems.push(format!("{id}: `from` names a format not in the dataset"));
        }
        if !d.formats.iter().any(|f| f.id == e.to) {
            problems.push(format!("{id}: `to` names a format not in the dataset"));
        }
        if !d.engines.iter().any(|g| g.id == e.engine) {
            problems.push(format!("{id}: engine not in the dataset"));
        }
    }
    problems
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
            spdx_components: e.license.spdx_components.clone(),
            patent_exposure: e.license.patent_exposure.clone(),
            license_verified: e.license.verified,
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
            invocation: e.invocation.clone(),
            operation: e.op.clone(),
            loss: e.loss.0.iter().map(|(k, v)| (k.clone(), v.as_str().to_string())).collect(),
            loss_policy: e.loss_policy.clone(),
            verification: e.status.clone(),
            confidence: e.confidence,
            specimens: e.specimens.clone(),
            // TOML values into JSON ones. Anything that will not convert is
            // dropped rather than guessed at: a mangled measurement in a
            // published dataset is worse than an absent one.
            measured: e
                .measured
                .iter()
                .filter_map(|(k, v)| {
                    serde_json::to_value(v).ok().map(|j| (k.clone(), j))
                })
                .collect(),
            real_world: e.real_world.clone(),
            generational: e.generational,
        })
        .collect();

    // Count each status on its own. Deriving one from the others is what put
    // refuted edges in the claimed bucket.
    let count_of = |s: &str| edges.iter().filter(|e| e.verification == s).count();
    let counts = Counts {
        formats: formats.len(),
        engines: engines.len(),
        edges_total: edges.len(),
        edges_verified: count_of("verified"),
        edges_claimed: count_of("claimed"),
        edges_refuted: count_of("refuted"),
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
        assert_eq!(
            d.counts.edges_verified + d.counts.edges_claimed + d.counts.edges_refuted,
            d.counts.edges_total,
            "every edge must land in exactly one status bucket"
        );
        assert!(d.counts.edges_verified > 0, "no verified edges to publish");

        // The buckets must be counted, not inferred. This assertion previously
        // read `verified + claimed == total`, which is true whether or not
        // refuted edges are misfiled — the test encoded the bug it should have
        // caught. Recount from the edges themselves.
        let by_status = |s: &str| d.edges.iter().filter(|e| e.verification == s).count();
        assert_eq!(d.counts.edges_verified, by_status("verified"));
        assert_eq!(d.counts.edges_claimed, by_status("claimed"));
        assert_eq!(d.counts.edges_refuted, by_status("refuted"));
        for e in &d.edges {
            assert!(
                matches!(e.verification.as_str(), "verified" | "claimed" | "refuted"),
                "unknown verification status `{}` would confuse consumers",
                e.verification
            );
        }
    }

    #[test]
    fn real_world_counts_are_published_and_only_count_declared_losses() {
        let d = build(&registry());
        let with: Vec<_> = d.edges.iter().filter(|e| e.real_world.is_some()).collect();
        assert!(!with.is_empty(), "real-world counts exist in the registry but were not exported");
        for e in with {
            let rw = e.real_world.as_ref().unwrap();
            let id = format!("{} {} -> {}", e.engine, e.from, e.to);
            assert_eq!(e.verification, "verified", "{id}");
            assert!(rw.ran <= rw.files, "{id}");
            if let Some(c) = rw.contradicted {
                assert!(c <= rw.ran, "{id}");
            }
            if let Some(sr) = &rw.size_ratio {
                assert!(sr.min > 0.0 && sr.min <= sr.median && sr.median <= sr.max, "{id}: {sr:?}");
            }
            for (prop, n) in &rw.affected {
                let declared = e.loss.get(prop).map(String::as_str);
                assert!(matches!(declared, Some(v) if v != "preserved"),
                        "{id}: affected `{prop}` but the edge declares {declared:?}");
                assert!(*n <= rw.ran, "{id}: affected {prop} {n} > ran {}", rw.ran);
            }
        }
    }

    #[test]
    fn the_real_dataset_has_no_integrity_problems() {
        let d = build(&registry());
        let problems = integrity_problems(&d);
        assert!(problems.is_empty(), "published dataset is broken: {problems:#?}");
    }

    #[test]
    fn integrity_check_actually_catches_things() {
        // A gate nothing proves fires is not a gate. `huh export` silently
        // published a dataset from a failing registry once already.
        let mut d = build(&registry());

        let mut bad = d.edges[0].clone();
        bad.verification = "probably-fine".into();
        d.edges.push(bad);
        assert!(integrity_problems(&d).iter().any(|p| p.contains("unknown verification")));

        let mut orphan = d.edges[0].clone();
        orphan.loss.clear();
        orphan.loss_policy.insert("alpha".into(), "composite_black".into());
        d.edges.push(orphan);
        assert!(integrity_problems(&d).iter().any(|p| p.contains("no loss entry")));

        let mut dangling = d.edges[0].clone();
        dangling.to = "image/entirely-made-up".into();
        d.edges.push(dangling);
        assert!(integrity_problems(&d).iter().any(|p| p.contains("not in the dataset")));
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
