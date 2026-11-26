# Roadmap

Progress is measured in verified conversions (ADR-0011). Current coverage is
the scope line in [README.md](README.md), which CI checks against the registry.

| Domain | State |
|---|---|
| Raster (PNG, JPEG, GIF, BMP, TIFF, WebP) | Measured across three engines |
| PDF | Sanitization measured, including on 40 real PDFs; no document fidelity metric yet |
| Audio (WAV, MP3, FLAC) | Measured across three engines (ffmpeg, SoX, GStreamer), on tonal, broadband and transient specimens |
| Office (DOC, XLS, PPT) | LibreOffice to PDF measured on synthetic specimens and 60 real documents |
| Video | Not started |

## Next, in priority order

1. **A document fidelity metric.** PDF cleaning is verified by the absence of
   payloads, not by how faithful the rebuild is.
2. **Count more losses on real files.** The dataset now publishes how often
   each loss occurred on real files (`real_world`, schema 1.1.0), but only
   where an observation can see it. Signal fidelity, metadata and text are
   not yet counted; each needs an observation that detects the declared
   outcome, not a threshold.
3. **Decide where output size goes.** `huh clean` makes real PDFs a median 23×
   larger (R§18). No tracked property can say so.
4. **Video.** `signal_fidelity` will likely need its metric recorded alongside
   it, because perceptual video metrics diverge sharply (ADR-0014).

## Not planned

- **A browser, server or hosted service** (ADR-0015).
- **Competing with Dangerzone or mat2** (R§12.3b). `huh clean` exists to
  produce registry data.
