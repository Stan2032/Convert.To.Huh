#!/usr/bin/env python3
"""Validate registry/ against docs/specs/registry-schema.md.

Dependency-free (stdlib tomllib, Python 3.11+). Run from anywhere:

    python3 tools/validate_registry.py
    python3 tools/validate_registry.py --self-test   # check the checker

The rules encoded here are the enforceable half of the specs. If you change a
rule, change the spec in the same commit — a rule that lives only in code is a
rule nobody can find.
"""
from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "registry"

# ---------------------------------------------------------------------------
# Closed vocabularies. These mirror the specs; keep them in sync deliberately.
# ---------------------------------------------------------------------------

# docs/specs/content-model.md §2
KINDS = {
    "Bytes", "Text", "Table", "DocTree", "PageLayout", "Raster",
    "Vector", "AudioPCM", "VideoStream", "Mesh3D", "Container",
}

# docs/specs/content-model.md §3
OPS = {"Transcode", "Transform", "Project", "Extract", "Assemble", "Synthesize"}

# Ops that must NOT change content kind.
SAME_KIND_OPS = {"Transcode", "Transform"}

# docs/specs/loss-vector.md §2 — closed and versioned on purpose.
PROPERTIES = {
    "geometry", "color_depth", "color_profile", "alpha", "time", "layers",
    "text_as_text", "structure", "metadata", "fonts", "interactivity",
    "signal_fidelity", "audio_channels", "sample_rate", "bit_depth", "precision", "topology",
    "materials", "rigging", "provenance",
}

# docs/specs/loss-vector.md §3
LOSS_VALUES = {"preserved", "degraded", "lost", "synthesized"}

SIGNATURE_WEIGHTS = {"decisive", "strong", "weak"}
HOSTILITY = {"trusted", "moderate", "high", "extreme"}
ISOLATION = {"wasm", "gvisor", "microvm"}
ISOLATION_RANK = {"wasm": 0, "gvisor": 1, "microvm": 2}
MEMORY_MODELS = {"streaming", "whole-file", "unbounded"}
CONCURRENCY = {"thread-safe", "process-per-job", "singleton"}
TIME_MODELS = {
    "constant", "linear_bytes", "linear_pixels", "linear_duration", "superlinear",
}
VERIFICATION_STATUS = {"claimed", "verified", "refuted"}
BUILD_PROFILES = {"lgpl", "gpl"}

HEX_RE = re.compile(r"^[0-9A-F]+$")


class Findings:
    def __init__(self) -> None:
        self.errors: list[str] = []
        self.warnings: list[str] = []

    def error(self, where: str, msg: str) -> None:
        self.errors.append(f"{where}: {msg}")

    def warn(self, where: str, msg: str) -> None:
        self.warnings.append(f"{where}: {msg}")


# ---------------------------------------------------------------------------
# Loading
# ---------------------------------------------------------------------------

def load_dir(path: Path, f: Findings) -> dict[str, dict]:
    out: dict[str, dict] = {}
    if not path.is_dir():
        f.error(str(path), "directory missing")
        return out
    for p in sorted(path.glob("*.toml")):
        try:
            out[p.name] = tomllib.loads(p.read_text(encoding="utf-8"))
        except tomllib.TOMLDecodeError as e:
            f.error(p.name, f"TOML parse error: {e}")
    return out


# ---------------------------------------------------------------------------
# Formats
# ---------------------------------------------------------------------------

