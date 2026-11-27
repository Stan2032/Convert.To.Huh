#!/usr/bin/env python3
"""Write each edge's [edge.verification.real_world] block from real-file results.

    python3 tools/real_world_blocks.py          # show what would change
    python3 tools/real_world_blocks.py --write  # rewrite the registry TOML

Reads the committed outputs of the real-world runs and turns them into counts
(docs/specs/registry-schema.md, rule 14), so the numbers in the registry are
generated from the results files rather than copied by hand:

  corpus/govdocs1/edges.json   core/examples/real_edges.rs (raster edges)
  corpus/govdocs1/office.json  tools/measure_office.py (office edges)

A property is counted in `affected` only when the observation can detect
exactly the outcome the edge declares. Otherwise a zero would claim the loss
never happened when the instrument simply cannot see it:

  property     counted when declared   observed as
  alpha        lost                    input had transparency, output has none
  color_depth  degraded                bit depth went down
  geometry     degraded or lost        dimensions changed
  fonts        degraded                no requested font embedded   (office)
  precision    degraded                a stored number not printed exactly (.xls)

  metadata     lost                     input had metadata; none of it in the output
  metadata     degraded                 input had metadata; some of it missing
  color_depth  lost (to GIF)            output has fewer distinct colours than input
  alpha        degraded (to GIF)        partial transparency came out on/off only

PNG to GIF declares `alpha = degraded` (one-bit transparency) and
`color_depth = lost` (palette reduction). The receipt's own checks see
neither; the first version of this tool published zero for both, which would
have said the loss never happened. The two GIF rows above come from the
harness's `observed` counts, which compare input and output directly.

`contradicted` is written only where an automated declared-vs-measured check
ran (raster). The office runs have none, so it is left out rather than set to
zero.
"""
import argparse
import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EDGES = ROOT / "registry" / "edges"
RASTER = "corpus/govdocs1/edges.json"
OFFICE = "corpus/govdocs1/office.json"
OFFICE_TYPE = {"application/msword": "doc", "application/vnd.ms-excel": "xls",
               "application/vnd.ms-powerpoint": "ppt"}
# property -> ((declared values the observation can detect, measured value meaning it occurred))
RASTER_OCCURRED = {"alpha": ({"lost"}, "lost"),
                   "color_depth": ({"degraded"}, "degraded"),
                   "geometry": ({"degraded", "lost"}, "changed")}


def declared(loss: dict, prop: str) -> str | None:
    v = loss.get(prop)
    return v.get("value") if isinstance(v, dict) else v


def raster_blocks() -> dict[tuple, dict]:
    out = {}
    for row in json.loads((ROOT / RASTER).read_text()):
        if not row["ran"] and not row["failed"]:
            continue  # no real file of this source format
        key = (row["engine"], row["invocation"], row["from"], row["to"])
        contradicted = {c["specimen"] for c in row["contradictions"]}
        affected = {}
        for prop, outcomes in row["measured"].items():
            if prop in RASTER_OCCURRED:
                affected[prop] = (RASTER_OCCURRED[prop], outcomes)
        obs = row.get("observed", {})
        if "metadata_present" in obs:
            affected["metadata"] = (({"lost"}, "occurred"), {"occurred": obs["metadata_lost"]})
            affected["metadata_degraded"] = (({"degraded"}, "occurred"),
                                             {"occurred": obs["metadata_some_missing"]})
        if row["to"] == "image/gif":
            obs = row.get("observed", {})
            affected["color_depth"] = (({"lost"}, "occurred"),
                                       {"occurred": obs.get("palette_reduced", 0)})
            affected["alpha"] = (({"degraded"}, "occurred"),
                                 {"occurred": obs.get("partial_alpha_binarised", 0)})
        # Files whose metadata reached the output contradict `metadata = lost`;
        # resolved per edge in rewrite(), where the declared value is known.
        survived = set(row.get("observed", {}).get("metadata_survived_in", []))
        out[key] = {"corpus": "govdocs1", "files": row["ran"] + sum(row["failed"].values()),
                    "ran": row["ran"], "contradicted": contradicted,
                    "size_ratio": row.get("size_ratio"),
                    "metadata_survived_in": survived,
                    "affected": affected, "results": RASTER}
    return out


