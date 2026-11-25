# Roadmap

Progress is measured in verified conversions (ADR-0011). Current coverage is
the scope line in [README.md](README.md), which CI checks against the registry.

| Domain | State |
|---|---|
| Raster (PNG, JPEG, GIF, BMP, TIFF, WebP) | Measured across three engines |
| PDF | Sanitization measured; no document fidelity metric yet |
| Audio (WAV, MP3, FLAC) | Measured across two engines, on tonal, broadband and transient specimens |
| Video, office documents | Not started |

## Next, in priority order

1. **A third audio engine** (GStreamer). ffmpeg truncates and SoX rounds on
   bit-depth reduction (R§14.6); a third engine shows which is the outlier.
2. **Rank by `loss_policy`.** Two routes that both lose alpha are currently
   ranked equally, though one turns transparency black and one does not.
3. **A document fidelity metric.** PDF cleaning is verified by the absence of
   payloads, not by how faithful the rebuild is.
4. **Video.** `signal_fidelity` will likely need its metric recorded alongside
   it, because perceptual video metrics diverge sharply (ADR-0014).

## Not planned

- **A browser, server or hosted service** (ADR-0015).
- **Competing with Dangerzone or mat2** (R§12.3b). `huh clean` exists to
  produce registry data.
