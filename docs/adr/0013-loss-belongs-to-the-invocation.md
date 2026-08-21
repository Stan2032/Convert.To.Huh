# 0013 — Loss belongs to the invocation, not the engine

Status: **accepted** · 2026-08-21

Amends ADR-0011 and ADR-0012. Written the same day as ADR-0012, because
ADR-0012's own falsification condition was met within hours of stating it.

## Context

ADR-0011 established that loss is recorded per `(engine, from, to)` rather than
per `(from, to)`, and measurement across three engines supported it. ADR-0012
then added a `policy` qualifier for what replaces a lost property, and closed
with an explicit caveat:

> If policies turn out to be **configuration rather than engine identity** — if
> libvips can be told to discard instead of compositing, and most callers do —
> then the field describes a default rather than a behaviour, and it should
> record the invocation instead.

That was written as a hypothetical. Two measurements settled it immediately.

**Alpha flattening is a flag.** libvips composites transparent pixels onto black
by default. With `--background 255` the same conversion composites onto white:

| Invocation | Transparent pixel becomes |
|---|---|
| `vips jpegsave IN OUT` | `(0,0,0)` |
| `vips jpegsave IN OUT --background 255` | `(255,255,255)` |

**WebP lossiness is a flag, and the default is the surprising one.** Converting
`rgb-photo.png`:

| Invocation | Bytes | SSIM |
|---|---|---|
| `vips webpsave IN OUT` (default) | 1,942 | **0.8362** |
| `vips webpsave IN OUT --lossless` | 15,512 | 1.0 |
| image-rs WebP encoder | 37,098 | 1.0 |

The second is the more serious finding. A consumer reading "PNG → WebP" from
this dataset — measured through image-rs, lossless, SSIM 1.0 — and then running
libvips in their own pipeline would silently get **0.84 SSIM** output. That is
visible degradation, presented by the dataset as a lossless conversion.

So `(engine, from, to)` is still under-specified. The missing coordinate is
**how the engine was called**.

## Decision

**An edge is a property of `(engine, version, invocation, from, to)`.**

Every edge records the exact `invocation` it was measured with, published in the
dataset and omitted only when genuinely not applicable (an in-process library
with no parameters).

```toml
[[edge]]
engine     = "libvips"
invocation = "vips webpsave IN OUT"
```

Consequently:

- **A loss vector without an invocation is a claim about a default**, and must
  be read as such. ADR-0012's `policy` field stands, re-scoped: it describes
  what *that invocation* does.
- **Different flags on the same engine are different edges**, not variants of
  one. `vips webpsave` and `vips webpsave --lossless` have different loss
  vectors and belong in the dataset separately.
- Defaults are worth measuring precisely *because* they are what most callers
  get. This is not a reason to record only explicit invocations.

## Consequences

- **The dataset's headline claim narrows and gets more honest.** Not "PNG → WebP
  through libvips is lossy" but "PNG → WebP through `vips webpsave` with default
  flags is lossy; add `--lossless` and it is not."
- **Existing edges need auditing.** The image-rs edges were measured through a
  single in-process code path with fixed parameters, so they are well-defined,
  but the quality argument (85) is an invocation detail that should be recorded.
- **Cross-engine comparison must compare like with like.** The libvips-vs-image-rs
  WebP gap is mostly a default-flags gap, not a capability gap. Reporting it as
  "libvips is worse" would be wrong and is exactly the trap §13.3 warned about
  when SSIM was measured against an un-composited baseline.
- Schema stays at `1.0.0`: `invocation` is additive and optional.
- This raises a **coverage problem the project cannot solve by measuring
  harder**. Engines have many flags; the dataset cannot enumerate their product.
  It should record defaults plus invocations that materially change the loss
  vector, and say plainly that it is not exhaustive.

## What would change our mind

If invocations turn out to vary so much between callers that recording defaults
misleads more than it informs, the dataset should stop publishing default-flag
edges and require an explicit invocation for every entry. That would shrink
coverage sharply and is not obviously worth it — most callers do use defaults,
which is why the defaults are the interesting measurement.

## Note on the process

ADR-0012 was written, and its stated falsification condition was met, within the
same day. That is the discipline working rather than failing: the condition was
specific enough to check, someone checked it, and the decision moved. An ADR
whose "what would change our mind" section is unfalsifiable would have quietly
stayed wrong.
