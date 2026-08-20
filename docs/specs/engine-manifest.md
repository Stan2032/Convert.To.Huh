# Spec: Engine Manifest & Adapter Contract

Status: **draft** · Owners: `engine-integrator` (contract), `sandbox-warden` (security fields)

What it takes to add a backend, and what the system is allowed to assume about it.

## The stance

Every engine is assumed **hostile and unreliable**. Hostile because it parses
attacker-supplied bytes in memory-unsafe code with a CVE history (R§4.1–4.2).
Unreliable because the best-maintained ones still crash on ordinary workloads
(R§3.2). The adapter contract exists to make both assumptions survivable.

## Adapter responsibilities

An adapter is a thin, stateless translator. It must:

1. **Build arguments from a hop, never from a string.** No shell. No
   interpolation. Argument vectors only. ImageTragick was, at bottom, string
   interpolation into a shell (R§4.1).
2. **Declare its input constraints as edge `requires`**, rather than assuming.
   whisper.cpp accepts only standard WAV; that constraint is an edge in the
   graph, so the planner inserts the normalization hop (R§3.6).
3. **Map exit conditions to the error taxonomy** below. "Non-zero exit" is not
   a diagnosis.
4. **Stream where the engine streams; declare `whole-file` where it does not.**
   The planner's memory estimate depends on this being honest.
5. **Never touch the network.** Adapters run with no network namespace; an
   adapter that would need one is not integrable (R§4.5).
6. **Emit progress heartbeats** for hops that can exceed the stall threshold
   (R§7.1).

## Error taxonomy

| Class | Meaning | Retryable | Surfaced as |
|---|---|---|---|
| `input_malformed` | The file is not what it claimed, or is corrupt | no | "This file appears damaged" |
| `input_unsupported` | Valid, but uses a feature the engine lacks | no | names the feature |
| `budget_exceeded` | Hit a declared limit (R§4.3) | no | names the limit and the value |
| `engine_crash` | Signal death, assertion, internal fault | once | "This file crashed the converter" |
| `engine_timeout` | Wall clock exceeded | once | |
| `engine_unavailable` | Not present in this runtime profile | no | triggers re-plan |
| `sandbox_violation` | Attempted a forbidden operation | **no — never retry** | security event |
| `internal` | Our bug | yes | generic |

`sandbox_violation` is never retried and always alerts. An engine attempting a
forbidden syscall or a network connect is either exploited or exploitable, and
in both cases the correct response is to stop, not to try again.

## Onboarding checklist

An engine is integrated when all of these are true:

- [ ] `registry/engines/<id>.toml` complete, `hostility` assigned by `sandbox-warden`
- [ ] License and patent exposure recorded; build-profile eligibility set (R§8)
- [ ] Version pinned — engine version is part of our output, not an ambient detail
- [ ] Adapter builds argument vectors, never shell strings
- [ ] Every claimed capability is an edge with `status = "claimed"` at minimum
- [ ] Hardening config shipped where the engine has one (ImageMagick
      `policy.xml`, Ghostscript `-dSAFER`) — as defence in depth, **never as the
      boundary** (R§4.1)
- [ ] Delegation disabled: no delegate binaries on `PATH`
- [ ] Crash and timeout behaviour observed and recorded, not assumed
- [ ] At least one corpus specimen per claimed edge
- [ ] Fuzz seed corpus registered (R§9)
- [ ] Runs green under the target isolation tier

## Notes on specific engines

Recorded because these cost real time to rediscover.

**ImageMagick** — delegates PDF/PS/EPS/XPS to Ghostscript by default. Sever it.
The `policy.xml` hardening is necessary and demonstrably insufficient: published
bypasses defeat the default, "limited" *and* "secure" policies (R§4.1).

**Ghostscript** — `hostility: extreme`. A sustained history of `-dSAFER`
escapes, including one exploited in the wild via EPS disguised as JPEG
(R§4.2). Microvm tier only.

**LibreOffice** — `crash_expected = true`, `concurrency = "singleton"`. Its own
tooling does not restart it on crash by design, and its conversion timeout
takes the server down with it (R§3.2). One document per instance, supervised,
aggressive wall-clock kill, own pool.

**libvips** — default raster engine: ~4–8× faster and an order of magnitude
lighter than ImageMagick, though measurably worse on small images, so engine
choice is a planner decision informed by input size (R§3.1).

**FFmpeg** — LGPL by default; `--enable-gpl` pulls in libx264 and changes our
distribution obligations. Patent exposure for H.264/H.265 is separate from
licensing and applies regardless (R§8).

**Assimp** — import breadth ≠ export breadth. glTF export is documented as
partial, FBX/3MF as experimental. Verify every direction against a specimen;
never copy claims from the README (R§3.5).
