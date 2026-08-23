//! Integration tests against the real registry and the real corpus.
//!
//! These lock in the behaviour that the whole design rests on. If one of these
//! breaks, an invariant in `docs/ARCHITECTURE.md` has been violated.

use huh_core::loss::LossValue;
use huh_core::planner::{Planner, Policy};
use huh_core::{detect, execute, registry::Registry};
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}
fn reg() -> Registry {
    Registry::load(&root()).expect("registry loads")
}
fn specimen(name: &str) -> Vec<u8> {
    std::fs::read(root().join("corpus/raster").join(name)).expect("specimen exists")
}

#[test]
fn detection_is_content_first_not_extension_based() {
    let r = reg();
    let png = specimen("rgba-gradient.png");
    // Lie about the extension. Content must win (ADR-0005).
    let d = detect::detect(&r, &png, Some("totally-a-document.pdf"));
    assert_eq!(d.format, "image/png");
    assert_eq!(d.detected_by, "signature");
    assert!(d.extension_mismatch, "a lying extension must be surfaced, not ignored");
}

#[test]
fn unverified_edges_are_ignored_by_default() {
    let r = reg();
    let strict = Planner::new(&r, &Policy::default());
    let permissive = Planner::new(&r, &Policy { allow_unverified: true, ..Default::default() });
    assert!(
        permissive.eligible_count() > strict.eligible_count(),
        "the registry should contain unverified edges that the planner ignores"
    );
}

#[test]
fn build_profile_prunes_before_search() {
    // ADR-0007: a legally unavailable route must not exist, rather than being
    // proposed and then failing. LibreOffice is gpl-only, so lgpl sees fewer.
    let r = reg();
    let lgpl = Planner::new(&r, &Policy {
        build_profile: "lgpl".into(), allow_unverified: true, ..Default::default()
    });
    let gpl = Planner::new(&r, &Policy {
        build_profile: "gpl".into(), allow_unverified: true, ..Default::default()
    });
    assert_ne!(lgpl.eligible_count(), gpl.eligible_count(),
        "build profiles must actually change the graph");
}

#[test]
fn png_to_jpeg_plans_and_declares_alpha_lost() {
    let r = reg();
    let p = Planner::new(&r, &Policy::default());
    let plan = p.plan("image/png", "image/jpeg").expect("a verified route exists");
    assert_eq!(plan.hops.len(), 1);
    assert_eq!(plan.totals.loss.get("alpha"), Some(LossValue::Lost));
    assert!(!plan.totals.contains_synthesis);
    assert!(plan.rationale.contains("transparency"), "{}", plan.rationale);
}

#[test]
fn png_to_jpeg_actually_converts_and_measurements_agree() {
    let r = reg();
    let p = Planner::new(&r, &Policy::default());
    let plan = p.plan("image/png", "image/jpeg").unwrap();
    let input = specimen("rgba-gradient.png");
    let (out, receipt) = execute::execute(&plan, &input, 85).expect("conversion succeeds");

    assert_eq!(&out[..3], &[0xFF, 0xD8, 0xFF], "output must actually be a JPEG");
    assert!(receipt.measurements.iter().all(|m| !m.contradicts),
        "no measurement may contradict the declared vector: {:?}", receipt.measurements);

    // This specimen carries real transparency and JPEG cannot, so the fidelity
    // score MUST be poor — the conversion genuinely destroys something.
    //
    // An earlier version of this test asserted > 0.95 here, and passed, because
    // the harness compared stored RGB and ignored alpha (RESEARCH.md §13.8).
    // That assertion was encoding the bug: it demanded the measurement agree
    // with an intuition that was wrong. The honest expectation is the opposite.
    let s = receipt.ssim.expect("comparable geometry");
    assert!(s < 0.95,
        "an alpha-destroying conversion scored {s:.4} — either the specimen lost \
         its transparency or the harness is ignoring alpha again");

    // The specimen has real transparency, so alpha loss must be OBSERVED.
    let alpha = receipt.measurements.iter().find(|m| m.property == "alpha").unwrap();
    assert_eq!(alpha.measured, "lost");
}

