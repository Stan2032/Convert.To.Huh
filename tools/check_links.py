#!/usr/bin/env python3
"""Check that internal documentation cross-references resolve.

This project leans hard on cross-references: ADRs are binding constraints,
specs cite the research dossier by section, and agent prompts point at both.
A dangling reference silently turns a binding rule into a rumour.

Checks:
  1. Relative markdown links point at files that exist.
  2. Bare ADR references (ADR-0007) name an ADR that exists.
  3. Research citations (R§4.1, docs/RESEARCH.md §4.1) name a section
     that exists in the dossier.
  4. Referenced spec paths exist.

Dependency-free.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parent.parent

MD_LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
ADR_REF = re.compile(r"\bADR-(\d{4})\b")
# R§4.1  |  §4.1 (when near a RESEARCH mention)  |  docs/RESEARCH.md §4.1
RESEARCH_REF = re.compile(r"(?:R|RESEARCH\.md|docs/RESEARCH\.md)\s*§\s*(\d+(?:\.\d+)?)")
SPEC_PATH = re.compile(r"`(docs/specs/[a-z0-9-]+\.md)`")
BARE_PATH = re.compile(r"`((?:docs|registry|tools|\.claude)/[A-Za-z0-9_./-]+)`")


def md_files() -> list[Path]:
    out = []
    for p in ROOT.rglob("*.md"):
        if any(part in {".git", "node_modules", "target"} for part in p.parts):
            continue
        out.append(p)
    return sorted(out)


def research_sections() -> set[str]:
    """Top-level and subsection numbers present in the dossier."""
    doc = (ROOT / "docs" / "RESEARCH.md")
    if not doc.is_file():
        return set()
    found: set[str] = set()
    for line in doc.read_text(encoding="utf-8").splitlines():
        m = re.match(r"^#{2,3}\s+(\d+(?:\.\d+)?)[.\s]", line)
        if m:
            found.add(m.group(1))
            found.add(m.group(1).split(".")[0])
    return found


def adr_numbers() -> set[str]:
    return {p.name[:4] for p in (ROOT / "docs" / "adr").glob("[0-9][0-9][0-9][0-9]-*.md")}


def main() -> int:
    errors: list[str] = []
    sections = research_sections()
    adrs = adr_numbers()

    if not sections:
        errors.append("docs/RESEARCH.md: no numbered sections found")
    if not adrs:
        errors.append("docs/adr/: no ADRs found")

    for p in md_files():
        rel = p.relative_to(ROOT)
        text = p.read_text(encoding="utf-8")

        for target in MD_LINK.findall(text):
            target = target.strip()
            if target.startswith("#"):
                continue
            parsed = urlparse(target)
            if parsed.scheme in {"http", "https", "mailto"}:
                continue
            path_part = target.split("#", 1)[0]
            if not path_part:
                continue
            resolved = (p.parent / path_part).resolve()
            if not resolved.exists():
                errors.append(f"{rel}: broken link -> {target}")

        for num in ADR_REF.findall(text):
            if num not in adrs:
                errors.append(f"{rel}: references ADR-{num}, which does not exist")

        for sec in RESEARCH_REF.findall(text):
            if sec not in sections:
                errors.append(f"{rel}: cites RESEARCH §{sec}, which has no such section")

        for spec in set(SPEC_PATH.findall(text)):
            if not (ROOT / spec).is_file():
                errors.append(f"{rel}: references missing spec `{spec}`")

        for bare in set(BARE_PATH.findall(text)):
            candidate = ROOT / bare
            if bare.endswith("/"):
                if not candidate.is_dir():
                    errors.append(f"{rel}: references missing directory `{bare}`")
            elif "*" in bare:
                if not list(ROOT.glob(bare)):
                    errors.append(f"{rel}: glob `{bare}` matches nothing")
            elif not candidate.exists():
                errors.append(f"{rel}: references missing path `{bare}`")

    print(f"checked {len(md_files())} markdown files, "
          f"{len(adrs)} ADRs, {len(sections)} research sections")
    if errors:
        print(f"\n{len(errors)} error(s):")
        for e in errors:
            print(f"  x {e}")
        return 1
    print("\nOK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
