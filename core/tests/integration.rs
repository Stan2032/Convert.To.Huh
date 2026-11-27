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
use huh_core::registry::{Edge, Engine, Format, License, Resources, Security};
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
        confidence: None,
        specimens: vec![],
        measured: Default::default(),
        real_world: None,
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

/// ADR-0012: routes that lose the same property but replace it differently
/// are different choices. PNG to JPEG via composite_black, composite_white and
/// discard_channel used to collapse into ONE offered route.
#[test]
fn routes_differing_only_in_policy_are_separate_choices() {
    let r = reg();
    let plans = Planner::new(&r, &Policy::default()).plan_ranked("image/png", "image/jpeg", 6);
    let policies: std::collections::BTreeSet<_> = plans.iter()
        .filter_map(|p| p.loss_policy.get("alpha").cloned())
        .collect();
    for want in ["composite_black", "composite_white", "discard_channel"] {
        assert!(policies.contains(want), "{want} missing from offered routes: {policies:?}");
    }
}

#[test]
fn a_stated_preference_ranks_the_matching_policy_first() {
    let r = reg();
    let pol = Policy {
        prefer: [("alpha".to_string(), "composite_white".to_string())].into(),
        ..Default::default()
    };
    let p = Planner::new(&r, &pol);
    let best = p.plan("image/png", "image/jpeg").expect("route");
    assert_eq!(best.loss_policy.get("alpha").map(String::as_str), Some("composite_white"));
    let ranked = p.plan_ranked("image/png", "image/jpeg", 6);
    assert_eq!(ranked[0].loss_policy.get("alpha").map(String::as_str), Some("composite_white"));
}

