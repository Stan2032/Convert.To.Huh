# Status and what's next

Success here is measured in **verified conversions, not features** (ADR-0011).
Coverage grows by measurement, which is slow on purpose.

## Where things stand

**14 formats · 7 engines · 19 conversions (18 verified) · 44 tests**

| Domain | State |
|---|---|
| Raster (PNG, JPEG, GIF, BMP, TIFF, WebP) | Measured across three independent engines |
| PDF | Sanitization measured; no fidelity metric for documents yet |
| Audio (WAV, MP3, FLAC) | Measured through two engines (ffmpeg, SoX) |
| Video, 3D, Office | Not started |

Working: content-first detection, route planning with ranked alternatives,
`huh clean`, `huh convert`, `huh export`, and a fidelity harness that has
already caught itself being wrong.

## Known gaps, in the order they'd bite

1. **`bit_depth` needs a policy vocabulary, and the evidence is incomplete.**
   ffmpeg and SoX differ by 7.3 dB on 16-to-8-bit reduction with dither disabled
   on both, and nobody has explained why (§14.5). The vocabulary should not be
   written until someone can — otherwise it encodes a guess. Running that gap
   down is the single most useful open task.
2. **Audio specimens are synthetic pure tones** — the easy case for a perceptual
   codec. The 90.3% retention figure is optimistic, not representative.
3. **A third audio engine** would settle whether ffmpeg or SoX is the outlier on
   bit depth, the way ImageMagick settled the alpha question. GStreamer is the
   obvious candidate.
4. **The planner ignores `loss_policy` when ranking.** Two conversions that both
   report `alpha = lost` are treated as equivalent, even though one turns
   transparency black and the other doesn't. The explanation now says which, but
   the *ranking* cannot yet prefer one.
5. **No document fidelity metric.** PDF cleaning is verified by absence of
   payloads, not by any measure of how faithful the rebuild is.
6. **`signal_fidelity` may need its metric recorded alongside it** for video,
   where perceptual metrics diverge sharply (ADR-0014).
7. **Isolation is process-level only.** `subprocess.rs` gives argument vectors,
   timeouts, classified errors and scratch cleanup — not namespaces or seccomp.
   Dangerzone's gVisor-in-a-container is the standard to meet.

## Not planned

**Competing with Dangerzone or mat2.** They do sanitization better and across
more formats (`RESEARCH.md §12.3b`). The tool here exists to produce and
validate registry data. If you need to clean hostile documents in production,
use theirs.

## What would make this project worth other people's time

A hundred well-measured conversions across independent engines, covering the
formats people actually convert between. That is a data-collection problem, not
an engineering one, which is why contributions are measurements rather than
code.
