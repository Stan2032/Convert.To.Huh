#!/usr/bin/env python3
"""Verify registry PUIDs and signatures against PRONOM.

PRONOM is the authoritative technical registry of file formats (UK National
Archives). Every format entry that carries a `puid` should be checkable against
it, so that our identifiers are joinable with the digital-preservation world
rather than merely plausible.

    python3 tools/verify_pronom.py                # check every format with a puid
    python3 tools/verify_pronom.py image/png      # check one
    python3 tools/verify_pronom.py --offline      # no network; report status only

Network-dependent by design, so this is NOT part of the default CI gate — it is
run deliberately by `format-registry-curator` when adding or auditing entries.
Exits non-zero if a declared PUID does not resolve or does not match.
"""
from __future__ import annotations

import argparse
import sys
import tomllib
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FORMATS = ROOT / "registry" / "formats"
NS = {"p": "http://pronom.nationalarchives.gov.uk"}
BASE = "https://www.nationalarchives.gov.uk/PRONOM/{puid}.xml"
UA = "convert-to-huh-registry-check (+https://github.com/Stan2032/Convert.To.Huh)"


def fetch(puid: str, timeout: int = 30) -> str:
    req = urllib.request.Request(BASE.format(puid=puid), headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read().decode("utf-8", "replace")


def _t(el, tag: str) -> str:
    n = el.find(f"p:{tag}", NS)
    return (n.text or "").strip() if n is not None and n.text is not None else ""


def parse(xml: str) -> dict | None:
    ff = ET.fromstring(xml).find(".//p:FileFormat", NS)
    if ff is None:
        return None
    ids = ff.findall("p:FileFormatIdentifier", NS)
    sigs = []
    for isig in ff.findall("p:InternalSignature", NS):
        for bs in isig.findall("p:ByteSequence", NS):
            sigs.append({
                "position": _t(bs, "PositionType"),
                "offset": _t(bs, "Offset") or "0",
                "value": _t(bs, "ByteSequenceValue"),
                "note": _t(isig, "SignatureNote"),
            })
    return {
        "name": _t(ff, "FormatName"),
        "version": _t(ff, "FormatVersion"),
        "puids": [_t(i, "Identifier") for i in ids if _t(i, "IdentifierType") == "PUID"],
        "mimes": [_t(i, "Identifier") for i in ids if _t(i, "IdentifierType") == "MIME"],
        "signatures": sigs,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("format_id", nargs="?", help="only check this format id")
    ap.add_argument("--offline", action="store_true", help="report declared status without contacting PRONOM")
    args = ap.parse_args()

    errors: list[str] = []
    notes: list[str] = []
    checked = skipped = 0

    for f in sorted(FORMATS.glob("*.toml")):
        d = tomllib.loads(f.read_text(encoding="utf-8"))
        fid = d.get("id", f.stem)
        if args.format_id and fid != args.format_id:
            continue
        puid = d.get("puid")
        if not puid:
            notes.append(f"{f.name}: no PUID declared")
            skipped += 1
            continue
        if args.offline:
            state = "verified" if d.get("puid_verified") else "UNVERIFIED"
            print(f"  {fid:<32} {puid:<12} {state}")
            skipped += 1
            continue

        try:
            info = parse(fetch(puid))
        except (urllib.error.URLError, TimeoutError) as e:
            errors.append(f"{f.name}: could not reach PRONOM for {puid}: {e}")
            continue
        except ET.ParseError as e:
            errors.append(f"{f.name}: malformed PRONOM response for {puid}: {e}")
            continue

        checked += 1
        if info is None:
            errors.append(f"{f.name}: PUID {puid} does not resolve to a format — wrong identifier")
            continue
        if puid not in info["puids"]:
            errors.append(f"{f.name}: declared {puid} but PRONOM reports {info['puids']}")
            continue

        label = f"{info['name']} {info['version']}".strip()
        print(f"  {fid:<32} {puid:<12} OK  {label}")

        declared_name = d.get("puid_name")
        if declared_name and declared_name != label:
            errors.append(f"{f.name}: puid_name is {declared_name!r} but PRONOM says {label!r}")
        if not d.get("puid_verified"):
            notes.append(f"{f.name}: matches PRONOM but puid_verified is still false — set it")

        ours = {m.lower() for m in d.get("mime_types", [])}
        theirs = {m.lower() for m in info["mimes"] if m}
        if theirs and not (theirs & ours):
            errors.append(f"{f.name}: MIME mismatch — ours {sorted(ours)} vs PRONOM {sorted(theirs)}")

        # PRONOM signatures use wildcards, ranges, alternation and EOF anchors
        # that our simple `magic` prefix cannot express. Surface, don't fail.
        for s in info["signatures"]:
            if any(c in s["value"] for c in "*{("):
                notes.append(
                    f"{f.name}: PRONOM signature is richer than our schema can express "
                    f"({s['position']}, {s['value'][:44]}...)"
                )
                break

    print(f"\nchecked {checked} against PRONOM, skipped {skipped}")
    if notes:
        print(f"\n{len(notes)} note(s):")
        for n in notes:
            print(f"  ! {n}")
    if errors:
        print(f"\n{len(errors)} error(s):")
        for e in errors:
            print(f"  x {e}")
        return 1
    print("\nOK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
