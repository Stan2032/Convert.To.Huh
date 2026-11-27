//! Graph construction and route planning. ADR-0001.
//!
//! Dijkstra over a typed graph. The cost function is deliberately constrained:
//! every term must be expressible to a user in a sentence, because invariant 8
//! says a plan that cannot be explained cannot be ranked.

use crate::loss::{LossValue, LossVector};
use crate::registry::{Edge, Registry};
use serde::Serialize;
use std::collections::{BTreeSet, BinaryHeap, HashMap};

/// Per-hop base cost — prefers shorter routes, all else equal.
const DEPTH_COST: f64 = 1.0;
/// Penalty per property, by what the hop does to it.
const COST_LOST: f64 = 1.0;
const COST_DEGRADED: f64 = 0.4;
/// Standing penalty so inference never appears in a route a non-inference
/// route could serve (ADR-0009).
const COST_SYNTHESIZED: f64 = 1.5;
const SYNTHESIS_OP_PENALTY: f64 = 2.0;
/// Chained lossy re-encodes compound faster than their individual ratios.
const GENERATIONAL_PENALTY: f64 = 0.3;
/// Time is a TIE-BREAKER, not an objective.
///
/// Scaled so that even a 10-second hop (10,000 ms x 0.00001 = 0.1) cannot
/// outweigh degrading a single property (0.4). The default policy is
/// "preserve the most, subject to reaching the target"; a planner that traded
/// fidelity for speed by default would be a different product. Among routes
/// that cost the user the same thing, prefer the faster one.
const TIME_TIEBREAK_PER_MS: f64 = 0.00001;

/// Added per hop whose policy for a property differs from the user's stated
/// preference (`Policy::prefer`). Half of one degraded property (0.4), so a
/// preference reorders routes that lose the same things but can never buy a
/// route that loses more. ADR-0012: `alpha = lost` via `composite_black` and
/// via `composite_white` are both correct and look nothing alike, and only the
/// user knows which they want.
const PREFERENCE_MISMATCH: f64 = 0.2;

pub fn edge_cost(e: &Edge) -> f64 {
    let mut c = DEPTH_COST;
    for v in e.loss.0.values() {
        c += match v {
            LossValue::Preserved => 0.0,
            LossValue::Degraded => COST_DEGRADED,
            LossValue::Lost => COST_LOST,
            LossValue::Synthesized => COST_SYNTHESIZED,
        };
    }
    if e.op == "Synthesize" { c += SYNTHESIS_OP_PENALTY; }
    if e.generational { c += GENERATIONAL_PENALTY; }
    c += e.base_ms as f64 * TIME_TIEBREAK_PER_MS;
    c
}

#[derive(Debug, Clone, Serialize)]
pub struct Hop {
    pub index: usize,
    pub engine: String,
    pub op: String,
    pub from: String,
    pub to: String,
    pub from_kind: String,
    pub to_kind: String,
    pub loss: LossVector,
    pub isolation: String,
    pub estimated_ms: u64,
    pub verification_status: String,
    /// The registry's pinned version for `engine`, carried here so a receipt
    /// can name the engine it actually used. Receipts previously recorded
    /// huh-core's own crate version for every engine, which made them
    /// unreproducible in the one field that most needed to be right (ADR-0013:
    /// loss belongs to `(engine, version, invocation, from, to)`).
    pub engine_version: String,
    /// The exact call the edge was measured with. Carried into the plan because
    /// execution must run THIS, not something equivalent-looking: it is half of
    /// the edge's identity under ADR-0013, and the loss vector shown to the user
    /// describes this invocation and no other.
    pub invocation: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Totals {
    pub loss: LossVector,
    pub estimated_ms: u64,
    pub contains_synthesis: bool,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub registry_version: String,
    pub source: String,
    pub target: String,
    pub hops: Vec<Hop>,
    pub totals: Totals,
    /// What replaced each lost property, composed along the route (the first
    /// hop to destroy a property decides). Part of the loss signature.
    pub loss_policy: std::collections::BTreeMap<String, String>,
    /// Generated from `totals.loss`, never hand-written (invariant 8).
    pub rationale: String,
}

impl Plan {
    /// Two routes with the same signature offer the user the same trade-off,
    /// whatever engines they pass through. Used to deduplicate alternatives.
    ///
    /// Includes policies. Without them, PNG to JPEG via `composite_black`,
    /// `composite_white` and `discard_channel` collapsed into ONE offered
    /// choice, though they produce visibly different images (ADR-0012).
    pub fn loss_signature(&self) -> String {
        let mut parts: Vec<String> = self.totals.loss.0.iter()
            .map(|(k, v)| format!("{k}={}", v.as_str()))
            .collect();
        parts.extend(self.loss_policy.iter().map(|(k, p)| format!("{k}~{p}")));
        parts.join(",")
    }

