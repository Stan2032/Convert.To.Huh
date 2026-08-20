# 0004 — Loss is a vector, not a scalar

Status: **accepted** · 2026-08-20

## Context

Polyglot weighted edges by "information retained" as a scalar (R§1.3). A scalar
cannot distinguish "kept every pixel, destroyed all text selectability" from
"kept the text, halved the resolution," and it cannot compose honestly: two
conversions losing different things are not multiplicatively worse at one
thing.

The adapter-chaining literature formulates composition as a matrix over
properties (R§1.4).

## Decision

Loss is a vector over a **closed, versioned list of preservable properties**,
each valued `preserved` / `degraded` / `lost` / `synthesized`. Composition is a
per-property join in which `lost` is absorbing and `synthesized` is
contaminating. See `docs/specs/loss-vector.md`.

## Consequences

- Route explanations are **generated** from the vector, never written by hand,
  so they cannot drift from the data.
- Declared vectors are falsifiable hypotheses, checked against SSIM/VMAF/tree-
  edit-distance measurements on corpus specimens; divergence is a registry bug
  raised automatically (R§5.4).
- Ranking becomes multi-objective and needs a weighting policy — currently
  uniform, which is known to be wrong (`loss-vector.md §7`).
- The closed property list is a bottleneck by design: an open list makes
  vectors incomparable across edges.

## What would change our mind

If measurement shows users only ever care about one aggregate number, the
vector could collapse to a scalar at the *presentation* layer. It would still
be computed as a vector, because composition requires it.
