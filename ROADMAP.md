# Roadmap

Progress is measured in verified conversions (ADR-0011). Current coverage is
the scope line in [README.md](README.md), which CI checks against the registry.

| Domain | State |
|---|---|
| Raster (PNG, JPEG, GIF, BMP, TIFF, WebP) | Measured across three engines |
| PDF | Sanitization measured on 40 real PDFs, including visual fidelity of the rebuild (stored and as viewed) |
| Audio (WAV, MP3, FLAC) | Measured across three engines (ffmpeg, SoX, GStreamer), on tonal, broadband and transient specimens |
| Office (DOC, XLS, PPT) | LibreOffice to PDF measured on synthetic specimens and 60 real documents |
| Video | Not started |

## Next, in priority order

1. **Count more losses on real files.** The dataset now publishes how often
   each loss occurred on real files (`real_world`, schema 1.1.0), but only
   where an observation can see it (now including metadata). Signal fidelity and text are
   not yet counted; each needs an observation that detects the declared
   outcome, not a threshold.
2. **Video.** `signal_fidelity` will likely need its metric recorded alongside
   it, because perceptual video metrics diverge sharply (ADR-0014).

## Not planned

- **A browser, server or hosted service** (ADR-0015).
- **Competing with Dangerzone or mat2** (R§12.3b). `huh clean` exists to
  produce registry data.
