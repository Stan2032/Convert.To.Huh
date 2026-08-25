# The Registry

Declarative data describing every format we know, every engine we can run, and
every capability we claim. This is the most valuable artifact in the repository
(`docs/ARCHITECTURE.md`), and it is machine-checked by
`tools/validate_registry.py` in CI.

```
formats/   one file per format   — owner: format-registry-curator
engines/   one file per backend  — owner: engine-integrator (+ sandbox-warden for [security])
edges/     one file per engine   — owner: capability-cartographer
VERSION    the registry version pinned into every receipt
```

Schema: `docs/specs/registry-schema.md`.

## Current state — read this before assuming anything works

**10 edges are `verified`; the rest are `claimed`.** The planner ignores
`claimed` edges by default (`registry-schema.md`, rule 6), so only the verified
subset can actually be routed.

Every verified edge belongs to `image-rs` and was promoted the same way: run
against corpus specimens, measured with SSIM and per-property observation, and
the numbers written into a `measured` block. `tools/verify_edges.sh`
regenerates them. The claimed edges — libvips, FFmpeg, LibreOffice,
whisper.cpp — are still what they always were: plausible statements copied from
documentation, which is not a capability (`docs/RESEARCH.md §3.5`).

The gap between those two groups is the whole point of the `verification`
field. Two of the four errors this registry's validator has caught were in
hand-written loss vectors that looked entirely reasonable.

PUIDs are verified too — 11 of 12, checked against PRONOM's XML API with
`tools/verify_pronom.py`, all correct. (WebP is the exception: PRONOM's search
returned nothing for it, so the field is deliberately empty rather than filled
with a plausible guess — see `registry/formats/image-webp.toml`.) Doing it
properly surfaced two things recall would have missed: PRONOM
identifies format *versions* rather than formats (so a PUID is a representative
version, not an equivalence), and PRONOM's signature syntax is strictly more
expressive than ours. Both are recorded per-format in `[notes]` and in
`docs/RESEARCH.md §2.2`.

## The checking is not ceremonial

Both of these were caught by `tools/validate_registry.py` in the same commit
that introduced them, in hand-written seed data, by an author who had just
written the spec:

- **`libvips` PNG→JPEG claimed `color_depth = degraded {16→8}`** against a
  `image/jpeg` entry that omitted `color_depth` from `preserves`. Rule 4
  rejected it. The *format* was wrong: baseline JPEG does carry colour depth,
  at 8 bits. This sharpened the semantics of `preserves` — it means "can carry
  this property at all", and capacity limits belong on the edge as `degraded`
  magnitudes. Alpha is `lost` because there is nowhere to put it; colour depth
  is `degraded` because there is somewhere smaller.

- **`whisper.cpp` WAV→text/plain claimed `time = degraded`**, reasoning that
  whisper emits segment timestamps. Rule 4 rejected it: `text/plain` cannot
  carry `time`, so those timestamps have nowhere to go and are genuinely lost.
  Keeping them requires a different *target* — WebVTT or SRT, which do carry
  `time`. A hand-wave became a concrete missing-capability finding.

Neither was a false positive. This is the argument for keeping the registry as
checked data rather than as code comments and good intentions.

### And once it was not enough

`image-rs png → gif` was measured twice, months apart and for different reasons
— once for raster coverage, once for palettisation policy — and the second
measurement was filed in a new file instead of merged into the existing entry.
Two rows for one edge, disagreeing in their notes, **published in the dataset**.

The validator had nothing to say, because every rule it owned was per-file, and
each entry was individually valid. It was found by loading the published JSON
and grouping edges by `(engine, from, to, invocation)` — reading the deliverable
as a stranger would, rather than checking the sources as their author.

Rule 9 now rejects it. The transferable part is not the rule: it is that
**validating the inputs is not the same as reading the output.** Do both.
