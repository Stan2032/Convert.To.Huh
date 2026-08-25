# 0006 — Local-first, with a computed escape to the server

Status: **accepted** · 2026-08-20

## Context

Client-side conversion is the strongest privacy claim available and the field
is moving there (R§1.1, R§1.2). But the browser has hard ceilings: wasm32 caps
linear memory at 4 GB; OPFS quotas run ~300 MB to a few GB with iOS evicting
aggressively; ffmpeg.wasm is materially slower than native, its multithreaded
core needs cross-origin isolation that breaks embeds and is upstream-flagged as
unstable, and iOS Safari lacks `SharedArrayBuffer` in workers (R§6.1, R§6.3).
VERT ships local-first and punts video to a server — the empirical version of
this trade-off (R§1.2).

## Decision

Default to browser execution. The planner **estimates peak memory and wall time
before executing** and selects the runtime profile. When a job cannot fit
locally, the user is told explicitly that the file must leave their device, and
asked.

WebCodecs is used as the fast path where the codec is supported (3–10× on
H.264), with ffmpeg.wasm as the fallback for coverage (R§6.2).

## Consequences

- The planner must model memory honestly, which forces `memory_model` in every
  engine manifest to be accurate rather than optimistic.
- Two engine matrices to maintain: WASM and native. Divergence is visible in
  the registry rather than hidden in code.
- Cross-origin isolation headers are a deployment constraint on the whole site.
- The consent prompt sits mid-flow. Whether it damages the experience is an
  open question requiring user testing, not argument (R§10.4).

## What would change our mind

If the consent prompt proves to have unacceptable drop-off, the alternative is
an explicit up-front mode choice ("private mode" vs "fast mode") rather than a
per-job interruption.
