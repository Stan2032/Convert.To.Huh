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
    assert!(receipt.ssim.unwrap() > 0.95, "ssim was {:?}", receipt.ssim);
    assert!(receipt.measurements.iter().all(|m| !m.contradicts),
        "no measurement may contradict the declared vector: {:?}", receipt.measurements);

    // The specimen has real transparency, so alpha loss must be OBSERVED.
    let alpha = receipt.measurements.iter().find(|m| m.property == "alpha").unwrap();
    assert_eq!(alpha.measured, "lost");
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