#[test]
fn fidelity_is_high_where_nothing_is_actually_lost() {
    // The counterpart to the test above: on an OPAQUE specimen the same
    // conversion loses nothing structural, so the score must be high. Without
    // this pair, "alpha conversions score low" could be satisfied by a harness
    // that simply scores everything low.
    let r = reg();
    let p = Planner::new(&r, &Policy::default());
    let plan = p.plan("image/png", "image/jpeg").unwrap();
    let input = specimen("rgb-photo.png");
    let (_, receipt) = execute::execute(&plan, &input, 85).expect("conversion succeeds");
    let s = receipt.ssim.expect("comparable geometry");
    assert!(s > 0.95, "opaque conversion scored only {s:.4}");
}

#[test]
fn opaque_alpha_is_reported_as_unexercised_not_lost() {
    // The control case. A file with an alpha channel that is entirely opaque
    // loses nothing observable, and saying "lost" would manufacture agreement.
    let r = reg();
    let p = Planner::new(&r, &Policy::default());
    let plan = p.plan("image/png", "image/jpeg").unwrap();
    let (_, receipt) = execute::execute(&plan, &specimen("rgba-opaque.png"), 85).unwrap();
    let alpha = receipt.measurements.iter().find(|m| m.property == "alpha").unwrap();
    assert_eq!(alpha.measured, "not-present-in-input");
    assert!(!alpha.contradicts);
}

#[test]
fn sixteen_bit_input_actually_demonstrates_depth_degradation() {
    let r = reg();
    let p = Planner::new(&r, &Policy::default());
    let plan = p.plan("image/png", "image/jpeg").unwrap();
    let (_, receipt) = execute::execute(&plan, &specimen("gray16.png"), 85).unwrap();
    let depth = receipt.measurements.iter().find(|m| m.property == "color_depth").unwrap();
    assert_eq!(depth.measured, "degraded", "detail: {:?}", depth.detail);
}

#[test]
fn round_trip_keeps_alpha_lost_because_lost_is_absorbing() {
    // The single most important property in the project. Returning to a format
    // that supports transparency does NOT bring the transparency back, and the
    // composed loss vector must say so (loss-vector.md §4).
    let r = reg();
    let p = Planner::new(&r, &Policy::default());
    let to_jpeg = p.plan("image/png", "image/jpeg").unwrap();
    let back = p.plan("image/jpeg", "image/png").unwrap();

    let composed = to_jpeg.totals.loss.compose(&back.totals.loss);
    assert_eq!(composed.get("alpha"), Some(LossValue::Lost),
        "PNG supports alpha, but the round trip must still report it lost");

    // And confirm it against the actual bytes, not just the model.
    let (jpeg, _) = execute::execute(&to_jpeg, &specimen("rgba-gradient.png"), 85).unwrap();
    let (png2, _) = execute::execute(&back, &jpeg, 85).unwrap();
    assert_eq!(png2[25], 2, "round-tripped PNG must be colour type 2 (RGB, no alpha)");
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, e) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *e = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

#[test]
fn budget_refuses_a_declared_pixel_bomb() {
    // A small file declaring an enormous grid must be refused BEFORE allocation
    // (ADR-0003). Refusal is a normal outcome, not a crash.
    //
    // The CRC recomputation below is the point of the test. An earlier version
    // patched the dimensions and left the checksum stale, so the decoder
    // rejected it as malformed and the budget guard was never reached — the
    // test passed for the wrong reason and proved nothing. A real bomb is
    // perfectly well-formed; that is what makes it dangerous.
    use huh_core::engines::{decode, Budget, ConvertError};
    let mut png = specimen("rgba-gradient.png");
    png[16..20].copy_from_slice(&60_000u32.to_be_bytes()); // width
    png[20..24].copy_from_slice(&60_000u32.to_be_bytes()); // height
    let crc = crc32(&png[12..29]); // over chunk type + IHDR data
    png[29..33].copy_from_slice(&crc.to_be_bytes());

    // 60000 x 60000 = 3.6e9 pixels, from a 35 KB file.
    let err = decode(&png, "image/png", &Budget::default()).unwrap_err();
    assert!(matches!(err, ConvertError::BudgetExceeded(_)),
        "a well-formed pixel bomb must hit the budget, got: {err}");
}

