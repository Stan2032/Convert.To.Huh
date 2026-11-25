# 0015 — Scope is the dataset and the CLI that produces it

Status: **accepted** · 2026-11-25

Supersedes ADR-0002, ADR-0006 and ADR-0008. Follows from ADR-0011.

## Context

ADRs 0002, 0006 and 0008 were written on day one for a product that ADR-0011
later decided not to build: a converter running in the browser and on a
server, with a single planner compiled to WASM, local-first execution with a
computed escape to a server, and durable job orchestration over persisted hops.

None of it was built. Three months on, what exists is a native CLI that
produces and checks `dataset/conversion-loss-registry.json`. The unbuilt design
still cost something:

- Every engine entry carried a `[runtimes]` table recording whether a WASM build
  existed, which no published field and no measurement used.
- The planner filtered on a `--runtime wasm` flag for a runtime that was never
  compiled.
- Four agent definitions (pipeline, WASM runtime, HTTP API, performance) owned
  subsystems that did not exist, and `ARCHITECTURE.md` described a browser
  executor, gVisor and Firecracker tiers, and a resumable hop store as if they
  were present.

A reader could not tell from the documentation which parts were real, which is
the same failure this project records against `claimed` edges.

## Decision

The project's scope is **the dataset, and the native CLI that produces and
validates it**. Specifically:

- ADR-0002 (one core, native and WASM) is superseded. The core is a native Rust
  crate. Nothing prevents a WASM build later; nothing is designed around one.
- ADR-0006 (local-first, computed escape to server) is superseded. There is no
  browser client and no server.
- ADR-0008 (execution as a state machine over persisted hops) is superseded.
  Execution runs hops in sequence within one process and emits a receipt; it
  does not persist or resume.
- `[runtimes]` is removed from engine entries, the validator and the schema
  spec. It was never exported, so the published dataset is unchanged.
- The `--runtime` planner option, the wasm32 CI build and the `huh sanitize`
  command are removed.
  `huh clean` covers sanitization, and now carries `sanitize`'s exit code 4 for
  a rebuild that changed the content.
- Agents for unbuilt subsystems are removed.
- `experiments/` (the cipher refutations) is removed. ADR-0010 keeps the record
  and names the commit where the scripts can be recovered.
  `core/examples/path_channel.rs` stays: it is the measurement `huh clean` rests
  on.

ADR-0007 (build profiles) stays. A dataset that recommends routes must not
recommend one that is illegal to distribute, and licence data is part of what
each engine entry publishes.

## Consequences

- The CLAUDE.md ground rule "the planner is one implementation, compiled to
  native and WASM" is dropped.
- Isolation remains process-level (`core/src/subprocess.rs`). The registry still
  records each engine's hostility and minimum isolation tier, as data for
  consumers; this repository does not enforce those tiers and says so.
- New work is judged by whether it adds or strengthens verified edges.

## What would change our mind

A concrete consumer of the dataset who needs the planner in a browser, for
example a preservation tool that wants to rank routes client-side. That would
justify a WASM build of the planner alone, under a new ADR, without bringing
back the server or orchestration design.
