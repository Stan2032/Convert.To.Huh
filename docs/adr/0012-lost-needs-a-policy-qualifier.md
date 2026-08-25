# 0012 — `lost` needs a policy qualifier

Status: **accepted** · 2026-08-21

Amends ADR-0004. The four-value vocabulary and the composition algebra stand
unchanged; this adds a descriptive qualifier alongside them.

## Context

ADR-0004 fixed loss as a per-property vector valued `preserved` / `degraded` /
`lost` / `synthesized`. Measurement has now found a case that vocabulary cannot
express, and it was found by the dataset rather than by argument — the first
time that has happened.

Converting RGBA to a format with no alpha channel, three engines were measured
on two structurally different specimens (`docs/RESEARCH.md §13`):

| Engine | Fully transparent pixel becomes | Policy |
|---|---|---|
| libvips 8.15.1 | `(0,0,0)` | composites onto black |
| image-rs 0.25 | original RGB, unchanged | discards the channel |
| ImageMagick 6.9.12 | original RGB, unchanged | discards the channel |

Confirmed by arithmetic, not inference: at half transparency libvips produces
128 × 126/255 ≈ 63 (measured 64), while the others produce 128 (measured 128).
Both match prediction to within JPEG quantisation noise, and both hold
identically on a smooth alpha ramp and on a hard opaque/transparent edge — so
the policy belongs to the **engine**, not to the specimen.

> **CORRECTION (RESEARCH.md §13.8, §13.9).** An earlier draft of this ADR also
> cited SSIM figures showing libvips scoring far worse. Those were invalid: the
> harness compared stored RGB and ignored alpha, so it scored its own choice of
> baseline. Re-measured against a stated background the ranking **reverses**
> depending on the background chosen. **This decision is unaffected**, because it
> rests on the pixel table above — directly observed values, confirmed by
> arithmetic — and never on the scores.

**All three record `alpha = lost`. All three are correct.** And a logo on a
transparent background comes out of libvips as a black box and out of the other
two showing whatever colour sat beneath the transparency.

`lost` says a property is gone. It does not say **what replaced it**, and for
alpha that is the entire question a user has. Two identical loss vectors,
visually incompatible output: someone routing on this dataset would be badly
surprised, which is exactly the failure the loss vector exists to prevent.

## Decision

A loss entry may carry an optional **`policy`** qualifier naming what replaced
the property.

```toml
[edge.loss]
alpha = { value = "lost", policy = "discard_channel" }
```

Emitted in the public dataset as a separate `loss_policy` map, omitted entirely
when empty.

Three constraints keep this from becoming a second, undisciplined vocabulary:

1. **The qualifier is descriptive, never part of the algebra.** Composition
   still joins `LossValue` alone. Composing policies is ill-defined — once alpha
   is gone the second hop's policy is irrelevant — and pretending otherwise
   would corrupt ADR-0004's join.
2. **A policy is only valid where measured.** It is a claim about engine
   behaviour and carries the same verification burden as the edge itself.
3. **Values are drawn from a small documented set per property.** For alpha:
   `composite_black`, `composite_white`, `discard_channel`, `premultiplied`.
   Extending the set is a documentation change, not a schema change.

## Consequences

- **The dataset gains its most user-relevant field.** "This conversion loses
  transparency" is the useful half; "and replaces it with black" is the half
  that determines whether the output is usable.
- **Every engine writing a lossy raster edge now owes a measurement**, not a
  guess. Three engines are recorded; the rest are unmeasured and must stay
  unqualified rather than assumed.
- **The `preserves` list on formats is unaffected.** Whether GIF can carry alpha
  is a property of GIF; what an engine does when it cannot is a property of the
  engine. Keeping them separate is what made this finding legible at all.
- Other properties likely need policies too — `color_depth` (dither or
  truncate?), `text_as_text` (rasterise or drop?). None are measured yet, so
  none are specified. Adding them speculatively would repeat the mistake this
  ADR corrects.
- Schema stays at `1.0.0`: the field is additive and optional, so existing
  consumers are unaffected.

## What would change our mind

If policies turn out to be **configuration rather than engine identity** — if
libvips can be told to discard instead of compositing, and most callers do —
then the field describes a default rather than a behaviour, and it should record
the invocation instead. This is likely true for at least some engines and is the
first thing to test when adding the fourth.

If, conversely, further measurement shows engines agree almost everywhere and
alpha was an isolated case, the qualifier stays but earns its keep only for
alpha, and the general mechanism should not be extended.
