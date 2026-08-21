# Architecture Decision Records

Ten binding decisions. An ADR constrains what may be built; if your work
requires violating one, **stop and revisit the ADR first** rather than working
around it. Reversal means a new ADR that supersedes the old one and states what
changed our mind — never a quiet edit.

Every ADR carries a **"What would change our mind"** section. That section is
the point: a decision whose falsification conditions are unstated is a
preference wearing a decision's clothes.

| # | Decision | Status |
|---|---|---|
| [0001](0001-plan-routes-not-pairs.md) | Plan routes; do not tabulate pairs | accepted |
| [0002](0002-one-core-two-runtimes.md) | One core, compiled to native and WASM | accepted |
| [0003](0003-engines-are-hostile.md) | Engines are hostile and unreliable | accepted |
| [0004](0004-loss-is-a-vector.md) | Loss is a vector, not a scalar | accepted |
| [0005](0005-never-trust-the-extension.md) | Never trust the extension | accepted |
| [0006](0006-local-first-with-computed-escape.md) | Local-first, with a computed escape to the server | accepted |
| [0007](0007-build-profiles.md) | Build profiles are a first-class concept | accepted |
| [0008](0008-plan-execution-state-machine.md) | Plan execution is a state machine over persisted hops | accepted |
| [0009](0009-synthesis-is-never-silent.md) | Synthesis is never silent | accepted |
| [0010](0010-conversion-is-not-a-cipher.md) | Conversion is not a cipher; it is a canonicalizer | accepted |

## Which ones bite most often

- **0004** and **0009** shape every registry entry. Most rejected edges fail
  because of one of them.
- **0005** is the one people violate by accident, usually by reaching for a
  file extension because it is right there and convenient.
- **0007** decides whether a route legally exists at all, and prunes *before*
  search rather than failing at runtime.
- **0010** redirected the project's purpose after measurement refuted the
  original premise. It is the clearest example of the repository's habit of
  recording negative results rather than deleting them.

## Reading order for someone new

0001 (why a graph), then 0004 (what an edge costs you), then 0003 (why
execution is paranoid). The rest can be read when the relevant work comes up.
