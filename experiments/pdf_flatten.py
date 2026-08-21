#!/usr/bin/env python3
"""Flattening a PDF removes active content — and costs you something real.

This is the case where sanitization earns its keep. A PDF can carry code that
runs when the document opens (/OpenAction + /JavaScript), embedded files, and
metadata naming the author and their machine. Antivirus looks for *known* bad
code. Flattening doesn't look for anything: it renders the page to pixels and
builds a fresh PDF around the image, so nothing structural survives — including
attacks nobody has catalogued yet.

Requires: poppler-utils (pdftoppm), libvips (vips).
Run:      python3 experiments/pdf_flatten.py

Two results, and the second matters as much as the first:

  1. Every active-content and metadata marker is gone.
  2. The text is now a picture of text. Not selectable, not searchable, not
     accessible to a screen reader.

That second point is the documented complaint about CDR — output that is safe
but degraded, with no explanation of what changed. It is exactly what the loss
vector exists to report: `text_as_text = lost`. Safety is not free, and a
sanitizer that hides the cost is lying by omission.
"""
import pathlib
import struct
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "corpus/document/active-content.pdf"
MARKERS = [b"JavaScript", b"OpenAction", b"PAYLOAD-EXECUTED",
           b"SECOND-PAYLOAD", b"Alice Example", b"SECRET-WORKSTATION-07"]


def jpeg_size(jpg: bytes) -> tuple[int, int]:
    i = 2
    while i < len(jpg):
        if jpg[i] != 0xFF:
            i += 1
            continue
        marker = jpg[i + 1]
        if marker in (0xC0, 0xC1, 0xC2):
            h = struct.unpack(">H", jpg[i + 5:i + 7])[0]
            w = struct.unpack(">H", jpg[i + 7:i + 9])[0]
            return w, h
        if marker in (0xD8, 0xD9):
            i += 2
            continue
        i += 2 + struct.unpack(">H", jpg[i + 2:i + 4])[0]
    raise ValueError("no JPEG frame header")


def build_pdf(jpg: bytes, w: int, h: int) -> bytes:
    """A fresh PDF containing one image. Nothing is copied from the original."""
    objs = [
        (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
        (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
        (3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 %d %d] /Contents 4 0 R "
            b"/Resources << /XObject << /Im0 5 0 R >> >> >>" % (w, h)),
    ]
    content = b"q %d 0 0 %d 0 0 cm /Im0 Do Q" % (w, h)
    objs.append((4, b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream"))
    objs.append((5, b"<< /Type /XObject /Subtype /Image /Width %d /Height %d "
                    b"/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode "
                    b"/Length %d >>\nstream\n" % (w, h, len(jpg)) + jpg + b"\nendstream"))

    out = bytearray(b"%PDF-1.4\n")
    offsets = {}
    for n, body in objs:
        offsets[n] = len(out)
        out += b"%d 0 obj\n" % n + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
    for n, _ in objs:
        out += b"%010d 00000 n \n" % offsets[n]
    out += (b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n"
            % (len(objs) + 1, xref))
    return bytes(out)


def main() -> None:
    for tool in ("pdftoppm", "vips"):
        if subprocess.run(["which", tool], capture_output=True).returncode:
            sys.exit(f"missing {tool} — apt-get install poppler-utils libvips-tools")

    original = SOURCE.read_bytes()
    tmp = pathlib.Path("/tmp/huh-flatten")
    tmp.mkdir(exist_ok=True)

    # Render through poppler — a parser family independent of whatever wrote it.
    subprocess.run(["pdftoppm", "-r", "150", "-png", str(SOURCE), str(tmp / "page")], check=True)
    page = sorted(tmp.glob("page*.png"))[0]
    subprocess.run(["vips", "jpegsave", str(page), str(tmp / "page.jpg"), "--Q", "90"], check=True)

    jpg = (tmp / "page.jpg").read_bytes()
    w, h = jpeg_size(jpg)
    cleaned = build_pdf(jpg, w, h)
    (tmp / "cleaned.pdf").write_bytes(cleaned)

    print(f"original {len(original):>6} bytes -> cleaned {len(cleaned):>6} bytes "
          f"(page rendered at {w}x{h})\n")
    print(f"{'MARKER':<24} {'ORIGINAL':>9} {'CLEANED':>8}")
    ok = True
    for m in MARKERS:
        a, b = original.count(m), cleaned.count(m)
        ok &= (b == 0)
        print(f"{m.decode():<24} {a:>9} {b:>8}")
    assert ok, "a marker survived flattening — investigate before trusting this"

    print("\nEvery marker removed. But note what it cost:")
    text = subprocess.run(["pdftotext", str(SOURCE), "-"],
                          capture_output=True, text=True).stdout.strip()
    after = subprocess.run(["pdftotext", str(tmp / "cleaned.pdf"), "-"],
                           capture_output=True, text=True).stdout.strip()
    print(f"  selectable text before: {text!r}")
    print(f"  selectable text after:  {after!r}")
    print("\ntext_as_text = lost. The words are still visible to a human, and gone")
    print("to search, copy-paste and screen readers. That is the honest tradeoff,")
    print("and reporting it is the point of the loss vector.")


if __name__ == "__main__":
    main()
