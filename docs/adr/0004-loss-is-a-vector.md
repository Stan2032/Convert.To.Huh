# 0004 — Loss is a vector, not a scalar

Status: **accepted** · 2026-08-20

## Context

Polyglot weighted edges by "information retained" as a single scalar, estimated
by round-tripping A→B→A and assigning that one score to **every edge on the
path, averaged across paths** (R§1.3). Two problems follow, and the second is
the fatal one.

A scalar cannot distinguish "kept every pixel, destroyed all text
selectability" from "kept the text, halved the resolution," and it cannot
compose honestly: two conversions losing different things are not
multiplicatively worse at one thing.

Worse, averaging a path score across its edges **destroys credit assignment**.
It cannot say which hop lost what. The authors' defence — that a bad edge
penalizing a good path "should stabilize" once enough other paths traverse it —
is a hope about graph density, not a guarantee, and it fails exactly where it
matters most: a rarely-traversed edge keeps whatever blame it was handed.

Their own results show the cost. Two reasonable similarity measures produced
*different* "best target format" answers on the same 40 models. A single
fidelity number silently encodes a preference about what matters.

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
- Round-trip scoring survives as a *bootstrap technique* where direct
  measurement is impossible (R§1.3), but the score is attributed per-property
  to the specific hop under test, not smeared across the path.

## What would change our mind

If measurement shows users only ever care about one aggregate number, the
vector could collapse to a scalar at the *presentation* layer. It would still
be computed as a vector, because composition requires it.