#[test]
fn refusal_when_no_route_exists() {
    let r = reg();
    let p = Planner::new(&r, &Policy::default());
    assert!(p.plan("image/png", "application/pdf").is_none(),
        "no verified PNG->PDF route exists yet; refusing is correct");
}

// ---------------------------------------------------------------------------
// Phase 2: ranked alternatives.
//
// The real registry's raster section is nearly tree-shaped (everything pivots
// through PNG), so genuine alternatives are rare there by construction. These
// tests build a synthetic graph so Yen's algorithm is exercised on its own
// terms rather than on whatever topology the registry happens to have.
// ---------------------------------------------------------------------------

use huh_core::loss::LossVector;
use huh_core::registry::{Edge, Engine, Format, License, Resources, Runtimes, Security};
use std::collections::BTreeMap;

fn fmt(id: &str, preserves: &[&str]) -> Format {
    Format {
        id: id.into(),
        name: id.into(),
        kinds: vec!["Raster".into()],
        extensions: vec![],
        mime_types: vec![],
        preserves: preserves.iter().map(|s| s.to_string()).collect(),
        puid: None,
        puid_verified: false,
        signatures: vec![],
        detection: Default::default(),
    }
}

fn engine(id: &str) -> Engine {
    Engine {
        id: id.into(),
        name: id.into(),
        version_pin: ">=1,<2".into(),
        license: License {
            spdx: "MIT".into(),
            build_profiles: vec!["lgpl".into()],
            spdx_components: vec![],
            patent_exposure: "none".into(),
            verified: true,
        },
        security: Security { hostility: "moderate".into(), isolation: "wasm".into() },
        resources: Resources { memory_model: "streaming".into(), default_timeout_s: 30 },
        runtimes: Runtimes { native: true, wasm: true },
    }
}

fn edge(from: &str, to: &str, loss: &[(&str, LossValue)], ms: u64) -> Edge {
    let mut v = LossVector::new();
    for (k, val) in loss {
        v.set(k, *val);
    }
    Edge {
        engine: "e".into(),
        op: "Transcode".into(),
        from: from.into(),
        to: to.into(),
        from_kind: "Raster".into(),
        to_kind: "Raster".into(),
        loss: v,
        invocation: None,
        loss_policy: Default::default(),
        generational: false,
        base_ms: ms,
        requires: vec![],
        provides: vec![],
        status: "verified".into(),
        specimens: vec![],
        measured: Default::default(),
    }
}

/// A → B two ways: a cheap route that destroys alpha, and a costlier route
/// that keeps it. A user genuinely might want either.
fn synthetic() -> Registry {
    let mut formats = BTreeMap::new();
    for f in [
        fmt("a", &["alpha", "geometry"]),
        fmt("b", &["alpha", "geometry"]),
        fmt("cheap", &["geometry"]),
        fmt("faithful", &["alpha", "geometry"]),
    ] {
        formats.insert(f.id.clone(), f);
    }
    let mut engines = BTreeMap::new();
    engines.insert("e".to_string(), engine("e"));
    Registry {
        version: "test".into(),
        formats,
        engines,
        edges: vec![
            edge("a", "cheap", &[("alpha", LossValue::Lost), ("geometry", LossValue::Preserved)], 1),
            edge("cheap", "b", &[("geometry", LossValue::Preserved)], 1),
            edge("a", "faithful", &[("alpha", LossValue::Preserved), ("geometry", LossValue::Preserved)], 5),
            edge("faithful", "b", &[("alpha", LossValue::Preserved), ("geometry", LossValue::Preserved)], 5),
        ],
    }
}

