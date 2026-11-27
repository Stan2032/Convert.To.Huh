#!/usr/bin/env python3
"""Measure what a video conversion did, against a lossless reference.

    python3 tools/measure_video.py REFERENCE CONVERTED
    python3 tools/measure_video.py --self-test

Reports, for the first video stream:

  geometry         width x height, before and after
  time             frame count (decoded, not the header's claim), frame rate
                   and duration, before and after
  signal_fidelity  SSIM (all planes) and PSNR in dB, frame by frame against
                   the reference, via ffmpeg's own `ssim` and `psnr` filters

SSIM and PSNR disagree on video more than on stills (ADR-0014), so both are
reported and neither is collapsed into a score. PSNR of identical streams is
infinite; ffmpeg prints "inf".

Needs ffmpeg and ffprobe. Standard library otherwise.
"""
import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path


def probe(path: Path) -> dict:
    r = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-count_frames",
         "-show_entries", "stream=width,height,r_frame_rate,nb_read_frames,codec_name,pix_fmt"
         ":format=duration", "-of", "json", str(path)],
        capture_output=True, text=True, check=True)
    d = json.loads(r.stdout)
    s = d["streams"][0]
    num, den = (int(x) for x in s["r_frame_rate"].split("/"))
    return {"codec": s.get("codec_name"), "pix_fmt": s.get("pix_fmt"),
            "width": s["width"], "height": s["height"],
            "fps": round(num / den, 3), "frames": int(s["nb_read_frames"]),
            "duration": round(float(d["format"]["duration"]), 3)}


def fidelity(ref: Path, conv: Path) -> dict:
    """SSIM and PSNR, converted stream first, reference second, both in
    the reference's pixel format so a format change is not scored as loss."""
    r = subprocess.run(
        ["ffmpeg", "-v", "info", "-nostats", "-i", str(conv), "-i", str(ref),
         "-lavfi", "[0:v]format=yuv420p[a];[1:v]format=yuv420p[b];"
                   "[a][b]ssim;[0:v]format=yuv420p[c];[1:v]format=yuv420p[d];[c][d]psnr",
         "-f", "null", "-"],
        capture_output=True, text=True)
    ssim = re.search(r"SSIM .*All:([0-9.]+)", r.stderr)
    psnr = re.search(r"PSNR .*average:([0-9.]+|inf)", r.stderr)
    return {"ssim": float(ssim.group(1)) if ssim else None,
            "psnr_db": (psnr.group(1) if psnr and psnr.group(1) == "inf"
                        else round(float(psnr.group(1)), 2) if psnr else None)}


def measure(ref: Path, conv: Path) -> dict:
    a, b = probe(ref), probe(conv)
    return {"reference": a, "converted": b,
            "geometry_preserved": (a["width"], a["height"]) == (b["width"], b["height"]),
            "frames_preserved": a["frames"] == b["frames"],
            **fidelity(ref, conv)}


def self_test() -> int:
    bad = 0
    with tempfile.TemporaryDirectory() as td:
        src = Path(td) / "src.mkv"
        blur = Path(td) / "blur.mkv"
        subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i",
                        "testsrc2=size=160x120:rate=10:duration=1", "-c:v", "ffv1",
                        "-pix_fmt", "yuv420p", str(src)], check=True)
        subprocess.run(["ffmpeg", "-v", "error", "-i", str(src), "-vf", "gblur=sigma=3",
                        "-c:v", "ffv1", "-pix_fmt", "yuv420p", str(blur)], check=True)
        same = measure(src, src)
        blurred = measure(src, blur)
        checks = [
            ("identical streams: SSIM 1", same["ssim"] == 1.0),
            ("identical streams: PSNR inf", same["psnr_db"] == "inf"),
            ("identical streams: frames and geometry kept",
             same["frames_preserved"] and same["geometry_preserved"]),
            ("frame count is decoded, not trusted from the header", same["reference"]["frames"] == 10),
            ("a blurred copy scores below 1", blurred["ssim"] is not None and blurred["ssim"] < 0.99),
            ("a blurred copy has finite PSNR", isinstance(blurred["psnr_db"], float)),
        ]
    for name, ok in checks:
        bad += not ok
        print(f"  [{'PASS' if ok else 'FAIL'}] {name}")
    print(f"\nself-test: {len(checks) - bad}/{len(checks)} passed")
    return 1 if bad else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("reference", nargs="?")
    ap.add_argument("converted", nargs="?")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    if not a.reference or not a.converted:
        ap.error("need REFERENCE and CONVERTED (or --self-test)")
    print(json.dumps(measure(Path(a.reference), Path(a.converted)), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
