#!/usr/bin/env python3
"""Generate corpus specimens deterministically.

`corpus-keeper` prefers generation scripts to committed blobs: a script is
reproducible, tiny in git, unambiguously ours (no licensing risk), and can be
regenerated when a format detail changes. Pure stdlib — PNG is simple enough to
write by hand, which is itself useful knowledge for a format registry.

    python3 corpus/generate.py
"""
import math
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent / "raster"


def chunk(tag: bytes, data: bytes) -> bytes:
    return (struct.pack(">I", len(data)) + tag + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))


def write_png(path: Path, width: int, height: int, bit_depth: int,
              color_type: int, rows: list[bytes]) -> None:
    """color_type: 0=gray, 2=RGB, 6=RGBA."""
    raw = b"".join(b"\x00" + r for r in rows)          # filter type 0 per scanline
    png = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, bit_depth, color_type, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(raw, 9))
           + chunk(b"IEND", b""))
    path.write_bytes(png)
    print(f"  {path.name:<28} {len(png):>7} bytes  {width}x{height} depth={bit_depth} ct={color_type}")


def gen_rgba_gradient(w=128, h=128):
    """8-bit RGBA with a real alpha ramp — exercises `alpha`."""
    rows = []
    for y in range(h):
        row = bytearray()
        for x in range(w):
            row += bytes((int(255 * x / (w - 1)), int(255 * y / (h - 1)), 128,
                          int(255 * (1 - x / (w - 1)))))
        rows.append(bytes(row))
    write_png(OUT / "rgba-gradient.png", w, h, 8, 6, rows)


def gen_rgb_photo(w=192, h=192):
    """8-bit RGB with smooth structure and edges — meaningful for SSIM.

    Flat colour would score ~1.0 through any codec and prove nothing.
    """
    rows = []
    for y in range(h):
        row = bytearray()
        for x in range(w):
            fx, fy = x / w, y / h
            r = 128 + 100 * math.sin(fx * 8) * math.cos(fy * 5)
            g = 128 + 90 * math.sin((fx + fy) * 6)
            b = 128 + 80 * math.cos(fy * 9)
            if (x // 24 + y // 24) % 2 == 0:          # hard edges: JPEG ringing
                r, g, b = r * 0.65, g * 0.65, b * 0.65
            row += bytes((max(0, min(255, int(r))),
                          max(0, min(255, int(g))),
                          max(0, min(255, int(b)))))
        rows.append(bytes(row))
    write_png(OUT / "rgb-photo.png", w, h, 8, 2, rows)


def gen_gray16(w=64, h=64):
    """16-bit greyscale — exercises `color_depth` degradation to 8-bit."""
    rows = []
    for y in range(h):
        row = bytearray()
        for x in range(w):
            row += struct.pack(">H", int(65535 * (x + y) / (2 * (w - 1))))
        rows.append(bytes(row))
    write_png(OUT / "gray16.png", w, h, 16, 0, rows)


def gen_opaque_rgba(w=64, h=64):
    """RGBA whose alpha is entirely opaque.

    The control case: the format supports alpha, this file does not USE it, so
    dropping alpha is not an observable loss. Distinguishing "format supports"
    from "file exercises" is what keeps the fidelity harness honest.
    """
    rows = []
    for y in range(h):
        row = bytearray()
        for x in range(w):
            row += bytes((x * 4 % 256, y * 4 % 256, 200, 255))
        rows.append(bytes(row))
    write_png(OUT / "rgba-opaque.png", w, h, 8, 6, rows)


def gen_png_with_passengers(w=48, h=48):
    """A valid PNG carrying content that is NOT part of the image.

    Sanitization has to be tested against a file that actually has something
    to strip. Every passenger here is legitimate PNG structure or a benign
    appendage — no exploit content — but each one is a real place where data
    rides along unnoticed:

      tEXt      arbitrary key/value metadata, rendered by nothing
      a private ancillary chunk, which decoders skip silently by design
      bytes after IEND, which every decoder ignores and every byte-copy keeps

    The last is the important one: appending after IEND leaves a file that is
    bit-for-bit valid to every viewer while carrying an arbitrary payload.
    """
    rows = []
    for y in range(h):
        row = bytearray()
        for x in range(w):
            row += bytes(((x * 5) % 256, (y * 5) % 256, ((x ^ y) * 3) % 256))
        rows.append(bytes(row))

    raw = b"".join(b"\x00" + r for r in rows)
    png = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
           # Metadata nobody sees but everybody copies.
           + chunk(b"tEXt", b"Author\x00Alice Example")
           + chunk(b"tEXt", b"Comment\x00internal-doc-id-88213-do-not-share")
           # A private ancillary chunk: lowercase first letter = decoders skip it.
           + chunk(b"prVt", b"PASSENGER-PAYLOAD-" + b"A" * 64)
           + chunk(b"IDAT", zlib.compress(raw, 9))
           + chunk(b"IEND", b"")
           # Appended after end-of-image. Structurally invisible; perfectly persistent.
           + b"TRAILER:" + b"B" * 256)

    path = OUT / "png-with-passengers.png"
    path.write_bytes(png)
    print(f"  {path.name:<28} {len(png):>7} bytes  {w}x{h} "
          f"(2 tEXt, 1 private chunk, {8 + 256} trailing bytes)")


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    print("generating corpus specimens:")
    gen_rgba_gradient()
    gen_rgb_photo()
    gen_gray16()
    gen_opaque_rgba()
    gen_png_with_passengers()
    print("done")