def check_formats(formats: dict[str, dict], f: Findings) -> dict[str, dict]:
    by_id: dict[str, dict] = {}
    for fname, doc in formats.items():
        for field in ("id", "name", "kinds", "extensions", "mime_types", "preserves"):
            if field not in doc:
                f.error(fname, f"missing required field `{field}`")
        fid = doc.get("id")
        if not fid:
            continue
        if fid in by_id:
            f.error(fname, f"duplicate format id `{fid}`")
        by_id[fid] = doc

        for k in doc.get("kinds", []):
            if k not in KINDS:
                f.error(fname, f"unknown content kind `{k}`")
        if not doc.get("kinds"):
            f.error(fname, "must declare at least one content kind")

        for prop in doc.get("preserves", []):
            if prop not in PROPERTIES:
                f.error(fname, f"unknown preservable property `{prop}`")

        sigs = doc.get("signatures", [])
        for i, sig in enumerate(sigs):
            magic = sig.get("magic", "")
            if not HEX_RE.match(magic):
                f.error(fname, f"signature[{i}] magic must be uppercase hex, got `{magic}`")
            if len(magic) % 2:
                f.error(fname, f"signature[{i}] magic has odd length (not whole bytes)")
            if sig.get("weight") not in SIGNATURE_WEIGHTS:
                f.error(fname, f"signature[{i}] weight must be one of {sorted(SIGNATURE_WEIGHTS)}")
            if not isinstance(sig.get("offset"), int) or sig.get("offset", -1) < 0:
                f.error(fname, f"signature[{i}] needs a non-negative integer offset")

        # registry-schema.md: undetectable formats must say so in notes.
        notes = doc.get("notes", {}).get("text", "")
        if not sigs and not doc.get("detection", {}).get("container_of"):
            if "UNDETECTABLE" not in notes.upper():
                f.error(
                    fname,
                    "no signatures and no container_of: must document this in "
                    "[notes] with the word UNDETECTABLE",
                )

        if "puid" in doc and "puid_verified" not in doc:
            f.warn(fname, "has a puid but no puid_verified flag")
        if doc.get("puid_verified") is False:
            f.warn(fname, f"puid `{doc.get('puid')}` is unverified against PRONOM "
                          f"(run: python3 tools/verify_pronom.py {doc.get('id','')})")
        if doc.get("puid_verified") is True and not doc.get("puid_name"):
            f.error(fname, "puid_verified is true but puid_name is missing — record "
                           "the exact PRONOM format name so drift is detectable")
        if "puid" not in doc and "puid" not in notes.lower():
            f.warn(fname, "no PUID and no justification in notes")

    check_priority_dag(by_id, f)
    return by_id


def check_priority_dag(by_id: dict[str, dict], f: Findings) -> None:
    """detection.priority_over must form a DAG (registry-schema.md)."""
    graph = {
        fid: [t for t in doc.get("detection", {}).get("priority_over", [])]
        for fid, doc in by_id.items()
    }
    for fid, targets in graph.items():
        for t in targets:
            if t not in by_id:
                f.error(f"format {fid}", f"priority_over references unknown format `{t}`")

    WHITE, GREY, BLACK = 0, 1, 2
    color = {fid: WHITE for fid in graph}

    def visit(node: str, trail: list[str]) -> None:
        color[node] = GREY
        for nxt in graph.get(node, []):
            if nxt not in color:
                continue
            if color[nxt] == GREY:
                cycle = " -> ".join(trail + [node, nxt])
                f.error("detection priority", f"cycle in priority_over: {cycle}")
            elif color[nxt] == WHITE:
                visit(nxt, trail + [node])
        color[node] = BLACK

    for fid in graph:
        if color[fid] == WHITE:
            visit(fid, [])


# ---------------------------------------------------------------------------
# Engines
# ---------------------------------------------------------------------------

