# 0001 — Plan routes; do not tabulate pairs

Status: **accepted** · 2026-08-20

## Context

The entire field ships a lookup table from (source, target) to a command line.
That works until you want breadth: a thousand formats is a million pairs
(R§1.5). It also cannot answer "AVI → PDF," because there is no single correct
entry to put in the cell — there are at least four defensible answers (R§5.2).

NCSA's Polyglot work established the alternative fifteen years ago: an
I/O-Graph of what each tool reads and writes, searched by shortest path, with
edge weights encoding information retained (R§1.3). It was never executed as a
polished product with a real loss model.

## Decision

Formats and content kinds are nodes; engine capabilities are typed, weighted
edges. A conversion request is a pathfinding problem. The planner returns
**ranked plans with generated explanations**, not a single command.

Pivot formats emerge as high-degree nodes, but they are not mandated: a direct
edge beats a pivot route when it is better, and the planner decides per request.

## Consequences

- Adding an engine adds edges and immediately enables *combinations* nobody
  enumerated — the Polyglot `.lwo → .3ds → .stp` effect (R§1.3).
- The registry becomes the primary asset, and its correctness becomes the
  primary risk. Hence verification (ADR-0003) and a dedicated curator role.
- We must be able to explain routes, or ranking is unaccountable (ADR-0004).
- Planning cost is now on the request path. Mitigated by caching plans keyed on
  (registry version, source properties, target, profile).

## What would change our mind

If real usage turns out to be overwhelmingly single-hop within one domain — if
the long tail of cross-domain routes goes unused — the planner is overhead and
a table would do. Measure route-length distribution in production before
concluding this.
