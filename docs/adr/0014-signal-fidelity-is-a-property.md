# 0014 — `signal_fidelity` is a tracked property

Status: **accepted** · 2026-08-22

Amends ADR-0004 by adding one entry to the closed property vocabulary. The
four-value scale and the composition algebra are unchanged.

## Context

`docs/RESEARCH.md §13.7` recorded a gap and deliberately left it open: the
vocabulary has **no word for lossy compression**, the single most common form of
loss in the whole project. It was left open on purpose — ADR-0004 had been
amended twice that day, and a third change on an hour-old finding would have
been the haste this repository's discipline exists to prevent. The note said the
proposal wanted fresh evidence, *ideally from audio*, where the same question
arises with a different metric.

That evidence now exists, and it is worse than the raster case suggested.

**Raster.** `PNG → JPEG` through image-rs on an 8-bit specimen: 8 bits in, 8
bits out, geometry and colour profile intact, SSIM 0.9950. The registry records
`color_depth = degraded`, which is simply wrong — nothing about colour depth
changed.

**Audio.** `highfreq-mono.wav → MP3 64k` measured with ffmpeg 6.1.1. Every
tracked property comes back clean:

| Property | Observed | Verdict |
|---|---|---|
| `audio_channels` | 1 → 1 | preserved |
| `sample_rate` | 44100 → 44100 | preserved |
| `bit_depth` | 16 → n/a | not applicable — MP3 has no fixed sample width |
| `time` | 1.00s → 1.00s | preserved |
| `metadata` | none → none | unexercised |

…while **~10% of the signal energy is gone** from both measured bands (1 kHz and
15 kHz, each at 90.3% of the original). FLAC on the same specimen retains
100.0%, so the measurement is sound and the loss is real.

Two independent domains, two different metrics, the same hole. In raster the
loss was being misfiled under `color_depth`; in audio there is nowhere to file
it at all, because `bit_depth` does not even apply to the target format.

## Decision

Add **`signal_fidelity`** to the closed vocabulary:

> Whether the sample values themselves survived. Distinct from how many samples
> there are (`geometry`, `sample_rate`), how wide each one is (`color_depth`,
> `bit_depth`), and how many parallel streams exist (`audio_channels`). This is
> the property lossy compression destroys.

Domain metric, named with every measurement (ADR-0013's lesson): SSIM for
raster, band energy or SNR for audio.

The four values map cleanly, which is a check on the concept rather than a
convenience:

- `preserved` — lossless. FLAC, PNG, `vips webpsave --lossless`.
- `degraded` — lossy re-encode. MP3, JPEG, default WebP.
- `lost` — the signal is gone entirely, not approximated. Audio → transcript
  keeps the words and destroys the waveform.
- `synthesized` — a signal invented by a model. Text-to-speech, image
  upscaling that hallucinates detail. Falls out of ADR-0009 without special
  handling.

**Existing raster edges are corrected**: `color_depth` reverts to meaning only
what it says (16-bit truncated to 8-bit), and lossy re-encoding moves to
`signal_fidelity`. Several edges currently conflate the two and are wrong today.

## Consequences

- **The most common loss in the dataset becomes expressible.** Every lossy edge
  gains a property that actually describes it.
- **`color_depth` becomes trustworthy.** It was carrying two meanings; readers
  could not tell which applied to a given edge without reading the notes.
- **Every existing lossy edge needs re-measuring**, not just re-labelling. The
  numbers were taken with a harness that has since been corrected (§13.8), so
  the labels and the measurements both move.
- **Schema is additive**: `signal_fidelity` is a new key in an existing map, so
  a consumer that ignores it sees exactly what it saw before. Stays at `1.0.0`.
- The vocabulary is now 20 properties. It is *closed*, and each addition makes
  the next one easier to justify by precedent, which is a slope worth naming.
  The bar for the next addition should be the same as this one: **failure
  demonstrated in at least two independent domains**, not one awkward edge.

## What would change our mind

If `signal_fidelity` turns out to be unmeasurable in some domain without
essentially arbitrary choices — the way alpha comparison needed an explicit
background (§13.9) — then it records a preference dressed as a fact, and should
carry its metric and parameters in the same way `loss_policy` carries a policy.
This is likely for video, where perceptual metrics diverge sharply, and should
be checked when video edges are first measured.

If, conversely, it turns out most consumers only want to know *lossy or not*,
the four-value scale on this property is over-engineered, and a boolean would do
the same job with less room for disagreement between measurers.
