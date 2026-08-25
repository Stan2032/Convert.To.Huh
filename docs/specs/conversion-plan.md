# Spec: Conversion Plan

Status: **draft** · Owner: `route-planner-engineer`

The contract between planner and executor, and the thing a receipt pins. It is
serializable, inspectable, and identical in the browser and on the server.

## Shape

```jsonc
{
  "plan_id": "pl_01J...",
  "registry_version": "2026.08.1",     // pinned: routes change between versions
  "source": {
    "format": "video/x-msvideo",
    "detected_by": "signature",        // signature | structure | ml | extension
    "confidence": 1.0,
    "declared_extension": "avi",       // recorded separately — mismatch is a signal
    "observed": { "duration_s": 631, "width": 1920, "height": 1080, "has_audio": true }
  },
  "target": { "format": "application/pdf", "intent": "transcript" },
  "runtime_profile": "browser",        // browser | server
  "hops": [ /* ... */ ],
  "totals": {
    "loss": { "time": "lost", "text_as_text": "synthesized", "structure": "degraded" },
    "estimated_ms": 42000,
    "peak_memory_bytes": 1610612736,
    "contains_synthesis": true
  },
  "rationale": "Transcribes the audio with a speech model and typesets it. The text is machine-generated, not extracted. Discards all video.",
  "budgets": { "max_output_bytes": 209715200, "max_pixels": 8000000000, "max_wall_s": 300 },
  "alternatives": ["pl_01J...b", "pl_01J...c"]
}
```

## Hops

```jsonc
{
  "index": 2,
  "engine": "whisper.cpp",
  "engine_version": "1.7.2",
  "op": "Synthesize",
  "from": { "kind": "AudioPCM", "format": "audio/wav" },
  "to":   { "kind": "Text", "format": "text/plain" },
  "params": { "model": "base.en", "language": "en" },
  "loss": { "text_as_text": "synthesized" },
  "confidence": 0.82,
  "isolation": "gvisor",
  "budget": { "max_wall_s": 180, "max_output_bytes": 10485760 },
  "cache_key": "sha256:...",
  "estimated_ms": 38000
}
```

## Rules

1. **A plan is data.** The executor may not consult the registry — everything it
   needs is in the plan. This is what makes plans reproducible, cacheable, and
   inspectable by a user before they run.
2. **Every hop is independently budgeted and independently cached.** A failed
   5-hop plan resumes at the failed hop; an identical hop from an earlier plan
   is a cache hit (R§7.1).
3. **`totals.loss` is computed by the composition rules** in
   `docs/specs/loss-vector.md`, not by any hop declaring it. `lost` is
   absorbing; `synthesized` is contaminating.
4. **`contains_synthesis` is derived, never set by hand.** True iff any hop is
   `Synthesize`. It drives every downstream marking obligation.
5. **`rationale` is generated from `totals.loss`.** Never hand-written.
6. **`isolation` per hop is the maximum** of the engine's declared minimum tier
   and any policy floor. The executor may raise it, never lower it.
7. **Plans are deterministic.** Same registry version + same input properties +
   same target + same profile ⇒ byte-identical plan (modulo `plan_id`). This is
   a property test, not an aspiration.
8. **`alternatives` carries sibling plans that differ in intent**, so the UI can
   offer them without re-planning (R§5.2).

## Lifecycle

```
planned → [user confirms if ambiguous or if profile = server] → executing
        → { completed | failed | refused }
```

`refused` is a first-class terminal state, not an error: budget exceeded
(R§4.3), no legal route under the active build profile (R§8), or a required
engine unavailable in the requested runtime profile. Refusals are explained in
the same generated plain language as rationales.
