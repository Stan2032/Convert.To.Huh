# Roadmap

Sequenced so that each phase produces something falsifiable. The ordering
principle: **build the thing that proves the thesis wrong soonest.**

## Phase 0 — Foundations ✅

Research dossier, architecture, nine ADRs, five specs, seeded registry,
dependency-free validators, fifteen agent definitions.

Done. The validators are green and the registry is honest about converting
nothing.

## Phase 1 — Make one route real, end to end ✅

**PNG → JPEG, planned and executed, with a measured receipt.**

- [x] `huh-core` Rust crate: registry loader, graph builder
- [x] Detector: structural signatures with priority rules
- [x] Planner: Dijkstra with a multi-term cost function, profile/runtime pruning
- [x] Plan serialization, generated rationales
- [x] Engine adapter + budget enforcement
- [x] Corpus: 4 generated PNG specimens (alpha, opaque-alpha control, 16-bit, photo)
- [x] First `verified` edges, with measurements
- [x] Receipt with declared vs measured fidelity, SSIM
- [x] 15 tests; wasm32 build in CI

**Engine substitution.** The plan said libvips. Nothing is installed in this
environment — no vips, no ImageMagick, no ffmpeg — so the first engine is
`image-rs`, the pure-Rust `image` crate, linked directly into `huh-core`. That
turned out better than the original plan: it compiles to native *and* wasm32,
so it is the concrete proof of ADR-0002 rather than a stand-in for it. libvips
remains the right production raster engine (~4–8× faster, streaming, far wider
coverage) and is still `claimed`, unverified, ignored by the planner.

**What it falsified.** The schema mostly held. Three things changed:

1. `[edge.verification.measured]` needed to be a real structured block, not a
   loose key — CI rule 5 refused `verified` without it, which is what forced the
   measurement to happen at all.
2. The bootstrap circularity is real: the planner ignores unverified edges, but
   verifying one requires running it. Resolved with a `--allow-unverified`
   policy flag, which is the `capability-cartographer` probe path.
3. Measurement has to distinguish **"the property was lost"** from **"the input
   never exercised the property."** An RGBA file that is entirely opaque loses
   nothing observable when alpha is dropped. Without that distinction the
   harness manufactures agreement with itself.

### Known gaps left by Phase 1

- The planner returns **one** plan, not ranked alternatives. Yen's k-shortest
  paths is Phase 2, and intent selection needs it.
- Cost is computed but never compared against reality — `estimated_ms` is a
  registry constant, not a measurement (`perf-profiler`).
- No hop-level caching or resume yet (ADR-0008); execution is a straight loop.
- `image-rs` decodes and re-encodes at every hop rather than passing a decoded
  image along, which will matter for multi-hop raster routes.
- gray16 → JPEG produced a file **197.9% the size of its PNG input**. Nothing in
  the cost model knows that "convert to JPEG" can make a file bigger.

## Phase 2 — Multi-hop and the cost model

- [ ] Loss vector composition — join semantics, `lost` absorbing (`fidelity-analyst`)
- [ ] Multi-objective ranking, k-shortest paths (`route-planner-engineer`)
- [ ] Generated explanations from loss vectors
- [ ] Determinism property tests
- [ ] FFmpeg integration, LGPL profile (`engine-integrator`, `license-compliance-officer`)
- [ ] Build-profile pruning before search (`license-compliance-officer`)

**Falsifies:** whether a chained route beats a direct one on any real pair, and
whether generated explanations are actually readable. If explanations read like
machine output, the ranking is unaccountable and invariant 8 fails.

## Phase 3 — The "Huh" conversions

The thesis test. `AVI → PDF`, all four intents.

- [ ] Operation classes implemented end to end (`semantic-bridge-designer`)
- [ ] whisper.cpp integration with the `requires`/`provides` normalization hop
- [ ] Synthesis marking: plan, metadata, receipt (`semantic-bridge-designer`)
- [ ] Intent selection surfaced as ranked alternatives
- [ ] Resolve the durable synthesis-marker question — needs legal review (`RESEARCH §10.2`)

**Falsifies:** the whole premise. If users find the four-way choice confusing
rather than clarifying, the planner's value proposition is wrong and this
becomes a well-engineered converter with an unusual internal architecture. Test
with real users before building further on it.

## Phase 4 — Hostile input

- [ ] Sandbox tiers: gVisor default, microVM for `extreme` (`sandbox-warden`)
- [ ] Budget computation and enforcement, including archive recursion
- [ ] Bomb corpus and defence tests (`corpus-keeper`, `sandbox-warden`)
- [ ] Fuzz harness seeded from the corpus
- [ ] Error taxonomy end to end
- [ ] CVE watch for pinned engines

**Falsifies:** whether budgets can be computed accurately enough from headers to
be useful without being so conservative they refuse legitimate files.

## Phase 5 — Browser execution

- [ ] `huh-core` to `wasm32`, registry bundled (`wasm-runtime-engineer`)
- [ ] libvips and ffmpeg WASM builds, bundle budget enforced
- [ ] OPFS storage with quota checks
- [ ] WebCodecs fast path with ffmpeg.wasm fallback
- [ ] Memory estimation validated against real peaks (`perf-profiler`)
- [ ] Local-vs-server decision with user consent
- [ ] iOS testing on real hardware

**Falsifies:** ADR-0002 and ADR-0006 together. If the bundle is too large or
cold start too slow, local-first fails and this collapses into a server product.

## Phase 6 — Breadth

Only after the above. Adding engines before the model is proven means
re-modelling every edge when it changes.

- [ ] LibreOffice in its own supervised pool (`pipeline-architect`)
- [ ] Pandoc as the DocTree engine
- [ ] Image family: HEIC, AVIF, JPEG XL, WebP
- [ ] Archives, 3D, geospatial, fonts, subtitles
- [ ] Registry scale test — planning latency at 1000+ formats (`perf-profiler`)

## Cross-cutting: distribution

Not a phase, because it constrains Phase 5 rather than following it. ISPs
blanket-block file-converter sites (`docs/RESEARCH.md §1.7`) — the reference
product ships a `fail.html` explaining this to blocked users and offers an
Electron offline build as the workaround. A local-first converter that cannot be
*loaded* is as useless as a hosted one that is down.

- [ ] Offline-capable build — PWA + service worker is lighter than Electron
      (`wasm-runtime-engineer`)
- [ ] Do not depend on a single domain
- [ ] Expect category-based blocking, not reputation-based: being open source
      and privacy-preserving does not exempt you

This sharpens ADR-0006 rather than complicating it: an offline-capable converter
survives an ISP block; a hosted one does not.

## Deliberately not scheduled

Frontend framework, hosting, database, auth, pricing, and a licence for this
repository. None constrain the core; deciding them now would be guessing. The
repository licence in particular should be decided by the owner before any
public release, and interacts with ADR-0007.

## Open questions blocking specific phases

Tracked in `docs/RESEARCH.md §10`. Closing one is a milestone worth recording:

| # | Question | Blocks |
|---|---|---|
| 1 | Specimens per edge before measured replaces declared | Phase 2 |
| 2 | Durable synthesis provenance marker (legal review) | Phase 3 |
| 3 | Plan stability across registry versions | Phase 2 |
| 4 | Does mid-flow upload consent destroy the experience | Phase 5 |
| 5 | How far does "any" go before it damages trust | Phase 3 — *evidence arrived, see §10.5* |
