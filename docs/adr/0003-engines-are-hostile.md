# 0003 — Engines are hostile and unreliable

Status: **accepted** · 2026-08-20

## Context

Our product is "accept a hostile binary from a stranger and feed it to a C
parser." The record is unambiguous (R§4):

- ImageMagick built shell strings from filenames (ImageTragick, CVE-2016-3714)
  and delegates PDF/PS/EPS/XPS to Ghostscript **by default**.
- Ghostscript has repeatedly escaped `-dSAFER` — CVE-2018-19475,
  CVE-2023-36664 (CVSS 9.8), CVE-2024-29510 exploited in the wild via EPS
  disguised as JPEG.
- Published bypasses defeat ImageMagick's default, "limited" **and** "secure"
  policies.
- LibreOffice crashes on ordinary batches and hangs at 100% CPU on corrupt
  input; its own tooling declines to restart it (R§3.2).

## Decision

Assume every engine is already compromised and will also crash on its own.

1. **Isolation tier by declared hostility** (R§4.4): WASM for engines we
   compile; gVisor as the default for native engines; Firecracker microVM for
   `hostility: extreme`. Declared in the engine manifest, enforced by the
   executor, assigned by `sandbox-warden` — not by the integrator.
2. **No network namespace.** Not a proxy, not an allowlist. None (R§4.5).
3. **Delegation severed.** No delegate binaries on `PATH`; ImageMagick cannot
   reach Ghostscript. PDF handling is an explicit, separately sandboxed hop the
   planner can see.
4. **Budgets before execution.** Bytes, pixels, duration, entry count,
   recursion depth, computed from headers and enforced during the run (R§4.3).
   Exceeding is a normal reported outcome.
5. **Crashing is an expected outcome**, modelled in the manifest
   (`crash_expected`), not an exception.
6. **Engine hardening configs are defence in depth, never the boundary.**

## Consequences

- Higher per-conversion cost. gVisor adds 10–30% on I/O-heavy work but near
  zero on compute-heavy work, which is what conversion is (R§4.4) — so the
  default tier is affordable.
- Some format features become unsupportable by construction: external entities,
  remote stylesheets, HLS segment fetching. These are reported as refused
  features, not silently ignored.
- `sandbox_violation` is never retried and always alerts.

## What would change our mind

Nothing short of engines becoming memory-safe. If an engine is rewritten in a
safe language with a clean history, its hostility class can be lowered — with
evidence, in a PR that `sandbox-warden` reviews.