#[test]
fn yens_finds_multiple_distinct_routes_and_ranks_fidelity_first() {
    let r = synthetic();
    let p = Planner::new(&r, &Policy::default());
    let plans = p.plan_ranked("a", "b", 5);
    assert_eq!(plans.len(), 2, "both routes differ in loss, so both must survive dedup");

    // The route that PRESERVES alpha ranks first, even though its hops declare
    // 5x the wall time. This test originally asserted the opposite, on the
    // assumption that "cheap in milliseconds" means "cheap to the planner".
    // It does not, and should not: the default policy is to preserve the most,
    // subject to reaching the target. Time is only a tie-breaker.
    assert_eq!(plans[0].totals.loss.get("alpha"), Some(LossValue::Preserved));
    assert_eq!(plans[1].totals.loss.get("alpha"), Some(LossValue::Lost));
    assert!(plans[0].totals.cost < plans[1].totals.cost);
}

#[test]
fn time_breaks_ties_but_never_outweighs_fidelity() {
    let mut r = synthetic();
    // Make both routes loss-equivalent so only time separates them...
    r.edges[2].loss.set("alpha", LossValue::Lost);
    r.edges[3].loss.set("alpha", LossValue::Lost);
    let p = Planner::new(&r, &Policy::default());
    let plans = p.plan_ranked("a", "b", 5);
    assert_eq!(plans.len(), 1, "loss-equivalent routes collapse");
    // ...and the surviving route is the fast one (1ms hops, not 5ms).
    assert!(plans[0].hops.iter().all(|h| h.estimated_ms == 1),
        "among equal-loss routes the faster must win: {:?}",
        plans[0].hops.iter().map(|h| h.estimated_ms).collect::<Vec<_>>());

    // And the tie-break must be too small to flip a fidelity decision: a
    // 10s hop is worth 0.1, a single degraded property is worth 0.4.
    let mut slow = synthetic();
    slow.edges[2].base_ms = 10_000;
    slow.edges[3].base_ms = 10_000;
    let p2 = Planner::new(&slow, &Policy::default());
    let plans2 = p2.plan_ranked("a", "b", 5);
    assert_eq!(plans2[0].totals.loss.get("alpha"), Some(LossValue::Preserved),
        "a very slow faithful route must still beat a fast lossy one");
}

#[test]
fn alternatives_are_labelled_by_what_they_cost_not_by_engine() {
    let r = synthetic();
    let p = Planner::new(&r, &Policy::default());
    let plans = p.plan_ranked("a", "b", 5);
    let labels: Vec<String> = plans.iter().map(|p| p.intent_label()).collect();
    assert!(labels.iter().any(|l| l.contains("flattened")), "{labels:?}");
    assert!(labels.iter().any(|l| l.contains("faithful")), "{labels:?}");
    // The label must describe the trade-off, not the pipeline.
    assert!(!labels.iter().any(|l| l.contains("image-rs") || l.contains("Transcode")));
}

#[test]
fn loss_equivalent_routes_collapse_to_one_choice() {
    // Two routes that cost the user exactly the same thing are ONE choice.
    // Offering both is decision fatigue with no information.
    let mut r = synthetic();
    // Make the faithful route lossy in exactly the same way as the cheap one.
    r.edges[2].loss.set("alpha", LossValue::Lost);
    r.edges[3].loss.set("alpha", LossValue::Lost);
    let p = Planner::new(&r, &Policy::default());
    let plans = p.plan_ranked("a", "b", 5);
    assert_eq!(plans.len(), 1, "identical loss signatures must deduplicate");
}

