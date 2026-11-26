# Roadmap

Progress is measured in verified conversions (ADR-0011). Current coverage is
the scope line in [README.md](README.md), which CI checks against the registry.

| Domain | State |
|---|---|
| Raster (PNG, JPEG, GIF, BMP, TIFF, WebP) | Measured across three engines |
| PDF | Sanitization measured; no document fidelity metric yet |
| Audio (WAV, MP3, FLAC) | Measured across three engines (ffmpeg, SoX, GStreamer), on tonal, broadband and transient specimens |
| Video, office documents | Not started |

## Next, in priority order

1. **Rank by `loss_policy`.** Two routes that both lose alpha are currently
   ranked equally, though one turns transparency black and one does not.
2. **A document fidelity metric.** PDF cleaning is verified by the absence of
   payloads, not by how faithful the rebuild is.
3. **Video.** `signal_fidelity` will likely need its metric recorded alongside
   it, because perceptual video metrics diverge sharply (ADR-0014).

## Not planned

- **A browser, server or hosted service** (ADR-0015).
- **Competing with Dangerzone or mat2** (R§12.3b). `huh clean` exists to
  produce registry data.
