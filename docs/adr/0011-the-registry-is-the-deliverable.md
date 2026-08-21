# 0011 — The registry is the deliverable

Status: **accepted** · 2026-08-21

Refines ADR-0010. That decision redirected the project from a refuted cipher
premise to verified sanitization. This one narrows further, after checking what
already exists.

## Context

ADR-0010 concluded with "a sanitizer that proves what it removed." A prior-art
check (`docs/RESEARCH.md §12.3b`) found most of that already shipped:

- **Dangerzone** (Freedom of the Press Foundation) renders untrusted documents
  to safe PDFs inside disposable containers and strips all metadata. Its
  isolation — a gVisor sandbox inside a container, no networking, nothing
  mounted from the host — is what ADR-0003 demands and what this project has
  not built.
- **mat2** (relied on by Tails) removes metadata across ~25 format families at
  the file-structure level. This registry covers six raster formats.

Two claims made in §12.3 were wrong and are corrected there: images are *not* an
uncovered gap, and "nobody reports what was removed" describes a missing
feature, not a missing product. **Building another sanitizer would reinvent both
tools with less rigour.**

What the same check did *not* find is a machine-readable dataset of conversion
loss. The digital preservation field discusses it continuously in prose —
OpenAIRE, CESSDA and USGS all publish guidance about losing resolution, layers,
formatting and precision — and publishes no structured measurements. PRONOM
identifies **what a file is**, with ~2,000 signatures and an API that this
project already consumes. Nothing identifies **what converting it costs**.

## Decision

**The published dataset is the deliverable. The CLI is the instrument that
produces and validates it.**

Concretely:

- `huh export` emits `dataset/conversion-loss-registry.json` against a
  versioned schema (`SCHEMA_VERSION`), decoupled from internal structs so
  representation can change without breaking consumers.
- The dataset is **CC0-1.0**. Measurements of how software behaves are facts;
  attaching conditions to them would defeat the purpose.
- Every edge carries its `verification` status. A consumer must always be able
  to distinguish a measurement from an assertion, and tests enforce that the
  field is present and drawn from a known set.
- Coverage is grown by measuring, not by asserting. An unmeasured claim is
  recorded as `claimed` and the planner ignores it (registry-schema rule 6).

## Consequences

- **Success is measured in verified edges, not features.** Ten today. A
  hundred well-measured edges across independent engines would be a genuinely
  useful public asset; a polished CLI over ten would not.
- **Cross-engine comparison becomes the most valuable data to collect.** Does
  libvips lose what image-rs loses on the same conversion? Nobody has published
  that, and the answer bears directly on multi-engine sanitization. This needs
  edges recorded per `(engine, from, to)` rather than per `(from, to)` — which
  the schema already does.
- **Negative results belong in the dataset.** `refuted` is a status, and an edge
  that does not work is as useful to a downstream consumer as one that does.
- **The project must be contributable by people who are not us.** A dataset only
  one repository can extend is a private database with extra steps. Hence
  `CONTRIBUTING.md` and a self-testing validator.
- The prose specs (`docs/specs/`) become the dataset's documentation and must
  stay accurate to what is emitted, not to what was intended.
- `huh clean` and the planner remain worth building, but as *consumers and
  validators* of registry data rather than as the point.

## What would change our mind

If a comparable dataset already exists and was missed, this decision should be
withdrawn and effort redirected to contributing there instead. The search was
one afternoon's work by one person; the preservation community is large and
this is exactly the kind of thing that might live inside an institution's
internal tooling.

If, conversely, measurement shows loss is too **engine-version-specific** to be
useful as published data — if every libvips point release changes the answers —
then the dataset degrades into a snapshot of one machine's software and the
value proposition weakens considerably. Version pinning is recorded per engine
precisely so this is detectable.