def check_engines(engines: dict[str, dict], f: Findings) -> dict[str, dict]:
    by_id: dict[str, dict] = {}
    for fname, doc in engines.items():
        for field in ("id", "name", "version_pin"):
            if field not in doc:
                f.error(fname, f"missing required field `{field}`")
        eid = doc.get("id")
        if not eid:
            continue
        if eid in by_id:
            f.error(fname, f"duplicate engine id `{eid}`")
        by_id[eid] = doc

        lic = doc.get("license", {})
        if "spdx" not in lic:
            f.error(fname, "[license] missing spdx")
        if "patent_exposure" not in lic:
            f.error(fname, "[license] missing patent_exposure (tracked separately from spdx, ADR-0007)")
        for prof in lic.get("build_profiles", []):
            if prof not in BUILD_PROFILES:
                f.error(fname, f"unknown build profile `{prof}`")
        # License determinations gate routing (ADR-0007), so they need provenance.
        if lic.get("verified") is True:
            if not lic.get("source"):
                f.error(fname, "[license] verified is true but no `source` URL — a license "
                               "claim that gates routing must be traceable")
            if not lic.get("checked"):
                f.error(fname, "[license] verified is true but no `checked` date")
        else:
            f.warn(fname, "[license] unverified — run down the actual license text "
                          "of the pinned version and set verified/source/checked")
        # A component license stricter than the headline one restricts profiles.
        components = lic.get("spdx_components", [])
        if any("GPL-3" in c or "AGPL" in c for c in components) and "lgpl" in lic.get("build_profiles", []):
            f.error(fname, f"[license] spdx_components {components} include a strong copyleft "
                           f"component, so the `lgpl` build profile cannot be claimed without "
                           f"a component-level audit (ADR-0007)")

        sec = doc.get("security", {})
        hostility = sec.get("hostility")
        if hostility not in HOSTILITY:
            f.error(fname, f"[security] hostility must be one of {sorted(HOSTILITY)}")
        isolation = sec.get("isolation")
        if isolation not in ISOLATION:
            f.error(fname, f"[security] isolation must be one of {sorted(ISOLATION)}")
        if sec.get("network") is not False:
            f.error(fname, "[security] network must be false — ADR-0003, no exceptions")

        # Hostility floor: extreme engines cannot run below microvm.
        if hostility == "extreme" and isolation and ISOLATION_RANK.get(isolation, 0) < ISOLATION_RANK["microvm"]:
            f.error(fname, "hostility `extreme` requires isolation `microvm` (ADR-0003)")
        if hostility == "high" and isolation and ISOLATION_RANK.get(isolation, 0) < ISOLATION_RANK["gvisor"]:
            f.error(fname, "hostility `high` requires isolation `gvisor` or stronger (ADR-0003)")

        res = doc.get("resources", {})
        if res.get("memory_model") not in MEMORY_MODELS:
            f.error(fname, f"[resources] memory_model must be one of {sorted(MEMORY_MODELS)}")
        if res.get("concurrency") not in CONCURRENCY:
            f.error(fname, f"[resources] concurrency must be one of {sorted(CONCURRENCY)}")
        if not isinstance(res.get("default_timeout_s"), int):
            f.error(fname, "[resources] default_timeout_s must be an integer")

        if "runtimes" not in doc:
            f.error(fname, "missing [runtimes] — the planner needs to know browser vs server availability")

        if not lic.get("build_profiles"):
            f.warn(fname, "no build profiles: the planner will prune this engine entirely")
    return by_id


# ---------------------------------------------------------------------------
# Edges
# ---------------------------------------------------------------------------

def loss_value_of(entry) -> str | None:
    if isinstance(entry, str):
        return entry
    if isinstance(entry, dict):
        return entry.get("value")
    return None


# Rule 7. What may replace a lost property (ADR-0012). Closed per property for
# the same reason the property list itself is closed: an open set makes two
# entries incomparable, and a typo becomes a silent new category.
LOSS_POLICIES = {
    "alpha": {"composite_black", "composite_white", "discard_channel", "premultiplied"},
    "audio_channels": {"downmix_average", "take_first", "downmix_weighted"},
}


