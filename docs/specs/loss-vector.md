# Spec: Loss Vector

Status: **draft** · Owner: `fidelity-analyst` · Evidence: `docs/RESEARCH.md §1.4, §5.3, §5.4`

How we say what a conversion costs, in a form that composes along a chain and
can be checked against measurement.

## 1. Why not a scalar

`quality: 0.8` cannot distinguish "kept every pixel, destroyed all text
selectability" from "kept the text, halved the resolution." Users care about
*which* thing was lost, and route explanations must name it. A scalar also
cannot compose honestly: two conversions that each lose a different property
are not "0.64 quality," they have lost two different things.

## 2. Preservable properties

The vector is indexed by property. Each entry is one of `preserved`,
`degraded`, `lost`, or `synthesized`.

| Property | Meaning | Typically lost by |
|---|---|---|
| `geometry` | Spatial resolution / sample density | downscaling, rasterization |
| `color_depth` | Bits per channel | 16→8 bit conversion |
| `color_profile` | ICC profile / colorimetry | naive RGB conversion |
| `alpha` | Transparency | JPEG output, flattening |
| `time` | Temporal dimension, frame timing | video → still |
| `layers` | Independent compositing layers | PSD → PNG, flattening |
| `text_as_text` | Text remains selectable/searchable | rasterization, outlining fonts |
| `structure` | Semantic structure — headings, lists, tables | DocTree → PageLayout, PDF export |
| `metadata` | EXIF, XMP, document properties | most transcodes, deliberately or not |
| `fonts` | Embedded font programs | format lacking font embedding |
| `interactivity` | Links, forms, scripts, annotations | flattening, print-style export |
| `signal_fidelity` | Whether the sample values themselves survived — the property lossy compression destroys | PNG → JPEG; WAV → MP3 |
| `audio_channels` | Channel count and layout | 5.1 → stereo downmix |
| `sample_rate` | Audio temporal resolution | 48 kHz → 44.1 kHz |
| `bit_depth` | Audio amplitude resolution | 24-bit → 16-bit |
| `precision` | Numeric precision of coordinates/values | float → int quantization |
| `topology` | Mesh connectivity, manifoldness | triangulation, decimation |
| `materials` | Surface appearance, textures, shaders | format lacking PBR support |
| `rigging` | Skeleton, skinning, animation | static-geometry-only export |
| `provenance` | Origin/history information | any hop that drops metadata |

The list is closed and versioned. Adding a property is a registry schema
change reviewed by `fidelity-analyst` — an open-ended list makes vectors
incomparable across edges, which defeats the purpose.

## 3. Values

| Value | Meaning | Reversible |
|---|---|---|
| `preserved` | Bit-exact or perceptually indistinguishable by the property's own metric | yes |
| `degraded` | Present but reduced; a magnitude accompanies it | no |
| `lost` | Not representable in the target; gone | no |
| `synthesized` | **Not present in the input** — inferred by a model | n/a — it was never there |

`synthesized` is not a degree of loss, it is a different axis. A transcript's
`text_as_text` is not "degraded"; it is invented from audio. Conflating the two
is how a user ends up trusting an ASR transcript as if it were extracted text.

`degraded` carries a magnitude: `{ value = "degraded", ratio = 0.5 }` for a
half-resolution downscale, `{ value = "degraded", from = 24, to = 16 }` for
bit depth. Magnitudes are property-specific and defined in the registry schema.

## 4. Composition

Loss composes along a chain, and **not by addition**. Per the adapter-chaining
formulation (R§1.4), composition is a per-property join:

Composition is the **maximum under a total order**, which is what makes it
associative and order-independent:

```
preserved  <  degraded  <  lost  <  synthesized
```

```
preserved  ∘ X           = X
degraded   ∘ preserved   = degraded
degraded   ∘ degraded    = degraded, magnitudes multiply
lost       ∘ preserved   = lost          (absorbing — you cannot un-lose)
lost       ∘ degraded    = lost
lost       ∘ synthesized = synthesized   (contamination beats absorption)
X          ∘ synthesized = synthesized   (contaminating — inference wins)
```

**`synthesized` outranks `lost`, in either operand order.** This case needs
stating because the two headline rules — `lost` absorbs, `synthesized`
contaminates — both match `lost ∘ synthesized` and, read alone, disagree about
the answer. The tie is broken toward `synthesized` because the values answer
different questions and one is more urgent: `lost` says the property is missing,
while `synthesized` says *what is there was invented by a model*. A chain that
destroys a property and then has a model invent a replacement must report the
invention (ADR-0009), because presenting inferred content as recovered content
is the failure this vocabulary exists to prevent. Reporting `lost` would be the
safer-sounding answer and the more dangerous one.

Enforced by `LossValue::rank` in `core/src/loss.rs`, and tested in both operand
orders.

Two consequences that matter:

- **`lost` is absorbing.** Once alpha is gone, no later hop restores it. A
  planner that sums scalar costs would happily route through a flattening hop
  and then an alpha-capable format and report a mild total. The join says the
  truth: alpha is gone.
- **`synthesized` is contaminating.** If any hop in a chain invents a property,
  the chain's output for that property is invented, regardless of what happens
  downstream. This is what makes provenance marking automatic rather than a
  thing someone has to remember to do.

Generational loss is the exception to the simple join: repeated lossy
`Transcode` hops on the same property degrade faster than the product of their
individual ratios. Edges declare a `generational` flag; the composer applies an
additional penalty when two flagged edges touch the same property in sequence.

## 5. Declared vs measured

A loss vector in the registry is a **hypothesis**. The fidelity harness tests it
against real specimens from the corpus:

| Kind | Metric | Notes |
|---|---|---|
| `Raster`, `VideoStream` | SSIM, VMAF | VMAF correlates with human MOS at ~0.93–0.96 vs ~0.6–0.7 for PSNR (R§5.4). PSNR recorded but never the deciding metric. |
| `AudioPCM` | Spectral distance, PEAQ-family where available | |
| `Text` | Normalized edit distance | after Unicode normalization |
| `DocTree` | Tree edit distance over the AST | structure-aware, not textual |
| `PageLayout` | Rendered-page SSIM | normalize through a common renderer first (R§3.4) |
| `Mesh3D` | Hausdorff distance, topology invariants | |

**A measured result that contradicts the declared vector is a registry bug**,
raised automatically, assigned to `capability-cartographer`. This is the loop
that the Polyglot work (R§1.3) left open, and it is the reason the declared
values are allowed to start as expert judgement: they are falsifiable.

## 6. Explanation generation

Route rationales are **generated from the vector**, never hand-written — a
hand-written rationale drifts from the data it describes.

```
lost:      [alpha, text_as_text]
degraded:  [geometry ×0.5]
preserved: [color_profile, metadata]

→ "Loses transparency and text selectability. Halves resolution.
   Keeps color accuracy and metadata."
```

If a plan cannot be explained this way, it cannot be ranked (invariant 8).

## 7. Open

- Property weights for ranking are currently uniform. They should probably
  depend on the target intent — `text_as_text` matters enormously for a
  document and not at all for a poster. Needs a weighting model.
- How many corpus specimens per edge before measured values replace declared
  ones? Unknown; `fidelity-analyst` to determine empirically (R§10.1).
