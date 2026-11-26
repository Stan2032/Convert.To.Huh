# Architecture

> Evidence lives in `docs/RESEARCH.md`; references like (R§4.1) point there.
> Scope is set by ADR-0011 and ADR-0015: the dataset, and the native CLI that
> produces and validates it.

## Overview

The registry is a **typed graph**. Formats are nodes; each edge is one engine
invocation that turns one format into another, annotated with what it loses
(a loss vector), what it costs, and whether that was measured. The CLI detects
a file's format from its content, plans routes over the graph, executes them,
checks the result against the declared loss, and exports the graph as a public
dataset.

```
 registry/*.toml ──▶ Registry ──┬──▶ Exporter ──▶ dataset/conversion-loss-registry.json
                                │
 bytes ──▶ Detector ──▶ format ─┴──▶ Planner ──▶ Plan ──▶ Executor ──▶ output + Receipt
                                                              │
                                                       Fidelity check
                                                  (declared vs measured loss)
```

## Components

All in `core/src/`, one Rust crate.

| Module | Role |
|---|---|
| `registry.rs` | Loads `registry/formats`, `engines`, `edges`. |
| `detect.rs` | Content-first detection by signature. The extension is recorded separately and only breaks ties (ADR-0005). |
| `planner.rs` | Dijkstra plus Yen's k-shortest paths over **verified** edges. Engines outside the active build profile are pruned before search (ADR-0007). Every plan carries a plain-language explanation generated from its loss vector. |
| `loss.rs` | The closed property vocabulary and composition rules (`docs/specs/loss-vector.md`). |
| `engines.rs`, `subprocess.rs` | Engine adapters. In-process for image-rs; external programs run with argument vectors, timeouts, classified failures and a self-removing scratch directory. |
| `execute.rs` | Runs a plan hop by hop and emits a receipt: digests, route, declared vs measured loss. |
| `fidelity.rs` | Measures what a conversion actually did, so a declared loss vector can be contradicted. |
| `sanitize.rs`, `clean.rs` | `huh clean`: rebuild a file from parsed content and report what did not survive (ADR-0010). |
| `export.rs` | Emits the dataset against a versioned public schema, decoupled from internal structs. |

## Registry

Declarative TOML, validated by `tools/validate_registry.py` in CI.

- **`formats/`** — identity: id, MIME types, extensions, PRONOM PUID,
  signatures, content kinds, which properties the format can carry.
- **`engines/`** — version pin, licence and build-profile eligibility,
  hostility class and minimum isolation tier, resource behaviour.
- **`edges/`** — one invocation from one format to another: operation class,
  loss vector (with policy qualifiers, ADR-0012), cost, and verification status
  with the specimens and tool that measured it.

Edges are separate from engines because verification is per invocation, not
per engine (ADR-0013). The planner ignores anything not `verified`.

## Operation classes

| Class | Kind change | Example |
|---|---|---|
| Transcode | none | WAV → FLAC |
| Transform | none, deliberately reduced | stereo → mono |
| Project | A → B, defined loss | PDF → PNG |
| Extract | composite → part | MP4 → audio |
| Assemble | many → one | pages → PDF |
| **Synthesize** | A → B by inference | audio → text (ASR) |

`Synthesize` output is estimated, not preserved, and is marked everywhere it
appears (ADR-0009). Full spec: `docs/specs/content-model.md`.

## Isolation

Process-level only: no shell strings, enforced timeouts, scratch cleanup. There
are no namespaces, seccomp or gVisor here. The registry records each engine's
hostility and minimum isolation tier as data for consumers who run engines in
production; this repository does not enforce those tiers. For untrusted
documents, use Dangerzone (R§12.3b).

## Invariants

1. **The extension is never authoritative.** Detection is content-first.
2. **Loss is a vector, never a scalar.**
3. **Every capability claim is verified against a real specimen** before the
   planner trusts it.
4. **Synthesized content is always marked** — plan, output, receipt.
5. **Conversion sandboxes have no network** — a requirement on any deployment
   that runs engines; not enforced by the process-level isolation above.
6. **Legally unavailable routes do not exist**, rather than failing at runtime.
7. **Every plan is explainable in plain language**, generated from its loss
   vector.
