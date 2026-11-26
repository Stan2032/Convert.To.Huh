#!/usr/bin/env python3
"""Run the real-world tier through `huh` and summarise what happened.

    python3 tools/fetch_govdocs.py     # once
    python3 tools/run_govdocs.py       # writes corpus/govdocs1/results.json

Synthetic specimens show that a loss exists. This shows how it behaves on files
people actually have: how often a declared loss is exercised at all, how often
the measurement contradicts the registry, and how often a route simply fails.

  raster (JPEG, GIF, PNG)  `huh convert` to PNG (PNG to JPEG), using the
                           receipt's declared-vs-measured rows and SSIM
  PDF                      `huh clean`, then pdftotext/pdfinfo before and after:
                           did the file have text to lose, and did the page
                           count survive?

Each file is identified by content (`huh detect`), never by its name.
Requires a release build (cargo build --release) and poppler-utils.
"""
import json
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FILES = ROOT / "corpus" / "govdocs1" / "files"
MANIFEST = ROOT / "corpus" / "govdocs1" / "MANIFEST.tsv"
OUT = ROOT / "corpus" / "govdocs1" / "results.json"
HUH = ROOT / "core" / "target" / "release" / "huh"
TIMEOUT = 300


def run(cmd: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=TIMEOUT, cwd=ROOT)


def text_chars(pdf: Path) -> int | None:
    r = run(["pdftotext", "-q", str(pdf), "-"])
    return len("".join(r.stdout.split())) if r.returncode == 0 else None


def pages(pdf: Path) -> int | None:
    r = run(["pdfinfo", str(pdf)])
    for line in r.stdout.splitlines():
        if line.startswith("Pages:"):
            return int(line.split()[1])
    return None


def raster(path: Path, fmt: str, tmp: Path) -> dict:
    target = "jpg" if fmt == "image/png" else "png"
    out, receipt = tmp / f"out.{target}", tmp / "receipt.json"
    r = run([str(HUH), "convert", str(path), str(out), "--receipt", str(receipt)])
    row = {"exit": r.returncode, "target": target}
    if receipt.is_file():
        rc = json.loads(receipt.read_text())
        row["route"] = rc["hops"]
        row["ssim"] = rc.get("ssim")
        row["measurements"] = {m["property"]: m["measured"] for m in rc["measurements"]}
        row["contradictions"] = [m["property"] for m in rc["measurements"] if m["contradicts"]]
        receipt.unlink()
    else:
        row["error"] = (r.stderr.strip().splitlines() or [""])[-1]
    return row


def pdf(path: Path, tmp: Path) -> dict:
    out, report = tmp / "clean.pdf", tmp / "report.json"
    r = run([str(HUH), "clean", str(path), str(out), "--report", str(report)])
    row = {"exit": r.returncode,
           "text_chars_before": text_chars(path), "pages_before": pages(path)}
    if r.returncode == 0 and out.is_file():
        row["text_chars_after"] = text_chars(out)
        row["pages_after"] = pages(out)
        row["bytes_ratio"] = round(out.stat().st_size / path.stat().st_size, 3)
    else:
        row["error"] = (r.stderr.strip().splitlines() or [""])[-1]
    for p in (out, report):
        p.unlink(missing_ok=True)
    return row


def summarise(rows: list[dict]) -> dict:
    s: dict = {}
    by = {}
    for r in rows:
        by.setdefault(r["format"], []).append(r)
    for fmt, rs in sorted(by.items()):
        ok = [r for r in rs if r["exit"] == 0]
        entry = {"files": len(rs), "succeeded": len(ok),
                 "failed": {r["member"]: r.get("error", f"exit {r['exit']}")
                            for r in rs if r["exit"] not in (0, 4)}}
        if fmt == "application/pdf":
            had_text = [r for r in rs if (r["text_chars_before"] or 0) > 0]
            entry["had_extractable_text"] = len(had_text)
            entry["text_survived_clean"] = sum(1 for r in ok if (r.get("text_chars_after") or 0) > 0)
            entry["page_count_changed"] = [r["member"] for r in ok
                                           if r.get("pages_after") != r["pages_before"]]
            if ok:
                entry["median_bytes_ratio"] = statistics.median(r["bytes_ratio"] for r in ok)
        else:
            entry["contradicted_registry"] = {r["member"]: r["contradictions"]
                                              for r in rs if r.get("contradictions")}
            ssims = [r["ssim"] for r in rs if r.get("ssim") is not None]
            if ssims:
                entry["ssim_min"] = round(min(ssims), 4)
                entry["ssim_median"] = round(statistics.median(ssims), 4)
            props = {}
            for r in rs:
                for k, v in r.get("measurements", {}).items():
                    props.setdefault(k, {}).setdefault(v, 0)
                    props[k][v] += 1
            entry["measured"] = props
        s[fmt] = entry
    return s


def main() -> int:
    if not HUH.is_file():
        sys.exit("build first: cargo build --release --manifest-path core/Cargo.toml")
    members = [line.split("\t")[0] for line in MANIFEST.read_text().splitlines()
               if line and not line.startswith("#")]
    rows = []
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        for m in members:
            path = FILES / Path(m).name
            if not path.is_file():
                sys.exit(f"missing {path}: run tools/fetch_govdocs.py")
            fmt = json.loads(run([str(HUH), "detect", str(path)]).stdout)["format"]
            row = {"member": m, "format": fmt}
            if fmt == "application/pdf":
                row |= pdf(path, tmp)
            elif fmt.startswith("image/"):
                row |= raster(path, fmt, tmp)
            else:
                row |= {"exit": None, "error": f"no route exercised for {fmt}"}
            rows.append(row)
            print(f"  {m:<20} {fmt:<18} exit={row['exit']}", flush=True)
    result = {"source": "Govdocs1 thread0, members listed in MANIFEST.tsv",
              "summary": summarise(rows), "files": rows}
    OUT.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["summary"], indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
