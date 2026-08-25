# Spec: Content Model

Status: **draft** · Owner: `semantic-bridge-designer` · Evidence: `docs/RESEARCH.md §5`

The type system that makes any→any conversion well-defined instead of arbitrary.

## 1. Formats vs content kinds

A **format** is a container and an encoding. A **content kind** is what travels
inside it. The distinction is what lets us reason about conversions between
domains that share no format lineage.

One format carries one or more kinds:

| Format | Carries |
|---|---|
| `image/png` | `Raster` |
| `image/svg+xml` | `Vector` (+ embedded `Raster`, `Text`) |
| `video/mp4` | `VideoStream` + `AudioPCM` + optional `Text` (subtitles) |
| `application/pdf` | `PageLayout` + usually `Text` + often `Raster`, `Vector` |
| `application/vnd.openxmlformats-officedocument.wordprocessingml.document` | `DocTree` |
| `text/csv` | `Table` |
| `model/gltf+json` | `Mesh3D` |
| `application/zip` | `Container` |

## 2. The kinds

| Kind | Definition | Identity |
|---|---|---|
| `Bytes` | Opaque octet sequence. The bottom type. | digest |
| `Text` | Character sequence + encoding. No structure. | normalized codepoints |
| `Table` | Rows × columns of typed cells, optional header. | cell values + shape |
| `DocTree` | Structured document AST — blocks, inlines, metadata. Pandoc-AST-shaped (R§3.3). | tree shape + leaf content |
| `PageLayout` | Fixed-geometry pages with positioned content. Presentation is committed. | page count + per-page render |
| `Raster` | Pixel grid + color space + optional alpha. | pixels + colorimetry |
| `Vector` | Resolution-independent geometry + style. | path set + style |
| `AudioPCM` | Sample stream: rate, depth, channel layout. | samples |
| `VideoStream` | Time-indexed `Raster` sequence + frame rate. | frames + timing |
| `Mesh3D` | Geometry, topology, materials, optional rig/animation. | vertices + topology |
| `Container` | Named collection of members, each itself typed. | member set (recursive) |

`Bytes` is the bottom type: everything converts to it, it converts to nothing
useful. A route that passes through `Bytes` has lost the plot, and the planner
penalizes it accordingly.

## 3. Operation classes

Every edge in the graph is exactly one of these six. The class determines how
loss is computed and how the operation is presented to the user.

### `Transcode` — same kind, different encoding
JPEG → PNG, WAV → FLAC, MP4/H.264 → MP4/AV1.
Loss is entirely a property of the codec pair. Lossless→lossless preserves
everything; anything involving a lossy encode incurs **generational loss**,
which compounds along a chain and must be modelled as such (R§1.4).

### `Transform` — same kind, different parameters
4K → 1080p, 48 kHz → 44.1 kHz, RGB → grayscale.
Loss is deliberate and quantified by the parameter change itself. Distinct from
`Transcode` because the user asked for it, so it is not reported as damage.

### `Project` — kind A → kind B with defined loss
SVG → PNG (rasterize), DocTree → PageLayout (typeset), Mesh3D → Raster (render).
The defining property: **the mapping is total and deterministic**, and what is
lost is structurally predictable — rasterizing always destroys resolution
independence, typesetting always commits reflowable text to fixed geometry.
Projections are the workhorse of cross-domain routing.

### `Extract` — composite → constituent
MP4 → AudioPCM, PDF → Text, ZIP → members, DOCX → embedded images.
A subset is retained exactly; the rest is dropped. Extraction is the *only*
operation permitted to increase the number of outputs without `Assemble`
(a container yields many members).

### `Assemble` — many → one
Frames → contact sheet, members → archive, pages → document, chapters → EPUB.
Composition is parameterized (layout, ordering, naming) and those parameters
are user-visible intent, not implementation detail.

### `Synthesize` — kind A → kind B by inference
Audio → Text (ASR), Raster → Text (OCR), Text → AudioPCM (TTS),
Raster → Vector (tracing), Raster → Mesh3D (photogrammetry).

**This class is categorically different from the other five.** Its output is an
*estimate about* the input, not a re-expression *of* it. Two consequences, both
mandatory:

- Synthesize hops are marked in the plan, the UI, the output metadata and the
  receipt. A user who receives a transcript must be able to tell that a model
  wrote it (invariant 4, `docs/ARCHITECTURE.md`).
- Synthesize edges carry a confidence estimate, not a loss vector alone, and
  the planner applies a standing penalty so inference never appears in a route
  that a non-inference route could serve.

## 4. Worked example: `AVI → PDF`

The request is underspecified. The planner enumerates; the UI asks (R§5.2).

| Plan | Route | Intent |
|---|---|---|
| Contact sheet | `AVI` →Extract→ `VideoStream` →Transform(sample 1fps)→ `Raster[]` →Assemble(grid)→ `Raster` →Project(typeset)→ `PageLayout` →Transcode→ `PDF` | "show me the video at a glance" |
| Frame per page | `AVI` →Extract→ `VideoStream` →Transform(keyframes)→ `Raster[]` →Project→ `PageLayout` →Transcode→ `PDF` | "I want every distinct shot, full size" |
| Transcript | `AVI` →Extract→ `AudioPCM` →Transform(16 kHz mono WAV)→ `AudioPCM` →**Synthesize(ASR)**→ `Text` →Project→ `DocTree` →Project(typeset)→ `PageLayout` →Transcode→ `PDF` | "I want to read what was said" |
| Storyboard | `AVI` →Extract→ `VideoStream` →Transform(scene detect)→ `Raster[]` →Assemble(+timecodes)→ `DocTree` →Project→ `PageLayout` →Transcode→ `PDF` | "give me a shot list" |

Notes that generalize:

- The transcript route's `Transform(16 kHz mono WAV)` hop is **not optional** —
  whisper.cpp consumes only standard WAV (R§3.6). Engine input constraints are
  edges, not assumptions.
- Every route ends `PageLayout → PDF` as a `Transcode`. Pivot convergence like
  this is what keeps the graph from exploding.
- The four routes differ in *intent*, not in cost alone. Ranking by cost would
  pick the contact sheet every time and be wrong for three users out of four.
  Hence: rank, describe, ask.

## 5. Rules

1. Every edge declares exactly one operation class.
2. `Synthesize` is never silent — plan, UI, metadata, receipt.
3. Kind transitions must be declared; an engine claiming to cross kinds without
   naming the class is an invalid registry entry (CI-enforced).
4. Routes through `Bytes` are penalized heavily and never chosen when any typed
   route exists.
5. Chained lossy `Transcode` hops compound; the cost model composes them as a
   matrix product over properties, not a sum of scalars (R§1.4).
6. When two or more plans differ in *intent* rather than only in cost, the
   planner must surface alternatives rather than silently choosing.

## 6. Open

- Is `Table → AudioPCM` (sonification) a legitimate `Synthesize` edge or a
  novelty that damages trust? Unresolved (R§10.5).
- `PageLayout → DocTree` (un-typesetting) is `Synthesize`, not `Extract` —
  recovering semantic structure from fixed geometry is inference. Confirm this
  classification survives contact with real PDFs, including tagged ones.
