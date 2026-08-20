# 0008 — Plan execution is a state machine over persisted hops

Status: **accepted** · 2026-08-20

## Context

A queue guarantees a worker receives the job; what happens *inside* the job is
outside its contract, and long jobs must heartbeat or be declared stalled and
failed. Durable execution engines persist every step and resume exactly where
they stopped. The stated dividing line: queues suit atomic, restartable work;
the moment you have chained steps you are building a workflow engine on top of
a queue, badly (R§7.1).

A conversion plan is by definition chained steps with expensive intermediates.
That is the textbook durable-execution case. Adopting a full workflow engine on
day one is nonetheless disproportionate.

## Decision

Model execution as an **explicit state machine over persisted, content-addressed
per-hop artifacts**, owned by us, from the start. The queue or workflow runtime
underneath stays swappable. Long hops emit heartbeats.

## Consequences

- A failed 5-hop plan resumes at hop 4 instead of restarting — which matters
  enormously when hop 3 was a 40-second ASR pass.
- Content-addressed hops deduplicate across users and across plans for free.
- We carry state-machine code we could have rented. Accepted, because it keeps
  the semantics ours and the runtime replaceable.
- Intermediates need a retention policy — they are derived from user data and
  must expire.

## What would change our mind

If hop-level resume proves to matter less than expected and the state machine
becomes a maintenance burden, adopting Temporal (or equivalent) wholesale is
the intended migration, and this design is deliberately shaped to allow it.