def check_edges(edge_files: dict[str, dict], formats: dict[str, dict],
                engines: dict[str, dict], f: Findings) -> int:
    count = 0
    for fname, doc in edge_files.items():
        for i, edge in enumerate(doc.get("edge", [])):
            count += 1
            where = f"{fname}[edge {i}]"

            engine = edge.get("engine")
            if engine not in engines:
                f.error(where, f"unknown engine `{engine}`")

            op = edge.get("op")
            if op not in OPS:
                f.error(where, f"op must be one of {sorted(OPS)}, got `{op}`")

            src, dst = edge.get("from"), edge.get("to")
            for label, fid in (("from", src), ("to", dst)):
                if fid not in formats:
                    f.error(where, f"`{label}` references unknown format `{fid}`")

            from_kind, to_kind = edge.get("from_kind"), edge.get("to_kind")
            for label, k in (("from_kind", from_kind), ("to_kind", to_kind)):
                if k not in KINDS:
                    f.error(where, f"`{label}` unknown content kind `{k}`")

            # Rule 2: kind-changing ops cannot be Transcode/Transform.
            if from_kind and to_kind and from_kind != to_kind and op in SAME_KIND_OPS:
                f.error(where, f"op `{op}` cannot change content kind ({from_kind} -> {to_kind}) — content-model.md rule 3")

            # Kinds must actually be carryable by the declared formats.
            if src in formats and from_kind and from_kind not in formats[src].get("kinds", []):
                f.error(where, f"format `{src}` does not carry kind `{from_kind}`")
            if dst in formats and to_kind and to_kind not in formats[dst].get("kinds", []):
                f.error(where, f"format `{dst}` does not carry kind `{to_kind}`")

            loss = edge.get("loss", {})
            if not loss:
                f.error(where, "missing [edge.loss] — every edge declares its cost (ADR-0004)")

            has_synthesized = False
            for prop, entry in loss.items():
                if prop == "generational":
                    if not isinstance(entry, bool):
                        f.error(where, "`generational` must be a boolean")
                    continue
                if prop not in PROPERTIES:
                    f.error(where, f"unknown property `{prop}` — loss-vector.md §2 is a closed list")
                    continue
                val = loss_value_of(entry)
                if val not in LOSS_VALUES:
                    f.error(where, f"property `{prop}` has invalid value `{val}`")
                if val == "synthesized":
                    has_synthesized = True
                # Rule 4: cannot preserve what the target cannot hold.
                if val in ("preserved", "degraded") and dst in formats:
                    if prop not in formats[dst].get("preserves", []):
                        f.error(
                            where,
                            f"claims `{val}` for `{prop}` but target format `{dst}` "
                            f"cannot carry it — registry-schema.md rule 4",
                        )

            # Rule 3: Synthesize requires confidence and a synthesized property.
            if op == "Synthesize":
                if "confidence" not in edge:
                    f.error(where, "op `Synthesize` requires a `confidence` field (ADR-0009)")
                if not has_synthesized:
                    f.error(where, "op `Synthesize` must mark at least one property `synthesized` (ADR-0009)")
            elif has_synthesized:
                f.error(where, f"op `{op}` marks a property `synthesized`; only `Synthesize` may do that")

            cost = edge.get("cost", {})
            if cost.get("time_model") not in TIME_MODELS:
                f.error(where, f"[edge.cost] time_model must be one of {sorted(TIME_MODELS)}")
            if cost.get("memory_model") not in MEMORY_MODELS:
                f.error(where, f"[edge.cost] memory_model must be one of {sorted(MEMORY_MODELS)}")

            ver = edge.get("verification", {})
            status = ver.get("status")
            if status not in VERIFICATION_STATUS:
                f.error(where, f"[edge.verification] status must be one of {sorted(VERIFICATION_STATUS)}")
            # Rule 5: verified requires evidence.
            if status == "verified":
                if not ver.get("specimens"):
                    f.error(where, "status `verified` requires at least one specimen — registry-schema.md rule 5")
                if not ver.get("measured"):
                    f.error(where, "status `verified` requires a `measured` block — registry-schema.md rule 5")

            # Rule 7: policy qualifiers must be from the documented set.
            for prop, val in (edge.get("loss") or {}).items():
                if not isinstance(val, dict):
                    continue
                pol = val.get("policy")
                if pol is None:
                    continue
                allowed = LOSS_POLICIES.get(prop)
                if allowed is None:
                    f.error(where, f"`{prop}` has a `policy` but no policy vocabulary is "
                                   f"defined for it — registry-schema.md rule 7")
                elif pol not in allowed:
                    f.error(where, f"unknown policy `{pol}` for `{prop}`; expected one of "
                                   f"{sorted(allowed)} — registry-schema.md rule 7")

            # Rule 8: a verified edge for an engine that is invoked as a
            # subprocess must record HOW (ADR-0013). Warning rather than error:
            # an in-process library with fixed parameters legitimately has no
            # invocation, and the validator cannot tell the two apart.
            eng = engines.get(engine) or {}
            if (edge.get("verification") or {}).get("status") == "verified" \
                    and not edge.get("invocation") \
                    and (eng.get("security") or {}).get("isolation") not in (None, "wasm"):
                f.warn(where, "verified edge has no `invocation`; loss depends on how the "
                              "engine was called — registry-schema.md rule 8")
            if status == "claimed":
                f.warn(where, "unverified: the planner will ignore this edge")
    return count


# ---------------------------------------------------------------------------
# Self-test — check the checker
# ---------------------------------------------------------------------------

