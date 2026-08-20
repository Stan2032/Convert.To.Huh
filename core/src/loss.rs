//! Loss vectors and their composition.
//!
//! Spec: `docs/specs/loss-vector.md`. The rules that matter and are easy to get
//! wrong: `Lost` is absorbing (you cannot un-lose a property) and `Synthesized`
//! is contaminating (if any hop invents a property, the chain's output for that
//! property is invented).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The closed, versioned property list from `docs/specs/loss-vector.md §2`.
/// Closed on purpose: an open list makes vectors incomparable across edges.
pub const PROPERTIES: &[&str] = &[
    "geometry", "color_depth", "color_profile", "alpha", "time", "layers",
    "text_as_text", "structure", "metadata", "fonts", "interactivity",
    "audio_channels", "sample_rate", "bit_depth", "precision", "topology",
    "materials", "rigging", "provenance",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LossValue {
    /// Bit-exact, or perceptually indistinguishable by the property's own metric.
    Preserved,
    /// Present but reduced.
    Degraded,
    /// Not representable in the target. Gone.
    Lost,
    /// NOT PRESENT IN THE INPUT — inferred by a model. A different axis entirely.
    Synthesized,
}

impl LossValue {
    /// Precedence for composition. Higher wins.
    ///
    /// `Synthesized` outranks `Lost` because contamination beats absorption:
    /// if a chain both loses and invents a property, the output is invented.
    fn rank(self) -> u8 {
        match self {
            LossValue::Preserved => 0,
            LossValue::Degraded => 1,
            LossValue::Lost => 2,
            LossValue::Synthesized => 3,
        }
    }

    pub fn join(self, other: LossValue) -> LossValue {
        if self.rank() >= other.rank() { self } else { other }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LossValue::Preserved => "preserved",
            LossValue::Degraded => "degraded",
            LossValue::Lost => "lost",
            LossValue::Synthesized => "synthesized",
        }
    }
}

/// A per-property loss vector. Absent property = not applicable to this hop.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LossVector(pub BTreeMap<String, LossValue>);

impl LossVector {
    pub fn new() -> Self { Self(BTreeMap::new()) }

    pub fn set(&mut self, prop: &str, v: LossValue) {
        self.0.insert(prop.to_string(), v);
    }

    pub fn get(&self, prop: &str) -> Option<LossValue> { self.0.get(prop).copied() }

    /// Compose this vector with a later hop's, per `loss-vector.md §4`.
    /// Not addition — a per-property join.
    pub fn compose(&self, next: &LossVector) -> LossVector {
        let mut out = self.0.clone();
        for (k, v) in &next.0 {
            let merged = out.get(k).map(|a| a.join(*v)).unwrap_or(*v);
            out.insert(k.clone(), merged);
        }
        LossVector(out)
    }

    pub fn contains_synthesis(&self) -> bool {
        self.0.values().any(|v| *v == LossValue::Synthesized)
    }

    fn props_with(&self, v: LossValue) -> Vec<&str> {
        let mut p: Vec<&str> = self.0.iter()
            .filter(|(_, x)| **x == v)
            .map(|(k, _)| k.as_str())
            .collect();
        p.sort_unstable();
        p
    }

    /// Plain-language rationale, GENERATED from the vector — never hand-written
    /// (invariant 8). If a plan cannot be explained, it cannot be ranked.
    pub fn explain(&self) -> String {
        fn human(p: &str) -> &str {
            match p {
                "alpha" => "transparency",
                "text_as_text" => "text selectability",
                "color_profile" => "colour accuracy",
                "color_depth" => "colour depth",
                "geometry" => "resolution",
                "structure" => "document structure",
                "time" => "timing",
                "audio_channels" => "audio channels",
                "sample_rate" => "audio sample rate",
                "bit_depth" => "audio bit depth",
                other => other,
            }
        }
        fn list(items: &[&str]) -> String {
            let h: Vec<&str> = items.iter().map(|p| human(p)).collect();
            match h.len() {
                0 => String::new(),
                1 => h[0].to_string(),
                2 => format!("{} and {}", h[0], h[1]),
                _ => format!("{} and {}", h[..h.len() - 1].join(", "), h[h.len() - 1]),
            }
        }

        let mut parts = Vec::new();
        let synth = self.props_with(LossValue::Synthesized);
        if !synth.is_empty() {
            parts.push(format!(
                "Generates {} with a model — this is inferred, not extracted",
                list(&synth)
            ));
        }
        let lost = self.props_with(LossValue::Lost);
        if !lost.is_empty() { parts.push(format!("Loses {}", list(&lost))); }
        let deg = self.props_with(LossValue::Degraded);
        if !deg.is_empty() { parts.push(format!("Reduces {}", list(&deg))); }
        let kept = self.props_with(LossValue::Preserved);
        if !kept.is_empty() { parts.push(format!("Keeps {}", list(&kept))); }

        if parts.is_empty() { return "No declared effect on any tracked property.".into(); }
        format!("{}.", parts.join(". "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lost_is_absorbing() {
        // Once alpha is gone, a later alpha-capable format does not restore it.
        let mut a = LossVector::new();
        a.set("alpha", LossValue::Lost);
        let mut b = LossVector::new();
        b.set("alpha", LossValue::Preserved);
        assert_eq!(a.compose(&b).get("alpha"), Some(LossValue::Lost));
        assert_eq!(b.compose(&a).get("alpha"), Some(LossValue::Lost));
    }

    #[test]
    fn synthesized_is_contaminating() {
        let mut a = LossVector::new();
        a.set("text_as_text", LossValue::Synthesized);
        let mut b = LossVector::new();
        b.set("text_as_text", LossValue::Preserved);
        assert_eq!(a.compose(&b).get("text_as_text"), Some(LossValue::Synthesized));
        assert!(a.compose(&b).contains_synthesis());
    }

    #[test]
    fn synthesized_outranks_lost() {
        let mut a = LossVector::new();
        a.set("text_as_text", LossValue::Lost);
        let mut b = LossVector::new();
        b.set("text_as_text", LossValue::Synthesized);
        assert_eq!(a.compose(&b).get("text_as_text"), Some(LossValue::Synthesized));
    }

    #[test]
    fn degraded_does_not_upgrade() {
        let mut a = LossVector::new();
        a.set("geometry", LossValue::Degraded);
        let mut b = LossVector::new();
        b.set("geometry", LossValue::Preserved);
        assert_eq!(a.compose(&b).get("geometry"), Some(LossValue::Degraded));
    }

    #[test]
    fn explanation_is_generated_and_names_the_property() {
        let mut v = LossVector::new();
        v.set("alpha", LossValue::Lost);
        v.set("color_profile", LossValue::Preserved);
        let e = v.explain();
        assert!(e.contains("transparency"), "{e}");
        assert!(e.contains("colour accuracy"), "{e}");
    }
}
