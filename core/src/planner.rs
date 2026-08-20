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

    pub fn plan(&self, from: &str, to: &str) -> Option<Plan> {
        let mut heap = BinaryHeap::new();
        let mut best: HashMap<String, f64> = HashMap::new();
        heap.push(QNode { cost: 0.0, node: from.to_string(), path: vec![] });
        best.insert(from.to_string(), 0.0);

        while let Some(QNode { cost, node, path }) = heap.pop() {
            if node == to {
                return Some(self.materialize(from, to, &path, cost));
            }
            if cost > *best.get(&node).unwrap_or(&f64::INFINITY) { continue; }
            for &ei in self.adjacency.get(&node).map(|v| v.as_slice()).unwrap_or(&[]) {
                let e = &self.reg.edges[ei];
                // Engine input constraints are edges, not assumptions: only
                // traverse if everything this edge requires has been provided.
                if !e.requires.is_empty() {
                    let provided: Vec<&str> = path.iter()
                        .flat_map(|&p| self.reg.edges[p].provides.iter().map(|s| s.as_str()))
                        .collect();
                    if !e.requires.iter().all(|r| provided.contains(&r.as_str())) { continue; }
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
