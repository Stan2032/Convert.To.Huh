//! Loads `registry/` — the declarative data that is the project's core asset.
//! Schema: `docs/specs/registry-schema.md`.

use crate::loss::{LossValue, LossVector};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Signature {
    pub offset: u64,
    pub magic: String,
    pub weight: String,
    /// Signatures sharing a group are ALTERNATIVES (any one matching is
    /// enough); distinct groups are all required. Ungrouped signatures each
    /// form their own group, i.e. they are required.
    ///
    /// Needed because the two shapes look identical in TOML but mean opposite
    /// things: AVI is `RIFF` at 0 AND `AVI ` at 8, while ZIP is
    /// `504B0304` at 0 OR `504B0506` at 0. Treating the second as AND made
    /// every ZIP undetectable by signature.
    #[serde(default)]
    pub group: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Detection {
    #[serde(default)]
    pub priority_over: Vec<String>,
    #[serde(default)]
    pub container_of: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Format {
    pub id: String,
    pub name: String,
    pub kinds: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub mime_types: Vec<String>,
    pub preserves: Vec<String>,
    #[serde(default)]
    pub puid: Option<String>,
    #[serde(default)]
    pub puid_verified: bool,
    #[serde(default)]
    pub signatures: Vec<Signature>,
    #[serde(default)]
    pub detection: Detection,
}

#[derive(Debug, Clone, Deserialize)]
pub struct License {
    pub spdx: String,
    #[serde(default)]
    pub build_profiles: Vec<String>,
    /// Licences pulled in by bundled components, which can be stricter than
    /// `spdx` and are what actually decides profile eligibility (ADR-0007).
    #[serde(default)]
    pub spdx_components: Vec<String>,
    /// ADR-0007 makes this first class, so consumers need it to apply the
    /// patent policy themselves rather than take our word for the profile.
    #[serde(default)]
    pub patent_exposure: String,
    /// Whether the licence text was actually read, as opposed to inferred.
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Security {
    pub hostility: String,
    pub isolation: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Resources {
    pub memory_model: String,
    pub default_timeout_s: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Runtimes {
    #[serde(default)]
    pub native: bool,
    #[serde(default)]
    pub wasm: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Engine {
    pub id: String,
    pub name: String,
    pub version_pin: String,
    pub license: License,
    pub security: Security,
    pub resources: Resources,
    #[serde(default)]
    pub runtimes: Runtimes,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EdgeCost {
    #[serde(default)]
    pub base_ms: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EdgeConstraints {
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub provides: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EdgeVerification {
    #[serde(default)]
    pub status: String,
    /// The specimens the measurement ran against.
    #[serde(default)]
    pub specimens: Vec<String>,
    /// The measurement itself, kept as written. Free-form because the metric
    /// differs by domain — SSIM for raster, SNR or band energy for audio
    /// (ADR-0014) — and a closed shape here would force every new metric
    /// through a schema change. Published: an edge saying `signal_fidelity =
    /// degraded` without the number behind it asks to be taken on trust, which
    /// is the one thing this dataset exists not to do.
    #[serde(default)]
    pub measured: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawEdge {
    pub engine: String,
    #[serde(default)]
    pub invocation: Option<String>,
    pub op: String,
    pub from: String,
    pub to: String,
    pub from_kind: String,
    pub to_kind: String,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub loss: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub cost: EdgeCost,
    #[serde(default)]
    pub constraints: EdgeConstraints,
    #[serde(default)]
    pub verification: EdgeVerification,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub engine: String,
    /// The exact call measured. ADR-0013: loss belongs to the invocation, not
    /// the engine — `vips webpsave` is lossy and `vips webpsave --lossless` is
    /// not, and both are "libvips PNG to WebP".
    pub invocation: Option<String>,
    pub op: String,
    pub from: String,
    pub to: String,
    pub from_kind: String,
    pub to_kind: String,
    pub loss: LossVector,
    /// Per-property qualifier saying WHAT REPLACED a lost property. ADR-0012.
    /// `lost` alone is true but insufficient: libvips composites alpha onto
    /// black while image-rs discards the channel, and both are "lost".
    pub loss_policy: BTreeMap<String, String>,
    pub generational: bool,
    pub base_ms: u64,
    pub requires: Vec<String>,
    pub provides: Vec<String>,
    pub status: String,
    /// Specimens the measurement ran against, carried through so the published
    /// dataset can say what the evidence was.
    pub specimens: Vec<String>,
    /// The measurement behind `status`, kept verbatim. See `EdgeVerification`.
    pub measured: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct EdgeFile {
    #[serde(default)]
    edge: Vec<RawEdge>,
}

#[derive(Debug, Clone)]
pub struct Registry {
    pub version: String,
    pub formats: BTreeMap<String, Format>,
    pub engines: BTreeMap<String, Engine>,
    pub edges: Vec<Edge>,
}

/// Parse an edge's `[edge.loss]` table.
///
/// Rejects an unrecognised loss value rather than skipping it. Skipping is the
/// dangerous option: an absent property means "not applicable to this
/// conversion" (see `dataset/README.md`), so a typo'd `alpha = "lostt"` would
/// not read as a broken entry — it would read as *transparency is unaffected*,
/// which is the opposite of what was written. A data error must not be able to
/// become a quieter, wronger claim.
fn parse_loss(
    raw: &BTreeMap<String, toml::Value>,
    where_: &str,
) -> Result<(LossVector, BTreeMap<String, String>, bool), String> {
    let mut v = LossVector::new();
    let mut policy = BTreeMap::new();
    let mut generational = false;
    for (k, val) in raw {
        if k == "generational" {
            generational = val.as_bool().ok_or_else(|| {
                format!("{where_}: `generational` must be a boolean, got {val}")
            })?;
            continue;
        }
        // A value is either a bare string or a table with a `value` key.
        let (s, pol) = match val {
            toml::Value::String(s) => (Some(s.as_str()), None),
            toml::Value::Table(t) => (
                t.get("value").and_then(|x| x.as_str()),
                t.get("policy").and_then(|x| x.as_str()),
            ),
            _ => (None, None),
        };
        let lv = match s {
            Some("preserved") => LossValue::Preserved,
            Some("degraded") => LossValue::Degraded,
            Some("lost") => LossValue::Lost,
            Some("synthesized") => LossValue::Synthesized,
            other => {
                return Err(format!(
                    "{where_}: loss value for `{k}` must be preserved, degraded, lost or \
                     synthesized, got {}",
                    other.map(|o| format!("`{o}`")).unwrap_or_else(|| val.to_string())
                ))
            }
        };
        // Only once the value is known good: a policy attached to an
        // unparseable value would otherwise outlive the value it qualifies.
        if let Some(p) = pol {
            policy.insert(k.clone(), p.to_string());
        }
        v.set(k, lv);
    }
    Ok((v, policy, generational))
}

impl Registry {
    pub fn load(root: &Path) -> Result<Registry, String> {
        let reg = root.join("registry");
        let version = std::fs::read_to_string(reg.join("VERSION"))
            .map_err(|e| format!("registry/VERSION: {e}"))?
            .trim()
            .to_string();

        let mut formats = BTreeMap::new();
        for p in read_toml_dir(&reg.join("formats"))? {
            let f: Format = toml::from_str(&p.1).map_err(|e| format!("{}: {e}", p.0))?;
            formats.insert(f.id.clone(), f);
        }

        let mut engines = BTreeMap::new();
        for p in read_toml_dir(&reg.join("engines"))? {
            let e: Engine = toml::from_str(&p.1).map_err(|e| format!("{}: {e}", p.0))?;
            engines.insert(e.id.clone(), e);
        }

        let mut edges = Vec::new();
        for p in read_toml_dir(&reg.join("edges"))? {
            let f: EdgeFile = toml::from_str(&p.1).map_err(|e| format!("{}: {e}", p.0))?;
            for r in f.edge {
                let (loss, loss_policy, generational) =
                    parse_loss(&r.loss, &format!("{} [{} -> {}]", p.0, r.from, r.to))?;
                edges.push(Edge {
                    engine: r.engine,
                    invocation: r.invocation,
                    op: r.op,
                    from: r.from,
                    to: r.to,
                    from_kind: r.from_kind,
                    to_kind: r.to_kind,
                    loss,
                    loss_policy,
                    generational,
                    base_ms: r.cost.base_ms,
                    requires: r.constraints.requires,
                    provides: r.constraints.provides,
                    status: if r.verification.status.is_empty() {
                        "claimed".into()
                    } else {
                        r.verification.status
                    },
                    specimens: r.verification.specimens,
                    measured: r.verification.measured,
                });
            }
        }

        Ok(Registry { version, formats, engines, edges })
    }
}

fn read_toml_dir(dir: &Path) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let s = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        out.push((p.file_name().unwrap().to_string_lossy().to_string(), s));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loss(src: &str) -> Result<(LossVector, BTreeMap<String, String>, bool), String> {
        let raw: BTreeMap<String, toml::Value> = toml::from_str(src).expect("fixture parses");
        parse_loss(&raw, "fixture")
    }

    #[test]
    fn a_typo_in_a_loss_value_is_rejected_not_dropped() {
        // The dangerous case. An absent property means "not applicable to this
        // conversion", so silently skipping `lostt` would turn "transparency is
        // destroyed" into "transparency is unaffected" — a quieter claim, and
        // the opposite of the truth.
        let e = loss(r#"alpha = "lostt""#).expect_err("must reject");
        assert!(e.contains("alpha"), "{e}");
        assert!(e.contains("lostt"), "{e}");
    }

    #[test]
    fn a_policy_never_outlives_the_value_it_qualifies() {
        // The policy used to be recorded before the value was validated, so a
        // bad value left an orphan policy describing nothing.
        assert!(loss(r#"alpha = { value = "nonsense", policy = "composite_black" }"#).is_err());

        let (v, policy, _) =
            loss(r#"alpha = { value = "lost", policy = "composite_black" }"#).expect("valid");
        assert_eq!(v.get("alpha"), Some(LossValue::Lost));
        assert_eq!(policy.get("alpha").map(String::as_str), Some("composite_black"));
    }

    #[test]
    fn every_documented_value_parses() {
        for good in ["preserved", "degraded", "lost", "synthesized"] {
            assert!(loss(&format!(r#"alpha = "{good}""#)).is_ok(), "{good} rejected");
        }
    }

    #[test]
    fn the_real_registry_still_loads() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        Registry::load(root).expect("the shipped registry must parse under the stricter rule");
    }
}
