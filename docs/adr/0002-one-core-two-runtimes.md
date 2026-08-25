# 0002 — One core, compiled to native and WASM

Status: **accepted** · 2026-08-20

## Context

We want local-first execution for privacy and cost, and server execution for
work that cannot fit in a browser (R§6.3: wasm32 caps at 4 GB; OPFS quotas run
~300 MB to a few GB). The obvious path is a TypeScript planner in the browser
and a separate server implementation — and they will diverge, silently, and the
divergence will show up as "the browser said this would work and the server
disagreed."

WASI Preview 2 stabilized in early 2026, and the component model lets one
binary target Wasmtime, Node, browsers and the edge; server-side Wasm has
overtaken browser-only use (R§6.4).

## Decision

Registry, detector, planner and plan format are **one Rust crate**, compiled to
native and `wasm32`. Engines differ per runtime; the brain does not.

## Consequences

- The browser's memory/time estimate and the server's are the same computation,
  so the local-vs-remote decision is trustworthy.
- Rust is required of anyone touching the core. Accepted cost.
- Registry data loads identically in both, so a registry version pin means the
  same thing everywhere.
- The WASM bundle carries the registry; it must stay small enough to ship.
  Budget tracked by `wasm-runtime-engineer`.

## What would change our mind

If the WASM core's bundle size or cold-start makes the browser experience worse
than a server round-trip for typical inputs, the local-first premise fails and
this collapses into a server-only design.