    /// A short label for what this route is FOR, derived from its loss vector.
    /// Not a pipeline description — the user does not care which engine ran.
    pub fn intent_label(&self) -> String {
        let lost: Vec<&str> = self.totals.loss.0.iter()
            .filter(|(_, v)| **v == LossValue::Lost)
            .map(|(k, _)| k.as_str())
            .collect();
        // Degraded properties belong in the label too. Without them, every
        // route that only degrades (WAV to MP3, lossy VP9, H.264) was labelled
        // "faithful — nothing tracked is lost" while its own explanation said
        // "Reduces fine detail": two halves of one plan contradicting each
        // other (ground rule 7). Found by the first video edges, RESEARCH §20.2.
        let degraded: Vec<&str> = self.totals.loss.0.iter()
            .filter(|(_, v)| **v == LossValue::Degraded)
            .map(|(k, _)| k.as_str())
            .collect();
        if self.totals.contains_synthesis {
            return "inferred — a model generates content that was not in the input".into();
        }
        if lost.is_empty() && degraded.is_empty() {
            return "faithful — nothing tracked is lost".into();
        }
        if lost.is_empty() {
            return format!("reduced — degrades {}", degraded.join(", "));
        }
        let also = if degraded.is_empty() { String::new() }
                   else { format!("; degrades {}", degraded.join(", ")) };
        // Name what replaced a lost property, so routes that differ only in
        // policy are distinguishable in a list, not only in the rationale.
        let how: Vec<String> = self.loss_policy.iter()
            .filter(|(k, _)| lost.contains(&k.as_str()))
            .map(|(k, p)| format!("{k} via {p}"))
            .collect();
        let suffix = if how.is_empty() { String::new() } else { format!(" ({})", how.join(", ")) };
        if lost.contains(&"alpha") && lost.len() == 1 {
            return format!("flattened — transparency is discarded{suffix}{also}");
        }
        if lost.contains(&"metadata") && lost.len() == 1 && degraded.is_empty() {
            // Not "pixels intact": this label is reached by audio and video too.
            return "stripped — content intact, metadata discarded".into();
        }
        format!("lossy — discards {}{suffix}{also}", lost.join(", "))
    }
}

#[derive(PartialEq)]
struct QNode {
    cost: f64,
    node: String,
    path: Vec<usize>,
}
impl Eq for QNode {}
impl Ord for QNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reversed: BinaryHeap is a max-heap, we want min-cost first.
        other.cost.partial_cmp(&self.cost).unwrap_or(std::cmp::Ordering::Equal)
    }
}
impl PartialOrd for QNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}

pub struct Planner<'a> {
    reg: &'a Registry,
    /// Indices into `reg.edges` that are eligible under current policy.
    eligible: Vec<usize>,
    adjacency: HashMap<String, Vec<usize>>,
    prefer: std::collections::BTreeMap<String, String>,
}

pub struct Policy {
    /// Which distribution profile is active (ADR-0007). Engines outside it are
    /// pruned BEFORE search, so an illegal route cannot be proposed then fail.
    pub build_profile: String,
    /// Normally false: the planner ignores unverified edges (registry-schema
    /// rule 6). `capability-cartographer` sets it true to probe new edges —
    /// that is how the bootstrap chicken-and-egg is resolved.
    pub allow_unverified: bool,
    /// Preferred policy per property, e.g. `alpha -> composite_white`. Routes
    /// whose policy differs rank lower (`PREFERENCE_MISMATCH`); routes are
    /// never excluded for it. Empty by default: no preference.
    pub prefer: std::collections::BTreeMap<String, String>,
}

impl Default for Policy {
    fn default() -> Self {
        Policy { build_profile: "lgpl".into(), allow_unverified: false, prefer: Default::default() }
    }
}

