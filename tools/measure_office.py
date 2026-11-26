#!/usr/bin/env python3
"""Measure what LibreOffice's office-to-PDF export loses, on real documents.

    python3 tools/fetch_govdocs.py          # once
    python3 tools/measure_office.py         # writes corpus/govdocs1/office.json
    python3 tools/measure_office.py --dir corpus/office --out -   # synthetic specimens
    python3 tools/measure_office.py --self-test

For each Govdocs1 .doc/.xls/.ppt it runs `soffice --convert-to pdf` and reports:

  word_recall    share of the source's distinct words found in the PDF text
  number_recall  the same for numbers, after removing digit-grouping commas
  pages          page count of the PDF
  tagged         whether the PDF carries a structure tree (pdfinfo "Tagged")
  fonts          font families the source requests, the PDF embeds, and
                 whether any requested family survived

REFERENCE TEXT comes from an independent extractor (catdoc, xls2csv, catppt),
not from LibreOffice, so the engine is not grading its own work. A synthetic
specimen with a <stem>.truth.txt beside it is compared against that instead.

THREE WAYS THIS INSTRUMENT WAS WRONG FIRST (docs/RESEARCH.md §19):

1. Counting word occurrences: catppt prints slide text several times over, so
   a faithful PDF scored 0.11. Recall is over DISTINCT words.
2. Counting numbers as words: a spreadsheet stores 2179678, the PDF prints
   2,179,678. Grouping commas are removed before comparing; what still
   differs after that is display rounding, i.e. real `precision` loss.
3. Hidden field codes: catdoc prints table-of-contents bookmark ids
   (_Toc124583466) that are never visible, which reads as missing numbers in
   .doc files. Recorded, not corrected: number_recall is reported for .doc but
   should not be read as a loss there.
4. Fonts LibreOffice invents: its flat-ODF font list includes its own
   defaults (Liberation, DejaVu, FreeSans, Noto, Inter), so a substitute
   counted as a surviving requested font. A name now counts as requested only
   if it occurs in the source file's own bytes, as ASCII or UTF-16LE, as a
   whole name: "Inter" is a substring of "international".

Needs LibreOffice, poppler-utils and catdoc. Standard library otherwise.
"""
import argparse
import json
import re
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FILES = ROOT / "corpus" / "govdocs1" / "files"
OUT = ROOT / "corpus" / "govdocs1" / "office.json"

EXTRACT = {"doc": ["catdoc", "-w"], "xls": ["xls2csv"], "ppt": ["catppt"]}
FLAT = {"doc": "fodt", "xls": "fods", "ppt": "fodp"}
MIN_TOKENS = 20  # below this a recall figure is noise, so none is reported


def normalise(text: str) -> str:
    """Lower-case and drop digit-grouping commas: 2,179,678 -> 2179678."""
    return re.sub(r"(?<=\d),(?=\d{3}\b)", "", text.lower())


def words(text: str) -> set[str]:
    return set(re.findall(r"[a-z][a-z0-9]{2,}", normalise(text)))


def numbers(text: str) -> set[str]:
    return set(re.findall(r"\d+(?:\.\d+)?", normalise(text)))


def recall(src: set[str], dst: set[str]) -> float | None:
    return round(len(src & dst) / len(src), 4) if len(src) >= MIN_TOKENS else None


def family(name: str) -> str:
    """'ABCDEF+LiberationSerif-BoldItalic' -> 'liberationserif'."""
    name = re.sub(r"^[A-Z]{6}\+", "", name)
    name = re.split(r"[-,]", name)[0]
    return re.sub(r"(bold|italic|oblique|regular|mt|ps)+$", "", name.replace(" ", "").lower())


def in_source(raw_lower: bytes, name: str) -> bool:
    """True if a font name occurs in the source bytes as a whole name, in
    ASCII or UTF-16LE. Whole-name matters: "Inter" is inside "international"."""
    pat = re.compile(r"(?<![a-z-])" + re.escape(name.lower()) + r"(?![a-z-])")
    texts = [raw_lower.decode("latin-1")]
    texts += [raw_lower[i:].decode("utf-16-le", "ignore") for i in (0, 1)]
    return any(pat.search(t) for t in texts)


def run(cmd, timeout=180) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, timeout=timeout)


