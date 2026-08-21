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
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawEdge {
    pub engine: String,
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
    pub op: String,
    pub from: String,
    pub to: String,
    pub from_kind: String,
    pub to_kind: String,
    pub loss: LossVector,
    pub generational: bool,
    pub base_ms: u64,
    pub requires: Vec<String>,
    pub provides: Vec<String>,
    pub status: String,
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

fn parse_loss(raw: &BTreeMap<String, toml::Value>) -> (LossVector, bool) {
    let mut v = LossVector::new();
    let mut generational = false;
    for (k, val) in raw {
        if k == "generational" {
            generational = val.as_bool().unwrap_or(false);
            continue;
        }
        // A value is either a bare string or a table with a `value` key.
        let s = match val {
            toml::Value::String(s) => Some(s.as_str()),
            toml::Value::Table(t) => t.get("value").and_then(|x| x.as_str()),
            _ => None,
        };
        let lv = match s {
            Some("preserved") => LossValue::Preserved,
            Some("degraded") => LossValue::Degraded,
            Some("lost") => LossValue::Lost,
            Some("synthesized") => LossValue::Synthesized,
            _ => continue,
        };
        v.set(k, lv);
    }
    (v, generational)
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
                let (loss, generational) = parse_loss(&r.loss);
                edges.push(Edge {
                    engine: r.engine,
                    op: r.op,
                    from: r.from,
                    to: r.to,
                    from_kind: r.from_kind,
                    to_kind: r.to_kind,
                    loss,
                    generational,
                    base_ms: r.cost.base_ms,
                    requires: r.constraints.requires,
                    provides: r.constraints.provides,
                    status: if r.verification.status.is_empty() {
                        "claimed".into()
                    } else {
                        r.verification.status
                    },
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
