# Architecture

> Read `docs/RESEARCH.md` first. This document is the design; that one is the
> evidence. Section references like (R§4.1) point into the dossier.

## The one-paragraph version

Every file converter in the field is a **lookup table**: a map from
(source format, target format) to a command line. Convert.To.Huh is a
**planner over a typed graph**. Formats and the content kinds they carry are
nodes; engine capabilities are typed, weighted edges. A conversion request is a
pathfinding problem whose answer is a ranked set of *plans*, each annotated
with what it will cost you in fidelity, time, memory and trust. Plans execute
identically in a browser tab or on a server, because the planner is one piece
of code compiled to both targets.

## Why this shape

Three forces determine the architecture.

1. **N² is impossible** (R§1.5). A thousand formats is a million pairs. Only a
   graph with high-degree pivot nodes is tractable — but a strict hub-and-spoke
   would route JPEG→PNG through a pivot and lose to a direct edge, so it must be
   a *graph with pivots*, planned per-request, not a mandated topology.
2. **Any→any is a semantics problem** (R§5). "AVI → PDF" has at least four
   defensible answers. A lookup table cannot express that; it can only pick one
   silently. A planner can enumerate them, rank them, and ask.
3. **Every engine is hostile or fragile, usually both** (R§4, R§3.2).
   Ghostscript has repeatedly escaped its own sandbox; ImageMagick delegates to
   it by default; LibreOffice crashes on ordinary workloads. The execution
   layer must assume the engine is compromised and the input is an attack.

## Components

```
                        ┌─────────────────────────────────────────┐
   bytes in ───────────▶│  DETECTOR                               │
                        │  signatures → structure → ML (advisory) │
                        └──────────────────┬──────────────────────┘
                                           │ FormatId + observed properties
                                           ▼
   ┌──────────────┐        ┌─────────────────────────────────────┐
   │  REGISTRY    │───────▶│  PLANNER                            │
   │  formats     │  graph │  k-shortest paths over typed edges  │
   │  engines     │        │  multi-objective cost               │
   │  edges       │        │  runtime-profile aware              │
   └──────────────┘        └──────────────────┬──────────────────┘
                                              │ ranked Plan[]
                              ┌───────────────┴───────────────┐
                              │  (ambiguous? ask the user)    │
                              └───────────────┬───────────────┘
                                              ▼
                        ┌─────────────────────────────────────────┐
                        │  EXECUTOR                               │
                        │  hop-by-hop, budgeted, sandboxed        │
                        │  browser (WASM) │ server (gVisor/µVM)   │
                        └──────────────────┬──────────────────────┘
                                           ▼
                        ┌─────────────────────────────────────────┐
                        │  LEDGER — receipt: route, loss, metrics │
                        └─────────────────────────────────────────┘
```

### 1. Registry — the data, and the whole point

`registry/` is declarative data, not code, and it is the most valuable artifact
in the repository. Three kinds of entry:

- **`registry/formats/*.toml`** — a format's identity: canonical id, MIME
  types, extensions, PRONOM PUID (R§2.2), detection signatures, the content
  kinds it can carry, the properties it can preserve.
- **`registry/engines/*.toml`** — a backend's identity: version pinning,
  license and build-profile eligibility (R§8), hostility class and required
  isolation tier (R§4.4), resource behaviour.
- **`registry/edges/*.toml`** — a capability: *this engine* can turn *this
  input* into *this output* via *this operation class*, losing *these
  properties*, at *this cost*. Edges are the graph.

Splitting edges out of engines is deliberate: an edge is a claim that must be
**empirically verified** (R§3.5 — Assimp's docs promise more than the code
delivers), and verification status is per-edge, not per-engine.

Formats and engines are joined only through edges, so adding an engine never
requires touching a format file, and a format can be added with zero edges (it
becomes detectable but not convertible — which is honest and useful).

### 2. Detector — content first, always

Layered, in order, per R§2:

1. **Structural signatures** — magic bytes and container probing, with PRONOM-
   style priority rules so DOCX beats ZIP when both match.
2. **Deep inspection** — for containers, enumerate what is actually inside;
   an MP4 with no video track is not a video.
3. **ML classification** (advisory) — Magika-class model for textual and
   ambiguous cases where signatures are structurally incapable. May raise
   confidence or break ties; **may never override a decisive structural parse**,
   because every detection must be explainable to the user.

The extension is a tie-breaker of last resort and is recorded separately from
the detected identity, so `.jpg` containing EPS (R§4.2 — a real, in-the-wild
exploit) is visible as a mismatch rather than silently trusted.

### 3. Planner — the differentiator

Input: source `FormatId` + observed properties, target format (or target
*intent*), runtime profile, policy. Output: **ranked `Plan`s**.

- **Search**: k-shortest loopless paths (Yen over Dijkstra) so alternatives are
  available, not just the optimum — because the alternatives are the product
  (R§5.2).
- **Cost**: multi-objective, not a scalar. Fidelity loss (vector, R§5.3), wall
  time, peak memory, isolation cost, license eligibility, and a synthesis
  penalty that keeps inference edges out of routes that don't need them.
- **Hard constraints prune the graph before search**: an engine outside the
  active build profile (R§8) or above the permitted hostility class is *not in
  the graph*, so illegal routes cannot be proposed and then fail at runtime.