def office_block(src_format: str, loss: dict) -> dict | None:
    data = json.loads((ROOT / OFFICE).read_text())
    rows = [r for r in data["files"] if r["type"] == OFFICE_TYPE[src_format]]
    if not rows:
        return None
    ok = [r for r in rows if "error" not in r]
    affected = {}
    if declared(loss, "fonts") not in (None, "preserved"):
        affected["fonts"] = sum(1 for r in ok if r["fonts_requested"]
                                and not r["requested_font_survived"])
    if declared(loss, "precision") not in (None, "preserved"):
        affected["precision"] = sum(1 for r in ok if r.get("number_recall") is not None
                                    and r["number_recall"] < 1)
    sizes = sorted(r["size_ratio"] for r in ok if "size_ratio" in r)
    size = ({"median": sizes[len(sizes) // 2], "min": sizes[0], "max": sizes[-1]}
            if sizes else None)
    return {"corpus": "govdocs1", "files": len(rows), "ran": len(ok),
            "affected": affected, "results": OFFICE, "size_ratio": size}


def render(rw: dict) -> str:
    lines = ["[edge.verification.real_world]"]
    for k in ("corpus", "files", "ran", "contradicted"):
        if k in rw:
            v = rw[k]
            lines.append(f'{k:<12} = {json.dumps(v)}')
    aff = ", ".join(f"{p} = {n}" for p, n in sorted(rw["affected"].items()))
    lines.append(f"{'affected':<12} = {{ {aff} }}")
    lines.append(f'{"results":<12} = {json.dumps(rw["results"])}')
    if rw.get("size_ratio"):
        sr = rw["size_ratio"]
        lines.append(f"{'size_ratio':<12} = {{ median = {sr['median']}, min = {sr['min']}, max = {sr['max']} }}")
    return "\n".join(lines) + "\n"


BLOCK_RE = re.compile(r"\n?\[edge\.verification\.real_world\]\n(?:(?!\[).*\n)*", re.M)


def rewrite(path: Path, raster: dict) -> tuple[str, int]:
    text = path.read_text()
    parts = re.split(r"(?m)^(?=\[\[edge\]\])", text)
    changed = 0
    for i, part in enumerate(parts):
        if not part.startswith("[[edge]]"):
            continue
        edge = tomllib.loads(part)["edge"][0]
        if (edge.get("verification") or {}).get("status") != "verified":
            continue
        key = (edge["engine"], edge.get("invocation"), edge["from"], edge["to"])
        rw = raster.get(key)
        if rw is None and edge["from"] in OFFICE_TYPE:
            rw = office_block(edge["from"], edge.get("loss") or {})
        if rw is None:
            continue
        # Only count losses the edge declares (rule 14), and only where the
        # observation can detect the declared outcome.
        loss = edge.get("loss") or {}
        counted = {}
        for p, v in rw["affected"].items():
            if isinstance(v, tuple):  # raster: ((detectable declared values, occurred), outcomes)
                (detectable, occurred), outcomes = v
                prop = p.removesuffix("_degraded")
                if declared(loss, prop) in detectable:
                    counted[prop] = outcomes.get(occurred, 0)
            elif declared(loss, p) not in (None, "preserved"):
                counted[p] = v
        rw = rw | {"affected": counted}
        if isinstance(rw.get("contradicted"), set):
            files = set(rw["contradicted"])
            if declared(loss, "metadata") == "lost":
                files |= rw.get("metadata_survived_in", set())
            rw["contradicted"] = len(files)
        rw.pop("metadata_survived_in", None)
        body = BLOCK_RE.sub("\n", part).rstrip("\n")
        new = body + "\n\n" + render(rw) + ("\n\n" if i < len(parts) - 1 else "")
        if new != part:
            parts[i] = new
            changed += 1
    return "".join(parts), changed


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--write", action="store_true")
    args = ap.parse_args()
    raster = raster_blocks()
    total = 0
    for path in sorted(EDGES.glob("*.toml")):
        new, n = rewrite(path, raster)
        tomllib.loads(new)  # never write a file that no longer parses
        if n:
            total += n
            print(f"  {path.relative_to(ROOT)}: {n} edge(s)")
            if args.write:
                path.write_text(new)
    print(f"{total} edge(s) {'updated' if args.write else 'would change'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
