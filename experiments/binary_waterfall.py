#!/usr/bin/env python3
"""Does changing MEDIUM repeatedly hide the content? No. Here is why.

The proposal this tests: convert an image to audio, that audio to an image,
that image to audio again — stripping every extension, filename and metadata
trail — so that without knowing the chain, nobody can tell what to convert it
back into.

Run:  python3 experiments/binary_waterfall.py

Result: the original is recovered byte-identical, in one pass, with no key.

The reason is that reinterpreting bytes as another medium does not change the
bytes. Each hop prepends a header and leaves the payload untouched, so the
original sits at a known offset inside, and file carving — standard forensics,
automated in binwalk and foremost — extracts it immediately.

The alternative, genuine semantic conversion (sonify the image, transcribe the
audio), does change the bytes, but destroys information at every hop, for the
sender as much as for anyone else. There is no third case.

Recorded because this question came up three times in different forms, and an
argument settles it less firmly than a measurement. See ADR-0010.
"""
import hashlib
import pathlib
import struct

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "corpus/raster/rgb-photo.png"


def to_wav(payload: bytes) -> bytes:
    """Reinterpret arbitrary bytes as 8-bit mono PCM. Produces a real WAV."""
    n = len(payload)
    return (b"RIFF" + struct.pack("<I", 36 + n) + b"WAVE"
            + b"fmt " + struct.pack("<IHHIIHH", 16, 1, 1, 8000, 8000, 1, 8)
            + b"data" + struct.pack("<I", n) + payload)


def to_bmp(payload: bytes) -> bytes:
    """Reinterpret arbitrary bytes as pixel data. Produces a real BMP."""
    n = len(payload)
    hdr = b"BM" + struct.pack("<IHHI", 54 + n, 0, 0, 54)
    dib = struct.pack("<IiiHHIIiiII", 40, 64, max(1, n // 192), 1, 24,
                      0, n, 2835, 2835, 0, 0)
    return hdr + dib + payload


SIGNATURES = {
    b"\x89PNG\r\n\x1a\n": "image/png",
    b"RIFF": "RIFF container",
    b"BM": "image/bmp",
    b"\xff\xd8\xff": "image/jpeg",
    b"GIF89a": "image/gif",
}


def main() -> None:
    orig = SOURCE.read_bytes()
    print(f"original          {len(orig):>7} bytes  "
          f"sha={hashlib.sha256(orig).hexdigest()[:16]}")

    # Three changes of medium: image -> audio -> image -> audio.
    chain = [("step1 WAV", to_wav(orig))]
    chain.append(("step2 BMP", to_bmp(chain[-1][1])))
    chain.append(("step3 WAV", to_wav(chain[-1][1])))
    for name, data in chain:
        print(f"{name:<17} {len(data):>7} bytes  "
              f"sha={hashlib.sha256(data).hexdigest()[:16]}")

    blob = chain[-1][1]
    print("\nEvery trail stripped: no extension, no filename hint, no metadata,"
          "\nno key. Now scan it the way any forensics tool would.\n")

    hits = []
    for sig, label in SIGNATURES.items():
        off = 0
        while (i := blob.find(sig, off)) >= 0:
            hits.append((i, label))
            off = i + 1
    for off, label in sorted(hits)[:10]:
        print(f"  offset {off:>6}  {label}")

    # Carve the PNG: header through the end of its IEND chunk.
    start = blob.find(b"\x89PNG\r\n\x1a\n")
    carved = blob[start:blob.find(b"IEND", start) + 8]

    print(f"\ncarved {len(carved)} bytes from offset {start}")
    print(f"  carved   sha={hashlib.sha256(carved).hexdigest()[:16]}")
    print(f"  original sha={hashlib.sha256(orig).hexdigest()[:16]}")
    identical = carved == orig
    print(f"  byte-identical to the original: {identical}")

    assert identical, "carve failed — investigate before trusting this result"
    print("\nThree changes of medium, no metadata, no key, recovered exactly.")


if __name__ == "__main__":
    main()