#[test]
fn ranked_planning_is_deterministic() {
    // Users bookmark conversions; a silently changed route is a silently
    // changed output (RESEARCH §10.3). Same inputs must give the same plans.
    let r = reg();
    let p = Planner::new(&r, &Policy::default());
    let a = p.plan_ranked("image/png", "image/jpeg", 4);
    for _ in 0..8 {
        let b = p.plan_ranked("image/png", "image/jpeg", 4);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.loss_signature(), y.loss_signature());
            assert_eq!(x.totals.cost, y.totals.cost);
            assert_eq!(
                x.hops.iter().map(|h| h.to.clone()).collect::<Vec<_>>(),
                y.hops.iter().map(|h| h.to.clone()).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn yens_never_returns_a_looping_route() {
    let r = reg();
    let p = Planner::new(&r, &Policy::default());
    for plan in p.plan_ranked("image/gif", "image/jpeg", 6) {
        let mut seen = vec![plan.source.clone()];
        for h in &plan.hops {
            assert!(!seen.contains(&h.to), "route revisits {}: {:?}", h.to, seen);
            seen.push(h.to.clone());
        }
    }
}

#[test]
fn tiff_is_the_only_target_preserving_sixteen_bit_depth() {
    // A measured claim from the registry, asserted so it cannot silently
    // regress: TIFF was the only probe target that kept gray16 at 16-bit.
    let r = reg();
    let p = Planner::new(&r, &Policy::default());
    let tiff = p.plan("image/png", "image/tiff").expect("tiff route");
    assert_eq!(tiff.totals.loss.get("color_depth"), Some(LossValue::Preserved));
    for lossy in ["image/webp", "image/bmp"] {
        let pl = p.plan("image/png", lossy).expect(lossy);
        assert_eq!(pl.totals.loss.get("color_depth"), Some(LossValue::Degraded), "{lossy}");
    }
    let gif = p.plan("image/png", "image/gif").expect("gif route");
    assert_eq!(gif.totals.loss.get("color_depth"), Some(LossValue::Lost));
}

#[test]
fn gif_and_jpeg_lose_opposite_things() {
    // The first genuine intent choice in the registry, backed by measurement.
    let r = reg();
    let p = Planner::new(&r, &Policy::default());
    let gif = p.plan("image/png", "image/gif").unwrap();
    let jpeg = p.plan("image/png", "image/jpeg").unwrap();

    // JPEG keeps colour, destroys transparency.
    assert_eq!(jpeg.totals.loss.get("alpha"), Some(LossValue::Lost));
    assert_eq!(jpeg.totals.loss.get("color_profile"), Some(LossValue::Preserved));
    // GIF keeps (some) transparency, destroys colour depth.
    assert_eq!(gif.totals.loss.get("color_depth"), Some(LossValue::Lost));
    assert_eq!(gif.totals.loss.get("alpha"), Some(LossValue::Degraded));
}

/// A costlier route carrying a capability must not be pruned by a cheaper one
/// that lacks it.
///
/// Dijkstra's visited set assumes two arrivals at the same node are
/// interchangeable. Engine `requires`/`provides` break that assumption: the
/// real search state is (node, capabilities carried). Keyed by node alone, the
/// cheap capability-less arrival at `mid` recorded the best cost and the
/// capable arrival was discarded, so the edge out of `mid` requiring that
/// capability became unreachable and planning returned None with nothing to
/// say why.
///
/// No shipped edge declares `provides` yet, but registry/edges/whisper-cpp.toml
/// already REQUIRES `wav_16khz_mono`, so this goes live with the first edge
/// that supplies one.
#[test]
fn a_capability_carrying_route_is_not_pruned_by_a_cheaper_one() {
    let mut formats = BTreeMap::new();
    for f in [
        fmt("start", &["geometry"]),
        fmt("mid", &["geometry"]),
        fmt("end", &["geometry"]),
    ] {
        formats.insert(f.id.clone(), f);
    }
    let mut engines = BTreeMap::new();
    engines.insert("e".to_string(), engine("e"));

    let keep = [("geometry", LossValue::Preserved)];
    // Cheap, but supplies nothing.
    let cheap = edge("start", "mid", &keep, 1);
    // Expensive, but supplies what the last hop needs.
    let mut capable = edge("start", "mid", &keep, 50);
    capable.provides = vec!["normalised".into()];
    // Only traversable with that capability in hand.
    let mut gated = edge("mid", "end", &keep, 1);
    gated.requires = vec!["normalised".into()];

    let reg = Registry {
        version: "test".into(),
        formats,
        engines,
        edges: vec![cheap, capable, gated],
    };

    let planner = Planner::new(&reg, &Policy::default());
    let plan = planner.plan("start", "end");
    assert!(
        plan.is_some(),
        "a valid route exists through the capability-providing edge, but the \
         planner found none — the cheaper capability-less route pruned it"
    );
    assert_eq!(plan.unwrap().hops.len(), 2, "expected the two-hop capable route");
}
