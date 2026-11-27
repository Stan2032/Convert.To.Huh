# Dataset changelog

Two version numbers, deliberately separate:

- **`schema_version`** (semantic) changes when the *shape* of
  `conversion-loss-registry.json` changes. Pin it if you parse the file.
- **`registry_version`** (calendar, `YYYY.MM.N`) changes when the *data*
  changes: an edge added, a loss value corrected, a count updated.

## registry 2026.11.4 · schema 1.2.0

**Data.** H.264 and H.265, under a new `ffmpeg-gpl` engine (GPL build,
`gpl` profile only, ADR-0007): x264 lossless (`-qp 0`, SSIM 1.0), x264's
default crf 23 (SSIM 0.9935, 42.47 dB) and x265's default crf 28 (SSIM
0.9850, 38.14 dB at 70% of x264's size). The two defaults, documented by
ffmpeg as visually equivalent, are not equal on either metric here
(docs/RESEARCH.md §20.2).

**Correction (engine note, not exported).** The measured ffmpeg binary was
always a GPL build. Every recorded `ffmpeg` invocation uses only components an
LGPL build contains, so no edge changes; a new validator rule (15) keeps it
that way (§20.1).

## registry 2026.11.3 · schema 1.2.0

**Data.** Video, first entries: Matroska, WebM and MP4 formats, and three
ffmpeg edges measured against a lossless FFV1 reference. VP9 lossless
(SSIM 1.0, the control), VP9 crf 40 (SSIM 0.9916, 41.04 dB) and MPEG-4
Part 2 q8 (SSIM 0.9782, 37.72 dB). Synthetic specimen only: Govdocs1 has no
video. Format signatures may now carry `within`, a search window, to tell
Matroska from WebM by DocType (docs/RESEARCH.md §20).

## registry 2026.11.2 · schema 1.2.0

**Schema 1.2.0 (additive).** `real_world.size_ratio`: output bytes divided
by input bytes over the real files, as `median`, `min` and `max`. A cost, not
a loss, so it sits beside the counts rather than in the loss vector.

**Data.** 17 edges carry it. PNG to JPEG made 23 of 39 real PNGs *larger*
(median 2.1x via libvips, 3.7x via image-rs): most real PNGs are charts with
256 colours or fewer, where PNG compresses far better than JPEG. Lossless
WebP shrinks them (0.77x); uncompressed TIFF and BMP grow them about 25x.

## registry 2026.11.1 · schema 1.1.0

**Correction.** ImageMagick PNG to GIF (`convert IN OUT.gif`) declared
`metadata: lost`; it is `degraded`. ImageMagick carries a PNG's comment text
into the GIF and drops the rest. Found by counting metadata survival on real
files (4 of 6 real PNGs with metadata kept their comment) and reproduced on
the committed specimen, where a comment reading
"internal-doc-id-88213-do-not-share" survives.

**Data.** Every image conversion now carries a real-file `metadata` count:
how many real files had metadata that the conversion actually removed (for
example, all 20 real JPEGs with metadata lost it through image-rs to PNG).

## registry 2026.11.0 · schema 1.1.0

**Schema 1.1.0 (additive).** Edges may carry `real_world`: how the conversion
behaved on real files rather than test specimens. Files run, files converted,
files where a measurement contradicted the declared loss, and per declared
loss the number of files on which it actually occurred. A property absent
from `affected` was not observable on those files, which is not the same as
zero. 20 edges carry it, from 206 real Govdocs1 files.

**Data.**

- 17 formats · 8 engines · 31 conversions (30 verified).
- Office documents: `.doc`, `.xls` and `.ppt` as formats, detected by
  content, and three LibreOffice conversions to PDF, verified on synthetic
  specimens and checked against 60 real documents.
- Audio: GStreamer as a third engine. With dither off it writes samples
  byte-identical to SoX, which makes ffmpeg's truncation the outlier.
- Real-file counts on every verified image conversion, plus two engine
  defects found that way: ImageMagick writes a PNG page offset as a GIF frame
  position outside the visible screen, and libvips caps GIF palettes at 255
  colours, so images with exactly 256 are requantised.

Measurement methods and every correction to them: `docs/RESEARCH.md` §14.7,
§18 to §19.1.

## registry 2026.08.0-alpha · schema 1.0.0

First publication: images, PDF and audio, 24 conversions (23 verified), with
per-edge invocation, loss vector, policy qualifiers, verification status and
the measurement behind it.
