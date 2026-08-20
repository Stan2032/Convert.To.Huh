# 0007 — Build profiles are a first-class concept

Status: **accepted** · 2026-08-20

## Context

FFmpeg is LGPL by default; `--enable-gpl` pulls in libx264 and propagates GPL
obligations to whatever we distribute. Independently, H.264/H.265 carry patent
obligations that apply regardless of which license path we take (R§8). These
two things are routinely conflated, and the conflation is expensive.

## Decision

Two profiles, declared per engine in the registry:

- **`lgpl`** — safe to distribute in a proprietary product.
- **`gpl`** — self-host only; wider codec and engine coverage.

License and `patent_exposure` are **separate fields**, because they are separate
obligations.

The planner prunes engines outside the active profile **before search**, so a
legally unavailable route does not exist rather than being proposed and then
failing at runtime.

## Consequences

- The same request can legitimately yield different plans in different
  distributions, and the receipt records which profile was active.
- Every engine PR requires a license determination — `license-compliance-officer`
  is a required reviewer for `registry/engines/`.
- Some conversions are simply unavailable in the LGPL profile. That is stated
  plainly to the user, not hidden behind a generic failure.

## What would change our mind

If we never distribute a binary — pure hosted SaaS — the LGPL/GPL distinction
weakens considerably. The patent obligations do not weaken at all, so the
`patent_exposure` field survives either way.
