# 0009 — Synthesis is never silent

Status: **accepted** · 2026-08-20

## Context

Cross-domain routing depends on inference edges: ASR for audio→text, OCR for
image→text, TTS for text→audio, tracing for raster→vector (R§3.6, R§5.1). These
are what make "AVI → PDF" produce something a user actually wants.

But their output is an *estimate about* the input, not a re-expression *of* it.
A transcript is not extracted text. A traced SVG is not the original vector art.
A user who cannot tell the difference will act on machine-generated content as
if it were ground truth — and in a document-conversion context, that content
looks exactly like the real thing.

## Decision

`Synthesize` is a distinct operation class (`docs/specs/content-model.md`), and
`synthesized` is a distinct loss value that **contaminates** through composition
(`docs/specs/loss-vector.md`): if any hop invents a property, the chain's output
for that property is invented, regardless of what happens downstream.

Every synthesis is marked in four places: the plan, the UI before the user
commits, the output file's metadata, and the receipt. `contains_synthesis` is
derived, never hand-set.

The planner applies a standing penalty so inference never appears in a route
that a non-inference route could serve.

## Consequences

- Marking is automatic rather than something someone must remember, because it
  falls out of the composition rules.
- The right *durable* marker in the output file is unresolved — XMP metadata, a
  visible cover page, both? Needs legal review before any synthesis edge ships
  (R§10.2).
- Confidence scores must be surfaced honestly, including when they are low.

## What would change our mind

Nothing about the marking. The *presentation* of the marking is open and should
be user-tested.
