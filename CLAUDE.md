# Convert.To.Huh — orientation

Read this first. Then `docs/ARCHITECTURE.md`. Then the spec covering whatever
you are about to touch.

## What this project is

**The deliverable is `dataset/conversion-loss-registry.json`** (ADR-0011) — a
CC0 dataset of what each file-format conversion destroys, measured against real
specimens. PRONOM says what a file *is*; nothing says what converting it
*costs*. Success is measured in verified edges, not features.

The CLI (`huh`) exists to produce and validate that data. It is built as a
**planner over a typed graph** of formats and conversion edges, and exposes:

- `huh convert` — runs a conversion and states its loss vector.
- `huh clean` — sanitization by canonicalizing rebuild (ADR-0010): the file is
  rebuilt from parsed content, and the loss vector reports what did not survive.
- `huh export` — emits the dataset from `registry/`.

**Prior art matters here.** Dangerzone and mat2 already do sanitization, better
isolated and across more formats. Do not rebuild them — see `RESEARCH.md §12.3b`.
The planner ignores unverified edges; an edge becomes `verified` when someone
runs it against a specimen and records a measurement.

## Ground rules

These hold everywhere. Violating one is a bug regardless of what else works.

1. **The extension is never authoritative.** Detection is content-first.
2. **Loss is a vector, never a scalar.** No `quality: 0.8` anywhere.
3. **Every capability claim is verified against a real specimen** before the
   planner trusts it.
4. **Synthesized content is always marked** — plan, UI, output metadata, receipt.
5. **Conversion sandboxes have no network.** (A deployment requirement; this
   repository isolates at process level only — see `docs/ARCHITECTURE.md`.)
6. **Legally unavailable routes do not exist**, rather than failing at runtime.
7. **Every plan is explainable in plain language**, generated from its loss vector.

## Where things live

| Path | Contents |
|---|---|
| `docs/RESEARCH.md` | Evidence base. Findings carry sources **and design implications**. Cited as `R§n`. |
| `docs/ARCHITECTURE.md` | System design and the invariants above. |
| `dataset/` | **The deliverable.** CC0 conversion-loss data, emitted by `huh export`. |
| `docs/adr/` | Binding decisions. Superseded by new ADRs, never edited quietly. `docs/adr/README.md` indexes them. |
| `docs/specs/` | Contracts, partly machine-enforced. |
| `registry/` | Formats, engines, edges. The core asset. |
| `tools/` | Validators. Dependency-free, run in CI. |
| `.claude/agents/` | Specialist agents. `.claude/agents/README.md` maps work to owner. |

## Before you commit

```bash
python3 tools/validate_registry.py     # must exit 0
python3 tools/validate_agents.py       # must exit 0
python3 tools/check_links.py           # must exit 0
```

`python3 tools/verify_pronom.py` checks format PUIDs against PRONOM's live API.
It is network-bound, so it is deliberately **not** in the CI gate — run it when
adding or auditing a format entry.

The registry validator enforces the parts of the specs that can be mechanized.
**If it rejects your change, the usual fix is not to weaken your data.** Rule 4
rejections in particular ("claims `preserved` for a property the target cannot
carry") almost always mean a format entry or the chosen target is wrong. Both
seed-data rejections in this repository were real bugs elsewhere —
see `registry/README.md`.

## Network access

Most reference sites are blocked by the egress proxy, and `curl` and `WebFetch`
have **separate** allowlists — a domain reachable by one may be blocked for the
other. `docs/RESEARCH.md §11` records what is reachable, the PDF-extraction
workaround, and the prioritized list of domains worth requesting. `WebSearch`
always works but returns summaries: fine for orientation, not sufficient for a
claim that lands in a registry entry.

## Conventions

- **ADRs are binding.** If your work requires violating one, stop and revisit
  the ADR first. Reversal means a new ADR that supersedes it and states what
  changed our mind.
- **Cite the dossier.** Design claims reference `docs/RESEARCH.md` sections.
  If the evidence is not there, add it there first — with a design implication,
  or it is trivia and does not belong.
- **Record what surprised you.** The `[notes]` fields across the registry exist
  because those facts cost real time to rediscover.
- **Specs and their enforcement ship in the same commit.** A rule that lives
  only in `tools/` is a rule nobody can find.

## Choosing an agent

`.claude/agents/README.md` has the full map. Quick version: registry data →
`format-registry-curator` (formats) or `capability-cartographer` (edges);
graph search and ranking → `route-planner-engineer`; what a cross-domain
conversion should *mean* → `semantic-bridge-designer`; anything security-shaped
→ `sandbox-warden` (who can veto); a specific broken conversion →
`conversion-forensics`.

## Things that look like bugs and are not

- **A budget refusal.** A zip bomb refusing to expand is the system working.
- **LibreOffice crashing.** `crash_expected = true` — documented behaviour.
- **Bad ASR output.** Inference is imperfect; that is why the class exists and
  why the output is marked. The bug would be failing to mark it.
- **A route ignoring an obvious direct edge.** That edge is probably `claimed`
  rather than `verified`.