- **Runtime-profile awareness**: the same query planned for `browser` and
  `server` yields different graphs, because WASM engine coverage is narrower
  and the memory ceiling is 4 GB (R§6.3). The planner estimates peak memory and
  decides *before* execution whether the job fits on-device.
- **Explainability is a hard requirement**: every plan carries a generated
  plain-language rationale derived from its loss vector. If a plan cannot be
  explained, it cannot be ranked.

### 4. Executor — assume the engine is already compromised

A plan is a sequence of hops. Each hop:

- runs in an isolation tier determined by the engine's declared hostility
  (R§4.4): WASM → gVisor → Firecracker microVM;
- has **no network namespace at all** — not a proxy, not an allowlist (R§4.5);
- gets a read-only rootfs, a scratch dir, and no delegate binaries on `PATH`,
  so ImageMagick physically cannot reach Ghostscript (R§4.1);
- carries an **output budget** — bytes, pixels, duration, entry count,
  recursion depth — computed from declared headers before execution and
  enforced during it (R§4.3). Exceeding budget is a normal reported outcome,
  not a crash;
- has a wall-clock kill, and *crashing is an expected outcome*, especially for
  LibreOffice (R§3.2), which runs one document at a time in its own supervised
  pool and never shares a worker.

Intermediates are persisted per hop and content-addressed, so a failed 5-hop
plan resumes at hop 4 and a re-run of the same input is free (R§7.1).

### 5. Ledger — the receipt

Every conversion emits a machine-readable record: input digest and detected
identity, the plan taken (with registry version pinned), per-hop engine
versions, declared vs **measured** fidelity, budgets consumed, and explicit
flags for any `Synthesize` hop. Two reasons this is a component and not a log
line: users deserve to know a model wrote their transcript (R§5.1), and
declared-vs-measured divergence is how registry bugs get found automatically
(R§5.4).

## The content model in one table

Formats are containers; content kinds are what travels inside. Six operation
classes connect them (full spec: `docs/specs/content-model.md`):

| Class | Kind change | Information | Example |
|---|---|---|---|
| Transcode | none | preserved or re-encoded | JPEG → PNG |
| Transform | none | deliberately reduced | 4K → 1080p |
| Project | A → B, defined loss | structurally lost | SVG → PNG |
| Extract | composite → part | subset retained | MP4 → AudioPCM |
| Assemble | many → one | composed | frames → contact sheet |
| **Synthesize** | A → B by inference | **estimated, not preserved** | audio → text (ASR) |

`Synthesize` is the class the rest of the field lacks a name for, and the one
that makes any→any real. It is marked distinctly everywhere it appears.

## Technology choices

| Layer | Choice | Why |
|---|---|---|
| Core (registry, planner, plan format) | **Rust**, compiled to native + `wasm32` | One planner, two runtimes, no drift. WASI P2 and the component model make this the well-supported path (R§6.4). |
| Registry data | **TOML**, schema-validated in CI | Hand-editable, reviewable diffs; the data is the asset, so it must be legible to humans. |
| Raster engine | **libvips** default, ImageMagick only for gaps | ~4–8× faster, ~200 MB vs ~3 GB memory (R§3.1). |
| Document AST | **Pandoc** | Its AST *is* our `DocTree`; don't reinvent it (R§3.3). |
| Media | **FFmpeg**, LGPL profile by default | GPL only via `--enable-gpl`; libx264 is GPL (R§8). |
| Browser video fast path | **WebCodecs**, ffmpeg.wasm fallback | 3–10× on H.264; narrow codec coverage means both are needed (R§6.2). |
| Browser storage | **OPFS** | Real streaming filesystem; quota-aware (R§6.3). |
| Isolation | WASM / gVisor / Firecracker by declared hostility | Matches threat to cost (R§4.4). |
| Orchestration | Explicit state machine over persisted hops; queue swappable | Plans are chained steps with expensive intermediates — the durable-execution case (R§7.1). |

## Repository layout

```
docs/            RESEARCH.md (evidence) · ARCHITECTURE.md (this) · specs/ · adr/
registry/        formats/ · engines/ · edges/        ← the asset
tools/           validate_registry.py and friends    ← executable definition of done
.claude/agents/  the roles that maintain all of the above
```

## Invariants

These hold everywhere. Violating one is a bug regardless of what else works.

1. **The extension is never authoritative.** Detection is content-first.
2. **Loss is a vector, never a scalar.** No `quality: 0.8` anywhere.
3. **Every capability claim is verified against a real specimen** before it is
   trusted by the planner.
4. **Synthesized content is always marked** — in the plan, the UI, the output
   metadata, and the receipt.
5. **Conversion sandboxes have no network.**
6. **Legally unavailable routes do not exist**, rather than failing at runtime.
7. **The planner is one implementation.** Browser and server never diverge.
8. **Every plan is explainable in plain language**, generated from its loss
   vector.

## What is deliberately not decided yet

Frontend framework, hosting, database, auth, pricing. None of them constrain
the core, and deciding them early would be guessing. See `ROADMAP.md` for
sequencing and `docs/RESEARCH.md §10` for the genuinely open questions.