SELF_TESTS: list[tuple[str, str, str]] = [
    (
        "synthesize without confidence",
        """
        [[edge]]
        engine="e"
        op="Synthesize"
        from="a"
        to="b"
        from_kind="AudioPCM"
        to_kind="Text"
        [edge.loss]
        text_as_text="synthesized"
        [edge.cost]
        time_model="constant"
        memory_model="streaming"
        [edge.verification]
        status="claimed"
        """,
        "requires a `confidence` field",
    ),
    (
        "policy from outside the documented vocabulary",
        """
        [[edge]]
        engine="e"
        op="Transcode"
        from="a"
        to="b"
        from_kind="Raster"
        to_kind="Raster"
        [edge.loss]
        alpha={ value="lost", policy="composite_puce" }
        [edge.cost]
        time_model="constant"
        memory_model="streaming"
        [edge.verification]
        status="claimed"
        """,
        "unknown policy",
    ),
    (
        "policy on a property with no policy vocabulary",
        """
        [[edge]]
        engine="e"
        op="Transcode"
        from="a"
        to="b"
        from_kind="Raster"
        to_kind="Raster"
        [edge.loss]
        geometry={ value="lost", policy="crop" }
        [edge.cost]
        time_model="constant"
        memory_model="streaming"
        [edge.verification]
        status="claimed"
        """,
        "no policy vocabulary is defined",
    ),
    (
        "transcode changing kind",
        """
        [[edge]]
        engine="e"
        op="Transcode"
        from="a"
        to="b"
        from_kind="AudioPCM"
        to_kind="Text"
        [edge.loss]
        text_as_text="lost"
        [edge.cost]
        time_model="constant"
        memory_model="streaming"
        [edge.verification]
        status="claimed"
        """,
        "cannot change content kind",
    ),
    (
        "preserving what the target cannot hold",
        """
        [[edge]]
        engine="e"
        op="Transcode"
        from="a"
        to="b"
        from_kind="AudioPCM"
        to_kind="Text"
        [edge.loss]
        alpha="preserved"
        [edge.cost]
        time_model="constant"
        memory_model="streaming"
        [edge.verification]
        status="claimed"
        """,
        "registry-schema.md rule 4",
    ),
    (
        "verified without evidence",
        """
        [[edge]]
        engine="e"
        op="Extract"
        from="a"
        to="b"
        from_kind="AudioPCM"
        to_kind="Text"
        [edge.loss]
        time="lost"
        [edge.cost]
        time_model="constant"
        memory_model="streaming"
        [edge.verification]
        status="verified"
        """,
        "registry-schema.md rule 5",
    ),
    (
        "unknown loss property",
        """
        [[edge]]
        engine="e"
        op="Extract"
        from="a"
        to="b"
        from_kind="AudioPCM"
        to_kind="Text"
        [edge.loss]
        vibes="lost"
        [edge.cost]
        time_model="constant"
        memory_model="streaming"
        [edge.verification]
        status="claimed"
        """,
        "closed list",
    ),
    (
        "non-Synthesize marking synthesized",
        """
        [[edge]]
        engine="e"
        op="Extract"
        from="a"
        to="b"
        from_kind="AudioPCM"
        to_kind="Text"
        [edge.loss]
        text_as_text="synthesized"
        [edge.cost]
        time_model="constant"
        memory_model="streaming"
        [edge.verification]
        status="claimed"
        """,
        "only `Synthesize` may do that",
    ),
]

ENGINE_SELF_TESTS: list[tuple[str, str, str]] = [
    (
        "lgpl profile claimed despite GPL-3 component",
        """
        id="e"
        name="E"
        version_pin=">=1,<2"
        [license]
        spdx="MPL-2.0"
        spdx_components=["GPL-3.0-or-later"]
        build_profiles=["lgpl","gpl"]
        patent_exposure="none"
        verified=true
        checked="2026-01-01"
        source="https://example.invalid/COPYING"
        [security]
        hostility="moderate"
        isolation="gvisor"
        network=false
        [resources]
        memory_model="streaming"
        concurrency="thread-safe"
        default_timeout_s=60
        [runtimes]
        native=true
        """,
        "strong copyleft component",
    ),
    (
        "verified license without a source",
        """
        id="e"
        name="E"
        version_pin=">=1,<2"
        [license]
        spdx="MIT"
        build_profiles=["lgpl"]
        patent_exposure="none"
        verified=true
        [security]
        hostility="moderate"
        isolation="gvisor"
        network=false
        [resources]
        memory_model="streaming"
        concurrency="thread-safe"
        default_timeout_s=60
        [runtimes]
        native=true
        """,
        "must be traceable",
    ),
    (
        "extreme hostility below microvm",
        """
        id="e"
        name="E"
        version_pin=">=1,<2"
        [license]
        spdx="MIT"
        build_profiles=["lgpl"]
        patent_exposure="none"
        verified=true
        checked="2026-01-01"
        source="https://example.invalid/L"
        [security]
        hostility="extreme"
        isolation="gvisor"
        network=false
        [resources]
        memory_model="streaming"
        concurrency="thread-safe"
        default_timeout_s=60
        [runtimes]
        native=true
        """,
        "requires isolation `microvm`",
    ),
    (
        "network enabled",
        """
        id="e"
        name="E"
        version_pin=">=1,<2"
        [license]
        spdx="MIT"
        build_profiles=["lgpl"]
        patent_exposure="none"
        verified=true
        checked="2026-01-01"
        source="https://example.invalid/L"
        [security]
        hostility="moderate"
        isolation="gvisor"
        network=true
        [resources]
        memory_model="streaming"
        concurrency="thread-safe"
        default_timeout_s=60
        [runtimes]
        native=true
        """,
        "network must be false",
    ),
]