def measure(path: Path, work: Path, profile: str) -> dict:
    ext = path.suffix[1:].lower()
    soffice = ["soffice", f"-env:UserInstallation=file://{profile}", "--headless"]
    row: dict = {"file": path.name, "type": ext}

    r = run(soffice + ["--convert-to", "pdf", "--outdir", str(work), str(path)])
    pdf = work / (path.stem + ".pdf")
    if not pdf.is_file():
        row["error"] = r.stderr.decode("utf-8", "replace").strip()[-200:] or f"exit {r.returncode}"
        return row

    truth = path.with_suffix(".truth.txt")
    if truth.is_file():  # synthetic specimen: the content is known exactly
        src = truth.read_text("utf-8")
        row["reference"] = "truth"
    else:
        src = run(EXTRACT[ext] + [str(path)]).stdout.decode("utf-8", "replace")
        row["reference"] = EXTRACT[ext][0]
    out = run(["pdftotext", "-q", str(pdf), "-"]).stdout.decode("utf-8", "replace")
    row["source_words"] = len(words(src))
    row["word_recall"] = recall(words(src), words(out))
    row["number_recall"] = recall(numbers(src), numbers(out))
    info = run(["pdfinfo", str(pdf)]).stdout.decode()
    row["pages"] = int(m.group(1)) if (m := re.search(r"Pages:\s+(\d+)", info)) else None
    row["tagged"] = bool(re.search(r"Tagged:\s+yes", info))

    embedded = {family(line.split()[0])
                for line in run(["pdffonts", str(pdf)]).stdout.decode().splitlines()[2:]
                if line.strip()}
    run(soffice + ["--convert-to", FLAT[ext], "--outdir", str(work), str(path)])
    flat = work / f"{path.stem}.{FLAT[ext]}"
    requested = set()
    if flat.is_file():
        xml = flat.read_text("utf-8", "replace")
        raw = path.read_bytes().lower()
        for n in set(re.findall(r'<style:font-face svg:font-family="&apos;?([^"&]+)', xml)
                     + re.findall(r'<style:font-face style:name="([^"]+)"', xml)):
            n = re.sub(r"\d+$", "", n).strip()  # LibreOffice dedupes as "Arial1"
            if in_source(raw, n):
                requested.add(family(n))
        flat.unlink()
    row["fonts_requested"] = sorted(requested)
    row["fonts_embedded"] = sorted(embedded)
    row["requested_font_survived"] = bool(requested & embedded)
    pdf.unlink()
    return row


def summarise(rows: list[dict]) -> dict:
    out = {}
    for ext in EXTRACT:
        rs = [r for r in rows if r["type"] == ext]
        ok = [r for r in rs if "error" not in r]
        wr = sorted(r["word_recall"] for r in ok if r.get("word_recall") is not None)
        nr = sorted(r["number_recall"] for r in ok if r.get("number_recall") is not None)
        out[ext] = {
            "files": len(rs),
            "converted": len(ok),
            "word_recall": {"n": len(wr), "median": statistics.median(wr), "min": wr[0]} if wr else None,
            "number_recall": {"n": len(nr), "median": statistics.median(nr), "min": nr[0]} if nr else None,
            "no_requested_font_embedded": sum(1 for r in ok if r["fonts_requested"]
                                              and not r["requested_font_survived"]),
            "with_requested_fonts": sum(1 for r in ok if r["fonts_requested"]),
            "tagged": sum(1 for r in ok if r.get("tagged")),
        }
    return out


def self_test() -> int:
    cases = [
        (normalise("2,179,678 and 1,000"), "2179678 and 1000"),
        (normalise("1,5 stays"), "1,5 stays"),  # decimal comma, not grouping
        (sorted(numbers("x 0.25 and 3,000")), ["0.25", "3000"]),
        (sorted(words("The QCD the qcd")), ["qcd", "the"]),  # distinct, not counted
        (recall({"a"} | {str(i) for i in range(30)}, {str(i) for i in range(15)}), round(15 / 31, 4)),
        (recall({"a", "b"}, {"a"}), None),  # too few tokens to report
        (family("BAAAAA+LiberationSerif-BoldItalic"), "liberationserif"),
        (family("Times New Roman"), "timesnewroman"),
        (family("ArialMT"), "arial"),
        (in_source(b"..t\x00i\x00m\x00e\x00s\x00..", "Times"), True),
        (in_source(b"...arial...", "Arial"), True),
        (in_source(b"...arial...", "Liberation Serif"), False),
        (in_source(b"...international...", "Inter"), False),
        (in_source(b"...inter-modal...", "Inter"), False),
    ]
    bad = 0
    for got, want in cases:
        ok = got == want
        bad += not ok
        print(f"  [{'PASS' if ok else 'FAIL'}] {got!r} == {want!r}")
    print(f"\nself-test: {len(cases) - bad}/{len(cases)} passed")
    return 1 if bad else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--dir", type=Path, default=FILES, help="specimen directory")
    ap.add_argument("--out", default=str(OUT), help="results file, or - for stdout only")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    files = sorted(p for p in args.dir.iterdir() if p.suffix[1:].lower() in EXTRACT)
    if not files:
        sys.exit("no office files: run tools/fetch_govdocs.py")
    build = run(["soffice", "--version"]).stdout.decode().strip()
    rows = []
    with tempfile.TemporaryDirectory() as work, tempfile.TemporaryDirectory() as profile:
        for f in files:
            row = measure(f, Path(work), profile)
            rows.append(row)
            print(f"  {f.name:<12} words {row.get('word_recall')}  numbers "
                  f"{row.get('number_recall')}  pages {row.get('pages')}", file=sys.stderr, flush=True)
    result = {"engine_build": build, "summary": summarise(rows), "files": rows}
    if args.out != "-":
        Path(args.out).write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(result["summary"], indent=2))
    else:
        print(json.dumps(result["files"], indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
