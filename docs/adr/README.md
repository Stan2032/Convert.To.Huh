# The Registry

Declarative data describing every format we know, every engine we can run, and
every capability we claim. This is the most valuable artifact in the repository
(`docs/ARCHITECTURE.md`), and it is machine-checked by
`tools/validate_registry.py` in CI.

```
formats/   one file per format   — owner: format-registry-curator
engines/   one file per backend  — owner: engine-integrator (+ sandbox-warden for [security])
edges/     one file per engine   — owner: capability-cartographer
VERSION    the registry version pinned into every receipt
```

Schema: `docs/specs/registry-schema.md`.

## Current state — read this before assuming anything works

**Every edge in this registry is `status = "claimed"`. Zero are `verified`.**

The planner ignores `claimed` edges by default (`registry-schema.md`, rule 6),
which means the system as committed converts **nothing**. That is deliberate and
correct: a claim copied from a README is not a capability
(`docs/RESEARCH.md §3.5`). Edges become real when `capability-cartographer`
runs them against a corpus specimen and records a measurement.

Similarly, PUIDs carry `puid_verified = false` until `format-registry-curator`
confirms them against PRONOM. They are recorded from memory and are exactly the
kind of plausible-looking data that is wrong often enough to matter.