FAKE_FORMATS = {
    "a": {"id": "a", "kinds": ["AudioPCM"], "preserves": ["time", "sample_rate"]},
    "b": {"id": "b", "kinds": ["Text"], "preserves": ["text_as_text"]},
}
FAKE_ENGINES = {"e": {"id": "e"}}


def self_test() -> int:
    failures = 0
    print("self-test: each fixture must be REJECTED for the stated reason\n")
    for name, toml_src, expect in SELF_TESTS:
        f = Findings()
        doc = tomllib.loads(toml_src)
        check_edges({"fixture.toml": doc}, FAKE_FORMATS, FAKE_ENGINES, f)
        hit = any(expect in e for e in f.errors)
        print(f"  [{'PASS' if hit else 'FAIL'}] {name}")
        if not hit:
            failures += 1
            print(f"         expected an error containing: {expect!r}")
            for e in f.errors:
                print(f"         got: {e}")

    # A valid edge must produce no errors.
    f = Findings()
    doc = tomllib.loads(
        """
        [[edge]]
        engine="e"
        op="Extract"
        from="a"
        to="b"
        from_kind="AudioPCM"
        to_kind="Text"
        [edge.loss]
        time="lost"
        [edge.cost]
        time_model="constant"
        memory_model="streaming"
        [edge.verification]
        status="claimed"
        """
    )
    check_edges({"fixture.toml": doc}, FAKE_FORMATS, FAKE_ENGINES, f)
    ok = not f.errors
    print(f"  [{'PASS' if ok else 'FAIL'}] valid edge accepted")
    if not ok:
        failures += 1
        for e in f.errors:
            print(f"         unexpected: {e}")

    for name, toml_src, expect in ENGINE_SELF_TESTS:
        f = Findings()
        check_engines({"engine.toml": tomllib.loads(toml_src)}, f)
        hit = any(expect in e for e in f.errors)
        print(f"  [{'PASS' if hit else 'FAIL'}] {name}")
        if not hit:
            failures += 1
            print(f"         expected an error containing: {expect!r}")
            for e in f.errors:
                print(f"         got: {e}")

    total = len(SELF_TESTS) + len(ENGINE_SELF_TESTS) + 1
    print(f"\nself-test: {total - failures}/{total} passed")
    return 1 if failures else 0


# ---------------------------------------------------------------------------

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--self-test", action="store_true", help="validate the validator against known-bad fixtures")
    ap.add_argument("--strict", action="store_true", help="treat warnings as errors")
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    f = Findings()

    version_file = REGISTRY / "VERSION"
    if not version_file.is_file():
        f.error("registry/VERSION", "missing — receipts pin this (registry-schema.md)")
    else:
        print(f"registry version: {version_file.read_text().strip()}")

    formats = check_formats(load_dir(REGISTRY / "formats", f), f)
    engines = check_engines(load_dir(REGISTRY / "engines", f), f)
    n_edges = check_edges(load_dir(REGISTRY / "edges", f), formats, engines, f)

    print(f"checked {len(formats)} formats, {len(engines)} engines, {n_edges} edges")

    if f.warnings:
        print(f"\n{len(f.warnings)} warning(s):")
        for w in f.warnings:
            print(f"  ! {w}")
    if f.errors:
        print(f"\n{len(f.errors)} error(s):")
        for e in f.errors:
            print(f"  x {e}")
        return 1

    if args.strict and f.warnings:
        print("\n--strict: warnings are errors")
        return 1

    print("\nOK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