impl<'a> Planner<'a> {
    pub fn new(reg: &'a Registry, policy: &Policy) -> Planner<'a> {
        let mut eligible = Vec::new();
        for (i, e) in reg.edges.iter().enumerate() {
            let Some(engine) = reg.engines.get(&e.engine) else { continue };
            // Hard constraint: prune before search.
            if !engine.license.build_profiles.contains(&policy.build_profile) {
                continue;
            }
            if !policy.allow_unverified && e.status != "verified" { continue; }
            if e.status == "refuted" { continue; }
            eligible.push(i);
        }
        let mut adjacency: HashMap<String, Vec<usize>> = HashMap::new();
        for &i in &eligible {
            adjacency.entry(reg.edges[i].from.clone()).or_default().push(i);
        }
        Planner { reg, eligible, adjacency, prefer: policy.prefer.clone() }
    }

    pub fn eligible_count(&self) -> usize { self.eligible.len() }

    /// `edge_cost` plus the preference penalty for this planner's policy.
    fn cost(&self, e: &Edge) -> f64 {
        let mismatches = self.prefer.iter()
            .filter(|(prop, want)| e.loss_policy.get(*prop).is_some_and(|have| have != *want))
            .count();
        edge_cost(e) + mismatches as f64 * PREFERENCE_MISMATCH
    }

    /// Dijkstra, with optional bans. The bans are what Yen's algorithm needs
    /// to force the search away from routes it has already found.
    fn shortest(
        &self,
        from: &str,
        to: &str,
        banned_edges: &[usize],
        banned_nodes: &[String],
        seed: &[usize],
    ) -> Option<(Vec<usize>, f64)> {
        // The search state is (node, capabilities carried so far), not node
        // alone. Engine `requires`/`provides` make two routes to the same node
        // non-interchangeable: a cheap route arriving without capability `c`
        // would otherwise record the best cost for that node and prune a
        // costlier route that arrives WITH `c`, making every edge out of the
        // node that requires `c` unreachable. `plan` would then return None
        // while a valid route existed, with nothing to indicate why.
        //
        // Latent today — no shipped edge declares `provides` — but
        // registry/edges/whisper-cpp.toml already requires `wav_16khz_mono`, so
        // it goes live with the first edge that supplies one.
        let capabilities = |seed: &[usize], path: &[usize]| -> BTreeSet<String> {
            seed.iter()
                .chain(path.iter())
                .flat_map(|&p| self.reg.edges[p].provides.iter().cloned())
                .collect()
        };

        let mut heap = BinaryHeap::new();
        let mut best: HashMap<(String, BTreeSet<String>), f64> = HashMap::new();
        let start = capabilities(seed, &[]);
        heap.push(QNode { cost: 0.0, node: from.to_string(), path: vec![] });
        best.insert((from.to_string(), start), 0.0);

        while let Some(QNode { cost, node, path }) = heap.pop() {
            if node == to {
                return Some((path, cost));
            }
            let here = capabilities(seed, &path);
            if cost > *best.get(&(node.clone(), here.clone())).unwrap_or(&f64::INFINITY) {
                continue;
            }
            for &ei in self.adjacency.get(&node).map(|v| v.as_slice()).unwrap_or(&[]) {
                if banned_edges.contains(&ei) {
                    continue;
                }
                let e = &self.reg.edges[ei];
                if banned_nodes.contains(&e.to) {
                    continue;
                }
                // Engine input constraints are edges, not assumptions: only
                // traverse if everything this edge requires has been provided
                // by the route so far (including the seed/root path).
                if !e.requires.is_empty() && !e.requires.iter().all(|r| here.contains(r)) {
                    continue;
                }
                let next_cost = cost + self.cost(e);
                let mut np = path.clone();
                np.push(ei);
                let key = (e.to.clone(), capabilities(seed, &np));
                if next_cost < *best.get(&key).unwrap_or(&f64::INFINITY) {
                    best.insert(key, next_cost);
                    heap.push(QNode { cost: next_cost, node: e.to.clone(), path: np });
                }
            }
        }
        None
    }

    /// A one-hop plan through one specific edge, or None if the policy makes
    /// it ineligible. For measurement harnesses: `plan` returns only the
    /// cheapest route, so an edge that never wins would never be exercised.
    pub fn plan_edge(&self, edge_index: usize) -> Option<Plan> {
        if !self.eligible.contains(&edge_index) {
            return None;
        }
        let e = &self.reg.edges[edge_index];
        Some(self.materialize(&e.from, &e.to, &[edge_index], self.cost(e)))
    }

    pub fn plan(&self, from: &str, to: &str) -> Option<Plan> {
        self.shortest(from, to, &[], &[], &[])
            .map(|(path, cost)| self.materialize(from, to, &path, cost))
    }

    /// Yen's k-shortest loopless paths.
    ///
    /// ADR-0001: the alternatives ARE the product. Ranking by cost alone picks
    /// one reading of an underspecified request and is wrong for most users
    /// (content-model.md §4). Deduplicated by loss signature — two routes that
    /// cost differently but lose exactly the same things are one choice, not
    /// two, and offering both is decision fatigue with no information.
    pub fn plan_ranked(&self, from: &str, to: &str, k: usize) -> Vec<Plan> {
        let Some((first, first_cost)) = self.shortest(from, to, &[], &[], &[]) else {
            return vec![];
        };
        let mut accepted: Vec<(Vec<usize>, f64)> = vec![(first, first_cost)];
        let mut candidates: Vec<(Vec<usize>, f64)> = Vec::new();

        while accepted.len() < k {
            let prev = accepted.last().unwrap().0.clone();
            for i in 0..prev.len() {
                let root = &prev[..i];
                let spur_node = if i == 0 {
                    from.to_string()
                } else {
                    self.reg.edges[prev[i - 1]].to.clone()
                };

                // Ban the next edge of every accepted path sharing this root,
                // so the spur search cannot rediscover a known route.
                let mut banned_edges: Vec<usize> = Vec::new();
                for (p, _) in accepted.iter().chain(candidates.iter()) {
                    if p.len() > i && p[..i] == *root {
                        banned_edges.push(p[i]);
                    }
                }
                // Ban root nodes so the spur cannot loop back through them.
                let mut banned_nodes: Vec<String> = vec![from.to_string()];
                for &ei in root {
                    banned_nodes.push(self.reg.edges[ei].from.clone());
                }

                if let Some((spur, _)) = self.shortest(&spur_node, to, &banned_edges, &banned_nodes, root) {
                    let mut total: Vec<usize> = root.to_vec();
                    total.extend(spur);
                    let cost: f64 = total.iter().map(|&e| self.cost(&self.reg.edges[e])).sum();
                    let known = accepted.iter().chain(candidates.iter()).any(|(p, _)| *p == total);
                    if !known {
                        candidates.push((total, cost));
                    }
                }
            }
            if candidates.is_empty() {
                break;
            }
            // Deterministic tie-break: cost, then length, then edge indices.
            candidates.sort_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.0.len().cmp(&b.0.len()))
                    .then(a.0.cmp(&b.0))
            });
            accepted.push(candidates.remove(0));
        }

        // Distinct by what they COST you, not by which engines they use.
        let mut seen_signatures: Vec<String> = Vec::new();
        let mut out = Vec::new();
        for (path, cost) in accepted {
            let plan = self.materialize(from, to, &path, cost);
            let sig = plan.loss_signature();
            if seen_signatures.contains(&sig) {
                continue;
            }
            seen_signatures.push(sig);
            out.push(plan);
        }
        out
    }

    fn materialize(&self, from: &str, to: &str, path: &[usize], cost: f64) -> Plan {
        let mut hops = Vec::new();
        let mut total_loss = LossVector::new();
        let mut ms = 0u64;
        for (i, &ei) in path.iter().enumerate() {
            let e = &self.reg.edges[ei];
            let engine = self.reg.engines.get(&e.engine);
            total_loss = total_loss.compose(&e.loss);
            ms += e.base_ms;
            hops.push(Hop {
                index: i,
                engine: e.engine.clone(),
                op: e.op.clone(),
                from: e.from.clone(),
                to: e.to.clone(),
                from_kind: e.from_kind.clone(),
                to_kind: e.to_kind.clone(),
                loss: e.loss.clone(),
                isolation: engine.map(|x| x.security.isolation.clone()).unwrap_or_default(),
                estimated_ms: e.base_ms,
                verification_status: e.status.clone(),
                engine_version: engine
                    .map(|x| x.version_pin.clone())
                    .unwrap_or_else(|| "unknown".into()),
                invocation: e.invocation.clone(),
            });
        }
        // Compose policies the same way as the vector: the FIRST hop to destroy
        // a property decides what replaced it, since later hops see it already
        // gone.
        let mut policies: std::collections::BTreeMap<String, String> = Default::default();
        for &ei in path {
            for (k, v) in &self.reg.edges[ei].loss_policy {
                policies.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        let rationale = total_loss.explain_with(&policies);
        // A policy only describes a property the route actually lost.
        policies.retain(|k, _| total_loss.get(k).is_some_and(|v| v != LossValue::Preserved));
        Plan {
            registry_version: self.reg.version.clone(),
            source: from.to_string(),
            target: to.to_string(),
            hops,
            totals: Totals {
                contains_synthesis: total_loss.contains_synthesis(),
                loss: total_loss,
                estimated_ms: ms,
                cost,
            },
            loss_policy: policies,
            rationale,
        }
    }
}
