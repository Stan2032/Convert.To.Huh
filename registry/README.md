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

**Every edge in this registry is `status = "claimed"`. Zero are `verified`.**

The planner ignores `claimed` edges by default (`registry-schema.md`, rule 6),
which means the system as committed converts **nothing**. That is deliberate and
correct: a claim copied from a README is not a capability
(`docs/RESEARCH.md §3.5`). Edges become real when `capability-cartographer`
runs them against a corpus specimen and records a measurement.

PUIDs, by contrast, **are** now verified. All 8 were checked against PRONOM's
XML API on 2026-08-20 with `tools/verify_pronom.py`, and all 8 were correct —
but doing it properly surfaced two things that recall would have missed: PRONOM
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
