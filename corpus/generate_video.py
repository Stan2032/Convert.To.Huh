#!/usr/bin/env python3
"""Generate video specimens with ffmpeg's built-in test sources.

    python3 corpus/generate_video.py      # needs ffmpeg

reference.mkv  2 s of `testsrc2` (moving gradients, text, a sweeping clock),
               320x240 at 25 fps, encoded LOSSLESSLY as FFV1 in Matroska.
               Every lossy edge is measured against it, so it must carry no
               loss of its own. `-bitexact` and stripped metadata make the
               bytes reproducible on the same ffmpeg version.

Synthetic, as all video here is: the Govdocs1 tier contains no video files
(docs/RESEARCH.md §20).
"""
import subprocess
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent / "video"


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    dest = OUT / "reference.mkv"
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y",
         "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25:duration=2",
         "-c:v", "ffv1", "-level", "3", "-pix_fmt", "yuv420p",
         "-map_metadata", "-1", "-fflags", "+bitexact", "-flags", "+bitexact",
         str(dest)],
        check=True,
    )
    print(f"  {dest.name:<16} {dest.stat().st_size:>8} bytes  320x240 25fps 2s FFV1 (lossless)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
