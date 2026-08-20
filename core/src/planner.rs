//! Graph construction and route planning. ADR-0001.
//!
//! Dijkstra over a typed graph. The cost function is deliberately constrained:
//! every term must be expressible to a user in a sentence, because invariant 8
//! says a plan that cannot be explained cannot be ranked.

use crate::loss::{LossValue, LossVector};
use crate::registry::{Edge, Registry};
use serde::Serialize;
use std::collections::{BinaryHeap, HashMap};

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
    /// Generated from `totals.loss`, never hand-written (invariant 8).
    pub rationale: String,
}

impl Plan {
    /// Two routes with the same signature offer the user the same trade-off,
    /// whatever engines they pass through. Used to deduplicate alternatives.
    pub fn loss_signature(&self) -> String {
        self.totals
            .loss
            .0
            .iter()
            .map(|(k, v)| format!("{k}={}", v.as_str()))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// A short label for what this route is FOR, derived from its loss vector.
    /// Not a pipeline description — the user does not care which engine ran.
    pub fn intent_label(&self) -> String {
        let lost: Vec<&str> = self.totals.loss.0.iter()
            .filter(|(_, v)| **v == LossValue::Lost)
            .map(|(k, _)| k.as_str())
            .collect();
        if self.totals.contains_synthesis {
            return "inferred — a model generates content that was not in the input".into();
        }
        if lost.is_empty() {
            return "faithful — nothing tracked is lost".into();
        }
        if lost.contains(&"alpha") && lost.len() == 1 {
            return "flattened — transparency is discarded".into();
        }
        if lost.contains(&"metadata") && lost.len() == 1 {
            return "stripped — pixels intact, metadata discarded".into();
        }
        format!("lossy — discards {}", lost.join(", "))
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
}

pub struct Policy {
    /// Which distribution profile is active (ADR-0007). Engines outside it are
    /// pruned BEFORE search, so an illegal route cannot be proposed then fail.
    pub build_profile: String,
    /// Normally false: the planner ignores unverified edges (registry-schema
    /// rule 6). `capability-cartographer` sets it true to probe new edges —
    /// that is how the bootstrap chicken-and-egg is resolved.
    pub allow_unverified: bool,
    /// `native` | `wasm`
    pub runtime: String,
}

impl Default for Policy {
    fn default() -> Self {
        Policy { build_profile: "lgpl".into(), allow_unverified: false, runtime: "native".into() }
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
            let runtime_ok = match policy.runtime.as_str() {
                "wasm" => engine.runtimes.wasm,
                _ => engine.runtimes.native,
            };
            if !runtime_ok { continue; }
            if !policy.allow_unverified && e.status != "verified" { continue; }
            if e.status == "refuted" { continue; }
            eligible.push(i);
        }
        let mut adjacency: HashMap<String, Vec<usize>> = HashMap::new();
        for &i in &eligible {
            adjacency.entry(reg.edges[i].from.clone()).or_default().push(i);
        }
        Planner { reg, eligible, adjacency }
    }

    pub fn eligible_count(&self) -> usize { self.eligible.len() }

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
        let mut heap = BinaryHeap::new();
        let mut best: HashMap<String, f64> = HashMap::new();
        heap.push(QNode { cost: 0.0, node: from.to_string(), path: vec![] });
        best.insert(from.to_string(), 0.0);

        while let Some(QNode { cost, node, path }) = heap.pop() {
            if node == to {
                return Some((path, cost));
            }
            if cost > *best.get(&node).unwrap_or(&f64::INFINITY) {
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
                if !e.requires.is_empty() {
                    let provided: Vec<&str> = seed
                        .iter()
                        .chain(path.iter())
                        .flat_map(|&p| self.reg.edges[p].provides.iter().map(|s| s.as_str()))
                        .collect();
                    if !e.requires.iter().all(|r| provided.contains(&r.as_str())) {
                        continue;
                    }
                }
                let next_cost = cost + edge_cost(e);
                if next_cost < *best.get(&e.to).unwrap_or(&f64::INFINITY) {
                    best.insert(e.to.clone(), next_cost);
                    let mut np = path.clone();
                    np.push(ei);
                    heap.push(QNode { cost: next_cost, node: e.to.clone(), path: np });
                }
            }
        }
        None
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
                    let cost: f64 = total.iter().map(|&e| edge_cost(&self.reg.edges[e])).sum();
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
            });
        }
        let rationale = total_loss.explain();
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
            rationale,
        }
    }
}