#[test]
fn a_preference_never_buys_a_route_that_loses_more() {
    // The cheap route loses alpha WITH the preferred policy; the faithful one
    // keeps alpha. Preferring how alpha is lost must not make losing it win.
    let mut r = synthetic();
    r.edges[0].loss_policy.insert("alpha".into(), "composite_white".into());
    let pol = Policy {
        prefer: [("alpha".to_string(), "composite_white".to_string())].into(),
        ..Default::default()
    };
    let best = Planner::new(&r, &pol).plan("a", "b").expect("route");
    assert_eq!(best.totals.loss.get("alpha"), Some(LossValue::Preserved),
               "a preference outweighed fidelity");
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

/// Execution must run the engine the plan selected, not an equivalent-looking
/// one.
///
/// `execute` used to call the in-process encoder for every hop regardless of
/// `hop.engine`, while the plan's loss vector and the receipt both came from
/// the selected edge. Planning a libvips PNG->JPEG hop therefore printed
/// "transparent areas become black" — libvips' `composite_black` — and produced
/// image-rs output, where a transparent pixel keeps the colour underneath.
///
/// Measured on rgba-gradient.png, whose corner is fully transparent over light
/// yellow: image-rs gave [254,254,130], real libvips gives [0,0,0]. The user
/// was told the opposite of what they got, which is the exact failure ADR-0012
/// exists to prevent.
#[test]
fn the_engine_that_runs_is_the_engine_that_was_planned() {
    let r = reg();
    let p = Planner::new(&r, &Policy::default());
    let plan = p.plan("image/png", "image/jpeg").expect("route exists");

    let src = specimen("rgba-gradient.png");
    let hop = &plan.hops[0];

    // Skip only where the engine genuinely is not installed — a capability gap
    // is not a broken build. Never silently fall back to another engine.
    if hop.engine == "libvips" && !engine_present("vips") {
        eprintln!("skipping: libvips not installed");
        return;
    }

    let (out, receipt) = execute::execute(&plan, &src, 85).expect("conversion runs");

    // The receipt names an engine; the bytes must have come from it.
    assert!(
        receipt.hops[0].starts_with(&hop.engine),
        "receipt says {:?} but the plan selected {}",
        receipt.hops[0],
        hop.engine
    );

    if hop.engine == "libvips" {
        // The transparent corner must follow libvips' composite_black policy,
        // which is what the plan told the user would happen.
        let img = image::load_from_memory(&out).expect("output decodes");
        let rgb = img.to_rgb8();
        let (w, h) = rgb.dimensions();
        let corner = rgb.get_pixel(w - 1, h - 1).0;
        assert!(
            corner.iter().all(|&c| c < 24),
            "transparent corner came out {corner:?}; the plan promised \
             composite_black, so image-rs (discard_channel, ~[254,254,130]) ran \
             instead of the planned engine"
        );
    }
}

/// Audio routes used to fail before any engine ran: execution decoded every
/// input as an image first, so a WAV got "no image-rs decoder" even though the
/// plan had chosen ffmpeg. Now the planned engine runs, and the receipt says
/// the result was not measured in-process instead of implying it was.
#[test]
fn audio_routes_run_the_planned_engine_and_say_they_were_not_measured() {
    let r = reg();
    let wav = std::fs::read(root().join("corpus/audio/highfreq-mono.wav")).expect("specimen");

    // lgpl picks ffmpeg; gpl excludes ffmpeg's lgpl-only entry and picks
    // GStreamer, whose invocation uses `location=IN` placeholders.
    for (profile, program) in [("lgpl", "ffmpeg"), ("gpl", "gst-launch-1.0")] {
        let pol = Policy { build_profile: profile.into(), ..Default::default() };
        let plan = Planner::new(&r, &pol).plan("audio/wav", "audio/flac").expect("route exists");
        let hop = &plan.hops[0];
        if !engine_present(program) {
            eprintln!("skipping {profile}: {program} not installed");
            continue;
        }
        let (out, receipt) = execute::execute(&plan, &wav, 85)
            .unwrap_or_else(|e| panic!("{profile} via {}: {e}", hop.engine));
        assert_eq!(&out[..4], b"fLaC", "{} did not write FLAC", hop.engine);
        assert!(receipt.hops[0].starts_with(&hop.engine));
        assert!(receipt.measurements.is_empty() && receipt.unmeasured.is_some(),
                "an unmeasured route must say so in the receipt");
    }
}

/// ImageMagick writes a PNG's `oFFs` page offset as the GIF frame position
/// without enlarging the logical screen, so the frame lands entirely outside
/// the visible area. Found on two real Govdocs1 PNGs (RESEARCH §18.1); the
/// registry's receipts did not flag it, because no tracked property changed.
///
/// A characterisation test: it pins what the engine does, so the registry
/// note describing it is re-checked on every CI run. If ImageMagick changes
/// behaviour this fails, and the note must be revised.
#[test]
fn imagemagick_moves_an_offset_png_outside_the_gif_screen() {
    if !engine_present("convert") {
        eprintln!("skipping: ImageMagick not installed");
        return;
    }
    let dir = std::env::temp_dir().join(format!("huh-offs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out.gif");
    let st = std::process::Command::new("convert")
        .arg(root().join("corpus/raster/png-with-offset.png"))
        .arg(&out)
        .status()
        .expect("convert runs");
    assert!(st.success());
    let b = std::fs::read(&out).unwrap();
    std::fs::remove_dir_all(&dir).ok();

    let screen = (u16::from_le_bytes([b[6], b[7]]), u16::from_le_bytes([b[8], b[9]]));
    let mut i = 13 + if b[10] & 0x80 != 0 { 3 << ((b[10] & 7) + 1) } else { 0 };
    while b[i] == 0x21 {
        i += 2;
        while b[i] != 0 {
            i += b[i] as usize + 1;
        }
        i += 1;
    }
    assert_eq!(b[i], 0x2C, "expected an image descriptor");
    let left = u16::from_le_bytes([b[i + 1], b[i + 2]]);
    let top = u16::from_le_bytes([b[i + 3], b[i + 4]]);
    assert_eq!(screen, (117, 64), "logical screen");
    assert_eq!((left, top), (109, 119), "frame position carries the oFFs offset");
    // Outside if it starts past the edge on either axis; here top 119 >= height 64.
    assert!(left >= screen.0 || top >= screen.1, "frame lies wholly outside the screen");
}

/// `.doc`, `.xls` and `.ppt` share the OLE2 signature. Detection must tell
/// them apart by content, from the compound file's directory, and must not
/// need the extension to do it.
#[test]
fn binary_office_formats_are_told_apart_by_content() {
    let r = reg();
    for (file, want) in [
        ("report.doc", "application/msword"),
        ("figures.xls", "application/vnd.ms-excel"),
        ("slides.ppt", "application/vnd.ms-powerpoint"),
    ] {
        let bytes = std::fs::read(root().join("corpus/office").join(file)).expect("specimen");
        let named = detect::detect(&r, &bytes, Some(file));
        assert_eq!(named.format, want, "{file}");
        assert_eq!(named.detected_by, "signature", "{file} must be identified by content");
        // Misnamed: content still wins, and the mismatch is reported.
        let wrong = detect::detect(&r, &bytes, Some("renamed.txt"));
        assert_eq!(wrong.format, want, "{file} renamed");
        assert!(wrong.extension_mismatch, "{file}: a misleading extension must be flagged");
    }
}

/// The LibreOffice edge runs through the executor (the `OUTDIR` placeholder)
/// and keeps the specimen's text, which is what its loss vector declares.
#[test]
fn word_to_pdf_runs_through_libreoffice_and_keeps_the_text() {
    if !engine_present("soffice") {
        eprintln!("skipping: LibreOffice not installed");
        return;
    }
    let r = reg();
    let pol = Policy { build_profile: "gpl".into(), ..Default::default() };
    let plan = Planner::new(&r, &pol).plan("application/msword", "application/pdf")
        .expect("route under gpl");
    assert_eq!(plan.hops[0].engine, "libreoffice");
    let doc = std::fs::read(root().join("corpus/office/report.doc")).unwrap();
    let (pdf, receipt) = execute::execute(&plan, &doc, 85).expect("conversion runs");
    assert_eq!(&pdf[..5], b"%PDF-");
    assert!(receipt.unmeasured.is_some());

    // The text layer must carry the specimen's words (pdftotext, if present).
    if engine_present("pdftotext") {
        let dir = std::env::temp_dir().join(format!("huh-doc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("out.pdf");
        std::fs::write(&p, &pdf).unwrap();
        let out = std::process::Command::new("pdftotext").arg(&p).arg("-").output().unwrap();
        std::fs::remove_dir_all(&dir).ok();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("Turbidity exceeded the advisory threshold"), "{text}");
    }
}

/// A GIF palette holds 256 colours. An image with exactly 256 fits with
/// nothing lost; image-rs keeps all of them, libvips keeps at most 255 and
/// quantises the rest (RESEARCH §18.3: four real Govdocs1 PNGs with exactly
/// 256 colours came back with 203-238). Pins both behaviours.
#[test]
fn a_full_256_colour_palette_survives_image_rs_but_not_libvips_gifsave() {
    use huh_core::engines::{decode, encode, Budget};
    use std::collections::HashSet;
    let count = |img: &image::DynamicImage| {
        img.to_rgba8().pixels().map(|p| p.0).collect::<HashSet<_>>().len()
    };
    let b = Budget::default();
    let src = std::fs::read(root().join("corpus/raster/palette-256.png")).unwrap();
    let img = decode(&src, "image/png", &b).unwrap();
    assert_eq!(count(&img), 256, "specimen must have exactly 256 colours");

    let gif = encode(&img, "image/gif", 85).unwrap();
    assert_eq!(count(&decode(&gif, "image/gif", &b).unwrap()), 256,
               "image-rs: a palette that fits must survive intact");

    if !engine_present("vips") {
        eprintln!("skipping libvips half: vips not installed");
        return;
    }
    let dir = std::env::temp_dir().join(format!("huh-pal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out.gif");
    let st = std::process::Command::new("vips")
        .arg("gifsave").arg(root().join("corpus/raster/palette-256.png")).arg(&out)
        .status().unwrap();
    assert!(st.success());
    let vgif = std::fs::read(&out).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    let n = count(&decode(&vgif, "image/gif", &b).unwrap());
    assert!(n < 256, "libvips gifsave kept all 256 colours ({n}); revise RESEARCH §18.3");
}

/// The registry declared ImageMagick PNG->GIF `metadata = lost`; real files
/// contradicted it (RESEARCH §18.5). ImageMagick carries PNG comment text into
/// the GIF and drops other keys, so the value is `degraded`. Pins both halves
/// on the committed specimen, including the comment marked "do-not-share".
#[test]
fn imagemagick_carries_png_comments_into_gif() {
    if !engine_present("convert") {
        eprintln!("skipping: ImageMagick not installed");
        return;
    }
    let dir = std::env::temp_dir().join(format!("huh-meta-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out.gif");
    let st = std::process::Command::new("convert")
        .arg(root().join("corpus/raster/png-with-passengers.png"))
        .arg(&out)
        .status()
        .unwrap();
    assert!(st.success());
    let gif = std::fs::read(&out).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    let has = |s: &[u8]| gif.windows(s.len()).any(|w| w == s);
    assert!(has(b"internal-doc-id-88213-do-not-share"), "comment no longer carried: revise the registry");
    assert!(!has(b"Alice Example"), "Author now carried too: revise the registry");
}

fn engine_present(prog: &str) -> bool {
    std::process::Command::new("which")
        .arg(prog)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Matroska and WebM share the EBML header (`1A45DFA3`); only the DocType a
/// few bytes in tells them apart (PRONOM's `{0-32}` window). The reference
/// must detect as Matroska by content, and its WebM output as WebM.
#[test]
fn video_containers_are_told_apart_by_doctype() {
    let r = reg();
    let mkv = std::fs::read(root().join("corpus/video/reference.mkv")).expect("specimen");
    let d = detect::detect(&r, &mkv, Some("clip.webm"));
    assert_eq!(d.format, "video/x-matroska");
    assert_eq!(d.detected_by, "signature");
    assert!(d.extension_mismatch, "a .webm name on a Matroska file must be flagged");

    if !engine_present("ffmpeg") {
        eprintln!("skipping execution: ffmpeg not installed");
        return;
    }
    let plan = Planner::new(&r, &Policy::default())
        .plan("video/x-matroska", "video/webm")
        .expect("a verified route");
    assert_eq!(plan.hops[0].engine, "ffmpeg");
    // The cheapest route is the lossless control: nothing in it may be degraded.
    assert!(plan.hops[0].invocation.as_deref().unwrap_or("").contains("-lossless 1"),
            "planner should prefer the route that loses nothing");
    let (webm, _receipt) = execute::execute(&plan, &mkv, 85).expect("conversion runs");
    let d = detect::detect(&r, &webm, None);
    assert_eq!(d.format, "video/webm", "output must be WebM by content");
}

/// H.264 and H.265 need a GPL ffmpeg (ADR-0007). Under the lgpl profile those
/// routes must not exist at all (ground rule 6); under gpl they must, and the
/// planner must pick the lossless one when nothing is to be lost.
#[test]
fn gpl_video_routes_exist_only_under_the_gpl_profile() {
    let r = reg();
    let lgpl = Planner::new(&r, &Policy::default());
    let plans = lgpl.plan_ranked("video/x-matroska", "video/mp4", 10);
    assert!(!plans.is_empty(), "MPEG-4 Part 2 is an LGPL route to MP4");
    assert!(plans.iter().all(|p| p.hops.iter().all(|h| h.engine != "ffmpeg-gpl")),
            "a GPL engine leaked into the lgpl profile");

    let gpl = Planner::new(&r, &Policy { build_profile: "gpl".into(), ..Default::default() });
    let best = gpl.plan("video/x-matroska", "video/mp4").expect("gpl route");
    assert_eq!(best.hops[0].engine, "ffmpeg-gpl");
    assert!(best.hops[0].invocation.as_deref().unwrap_or("").contains("-qp 0"),
            "with a lossless H.264 route available, nothing should be degraded");
}

/// The label and the explanation are generated from the same loss vector, so
/// they must not contradict each other (ground rule 7). A route that only
/// degrades was once labelled "faithful — nothing tracked is lost", and WAV to
/// MP3 "stripped — pixels intact".
#[test]
fn intent_label_names_degradation_not_just_loss() {
    let r = reg();
    let lossy_vp9 = Planner::new(&r, &Policy::default())
        .plan_ranked("video/x-matroska", "video/webm", 5);
    let labels: Vec<String> = lossy_vp9.iter().map(|p| p.intent_label()).collect();
    assert!(labels.iter().any(|l| l.starts_with("faithful")), "{labels:?}");
    assert!(labels.iter().any(|l| l.starts_with("reduced — degrades") && l.contains("signal_fidelity")),
            "the lossy VP9 route must say what it degrades: {labels:?}");

    let mp3 = Planner::new(&r, &Policy::default()).plan("audio/wav", "audio/mpeg").unwrap();
    let l = mp3.intent_label();
    assert!(!l.contains("pixels") && !l.contains("intact"), "{l}");
    assert!(l.contains("degrades") && l.contains("signal_fidelity"), "{l}");
}
