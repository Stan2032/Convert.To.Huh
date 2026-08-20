# Convert.To.Huh

> A universal file converter that treats "convert this AVI to a PDF" as a
> question worth answering properly, rather than a joke or an error.

**Status: design and scaffolding.** There is no working converter yet. What
exists is a researched architecture, five enforced specifications, a seeded and
machine-checked registry, and fifteen agent definitions covering the work.
See [ROADMAP.md](ROADMAP.md) for what happens next and
[`registry/README.md`](registry/README.md) for an honest account of what
currently converts (nothing, deliberately).

## The idea

The commercial converters are **lookup tables**: a map from (source format,
target format) to a command line. That works until you want breadth — a thousand
formats is a million pairs — and it has no answer at all for conversions that
cross domains.

The best open-source converter, [convert.to.it](https://convert.to.it/), already
went further and runs Dijkstra over a format graph. So pathfinding is not the
novel part here; it is the baseline. What is missing from every one of them is
any account of **what a conversion costs you** — see
[`docs/RESEARCH.md §1.6`](docs/RESEARCH.md) for a source-level comparison.

Convert.To.Huh is a **planner over a typed graph**. Formats and the content
kinds they carry are nodes. Engine capabilities are typed, weighted edges. A
conversion request is a pathfinding problem whose answer is a ranked set of
*plans*, each annotated with exactly what it will cost you.

Ask for `AVI → PDF` and you get four defensible answers, not a guess:

| Plan | What you get |
|---|---|
| **Contact sheet** | Frames sampled at 1 fps, tiled onto pages — the video at a glance |
| **Frame per page** | Every keyframe, full size |
| **Transcript** | The audio transcribed by a speech model and typeset. *Machine-generated — marked as such* |
| **Storyboard** | One frame per detected scene, with timecodes |

The system ranks them, explains each one in plain language, and asks. "Huh?" is
the correct reaction to the request. Resolving it is the product.

## Three ideas the design rests on

**Loss is a vector, not a scalar.** `quality: 0.8` cannot distinguish "kept
every pixel, destroyed all text selectability" from "kept the text, halved the
resolution." We track loss per property — transparency, semantic structure,
colour profile, text-as-text, and fifteen more — and compose it along the chain,
where `lost` is absorbing and inference is contaminating. Route explanations are
*generated* from that vector, so they cannot drift from the truth.

**Synthesis is never silent.** Some conversions require *inventing* information:
speech-to-text, OCR, image tracing. Those outputs are estimates about the input,
not re-expressions of it. They get their own operation class, their own loss
value, and mandatory marking in the plan, the interface, the file metadata and
the receipt. A user who receives a machine-written transcript that looks exactly
like extracted text has been misled.

**Every engine is hostile and unreliable.** Ghostscript has repeatedly escaped
the sandbox meant to contain it — one bug was exploited in the wild via EPS
files disguised as JPEGs. ImageMagick delegates to Ghostscript by default.
LibreOffice crashes on ordinary workloads and its own tooling declines to
restart it. Isolation tier is a declared property of each engine, conversion
sandboxes have no network at all, and refusing a decompression bomb is a normal
outcome rather than an error.

## Repository

| Path | What it is |
|---|---|
| [`docs/RESEARCH.md`](docs/RESEARCH.md) | The evidence base. Every design decision cites it. |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | The system design. |
| [`docs/adr/`](docs/adr/) | Nine binding decisions, each with falsification conditions. |
| [`docs/specs/`](docs/specs/) | Content model, loss vector, plan format, registry and engine schemas. |
| [`registry/`](registry/) | Formats, engines and capability edges — the project's core asset. |
| [`tools/`](tools/) | Dependency-free validators, run in CI. |
| [`.claude/agents/`](.claude/agents/) | Fifteen specialist agents covering the work. |

## Try the checks

```bash
python3 tools/validate_registry.py --self-test   # check the checker
python3 tools/validate_registry.py               # check the registry
python3 tools/validate_agents.py                 # check the agent definitions
```

The registry validator caught two genuine modelling errors in its own seed data
on first run. Both are documented in [`registry/README.md`](registry/README.md),
because that is the argument for keeping the registry as checked data rather
than as good intentions.

## Prior art, credited

The graph idea is not new. NCSA's Polyglot proposed an "I/O-Graph" of tool
capabilities searched by shortest path, with edge weights encoding information
retained, around fifteen years ago. It was never executed as a polished product
with a real loss model. This project adopts that structure and extends it with
typed content kinds, loss as a vector, measured verification of every capability
claim, and inference edges the original did not contemplate.

The immediate inspiration is [convert.to.it](https://convert.to.it/)
(`p2r3/convert`, GPL-2.0), which does browser-local WebAssembly conversion and
makes the striking claim that it converts between *any* formats — "What if you
_really_ need to convert an AVI video to a PDF document?"

Having read its source, three of its ideas are adopted here directly:
path-dependent penalties that suppress nonsense category sequences, runtime
dead-end learning that reroutes around a hop that just failed, and a penalized
"accepts anything" escape hatch so the system returns *something*. What it does
not do — and what this project is actually for — is tell you what the conversion
cost you.
