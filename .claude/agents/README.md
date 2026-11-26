# Agents

Eleven specialists. The division is by **recurring problem with its own
invariants**, not by layer of the stack — because the hard parts of this project
are not layers. Keeping a registry honest, deciding what a cross-domain
conversion *means*, and containing a memory-unsafe parser are three different
skills that happen to touch the same files.

## Map

### The graph — the project's core asset

| Agent | Owns | Use when |
|---|---|---|
| `format-registry-curator` | `registry/formats/` | Adding or correcting a format; detection is misidentifying files; a `preserves` list looks wrong |
| `capability-cartographer` | `registry/edges/` | Adding or verifying a capability claim; promoting `claimed → verified`; measurement contradicts a declared loss vector |
| `engine-integrator` | `registry/engines/` + adapters | Wrapping a new backend; upgrading a pinned version; adapter arguments, streaming, timeouts, error mapping |

### The thinking

| Agent | Owns | Use when |
|---|---|---|
| `route-planner-engineer` | Graph search, cost model, ranking | Pathfinding, the cost function, plan determinism, or the planner picking a route that looks wrong |
| `semantic-bridge-designer` | Cross-domain semantics | Deciding what a conversion between unlike kinds should *mean*; classifying an operation; designing intent options; whether a conversion should exist |

### The checking

| Agent | Owns | Use when |
|---|---|---|
| `sandbox-warden` | Threat model, sandbox | Anything security-shaped. **Has veto power over engine integration.** |
| `fidelity-analyst` | Quality measurement | Fidelity harness, calibrating declared loss against measured, quality regressions |
| `corpus-keeper` | Test corpus | Adding specimens, reference files, fuzz seeds, minimized reproductions, specimen licensing |
| `conversion-forensics` | Debugging | A specific file fails, converts wrong, is misidentified, or is far slower than estimated |

### The keeping-honest

| Agent | Owns | Use when |
|---|---|---|
| `license-compliance-officer` | Licensing and patents | Engine additions and upgrades, build-profile eligibility, attribution, patent exposure |
| `docs-and-adr-steward` | Documentation integrity | Writing or superseding an ADR; code has drifted from a spec; the dossier needs a finding |

## How they interlock

The dependencies are real, and mostly flow toward verification:

```
format-registry-curator ─┐
engine-integrator ───────┼─▶ capability-cartographer ─▶ route-planner-engineer
corpus-keeper ───────────┘            ▲                          │
                                      │
                          fidelity-analyst

sandbox-warden           ─── vetoes ──▶ engine-integrator
license-compliance-officer ─ gates ───▶ engine-integrator
conversion-forensics     ─── routes findings to whoever owns the cause
docs-and-adr-steward     ─── keeps everyone's claims traceable
```

Two agents hold power over others deliberately: `sandbox-warden` can block an
engine integration, and `license-compliance-officer` gates build-profile
eligibility — which is load-bearing on *correctness*, not just paperwork, because
the planner prunes ineligible engines before search.

`fidelity-analyst` closes the loop: measured results that contradict declared
loss vectors are filed back to `capability-cartographer` as registry bugs. That
feedback edge is what makes the whole scheme falsifiable rather than
aspirational, and it is the thing the academic prior art never built.

## Conventions

- Each agent's body is its system prompt. It should encode **judgement** —
  what to refuse, what looks like a bug but is not, what the traps are — rather
  than restating documentation the agent can read.
- `model: opus` and `effort: high` are reserved for the three roles where a
  wrong call is expensive to undo: `route-planner-engineer`,
  `semantic-bridge-designer`, `sandbox-warden`.
- Descriptions say **when to use this agent**, not just what it is. The main
  agent routes on that text.
- `python3 tools/validate_agents.py` checks the frontmatter, name/filename
  agreement, and that descriptions are specific enough to route on.
