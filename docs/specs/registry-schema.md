# Spec: Registry Schema

Status: **draft** · Owners: `format-registry-curator` (formats), `capability-cartographer` (edges), `engine-integrator` (engines)
Enforced by: `tools/validate_registry.py` (CI)

The registry is data, not code, and it is the most valuable thing in this
repository. Everything below is machine-checked.

## Files

```
registry/formats/<id>.toml    one format
registry/engines/<id>.toml    one backend
registry/edges/<engine>.toml  that engine's capabilities (many edges per file)
```

## Format entries

```toml
id           = "image/png"        # canonical: MIME type where one exists, else "x-huh/<name>"
name         = "Portable Network Graphics"
kinds        = ["Raster"]         # content kinds this format can carry
extensions   = ["png"]            # hint only — never authoritative (invariant 1)
mime_types   = ["image/png"]
puid         = "fmt/13"           # PRONOM identifier, joins us to the preservation world
puid_verified = true              # checked with tools/verify_pronom.py
puid_name    = "Portable Network Graphics 1.2"   # exact PRONOM name, so drift is detectable
spec_url     = "https://www.w3.org/TR/png-3/"

preserves = ["geometry", "color_depth", "color_profile", "alpha", "metadata"]
# properties this format is *capable* of carrying. An edge cannot claim to
# preserve a property the target format cannot represent — CI checks this.

[[signatures]]                    # structural detection (R§2.2)
offset = 0
magic  = "89504E470D0A1A0A"       # uppercase hex, whole bytes
weight = "decisive"               # decisive | strong | weak
# position = "BOF"                # BOF (default) | EOF  — not yet implemented

[detection]
priority_over = []                # PRONOM-style priority: this beats these ids
                                  # e.g. docx sets priority_over = ["application/zip"]
container_of  = []                # e.g. docx: ["application/zip"]

[notes]
text = "Free-form. Gotchas, ambiguities, why a field is what it is."
```

Rules:

- `id` is stable forever. Renaming requires an alias entry, never an edit.
- A format with no signatures and no `container_of` is **undetectable** and must
  say so in `notes` — permitted, but it can only ever be a target, never a
  detected source.
- `puid` is required where PRONOM has one. Absence must be justified in `notes`.
- **`puid` names a representative version, not an exact equivalence.** PRONOM
  identifies format *versions* — `fmt/13` is PNG **1.2**, `fmt/43` is JFIF
  **1.01**, `fmt/276` is PDF **1.7** — whereas this registry identifies
  *formats*. Our signatures are deliberately broader than PRONOM's as a result
  (we match all JPEGs, not only JFIF 1.01), because a converter cares that a
  file *is* a JPEG. Record the divergence in `notes`; do not narrow our
  signature to match PRONOM's.
- `puid_verified = true` requires `puid_name` — CI-enforced. Recording the exact
  PRONOM name is what makes a later PRONOM change detectable rather than silent.
- Verify with `python3 tools/verify_pronom.py [format-id]`. It is network-bound
  and therefore **not** in the default CI gate; it is run deliberately when
  adding or auditing an entry.
- Every format needs at least one corpus specimen before any edge touching it
  is marked `verified` (see `corpus-keeper`).

## Engine entries

```toml
id      = "libvips"
name    = "libvips"
version_pin = ">=8.15,<9"          # pinned: engine behaviour is part of our output
homepage    = "https://www.libvips.org/"

[license]
spdx            = "LGPL-2.1-or-later"
build_profiles  = ["lgpl", "gpl"]  # which of our distributions may include it
patent_exposure = "none"           # tracked separately from license (R§8)

[security]
hostility  = "moderate"            # trusted | moderate | high | extreme
isolation  = "gvisor"              # wasm | gvisor | microvm — minimum tier
network    = false                 # always false; present so a `true` is reviewable
delegates  = false                 # may it invoke other binaries? (R§4.1)
policy_file = "policies/vips.conf" # optional hardening config we ship

[resources]
memory_model    = "streaming"      # streaming | whole-file | unbounded
concurrency     = "thread-safe"    # thread-safe | process-per-job | singleton
crash_expected  = false            # true for LibreOffice (R§3.2)
default_timeout_s = 120

[runtimes]
native = true
wasm   = true                      # is there a usable WASM build? (R§6)
```

`hostility` drives the isolation tier the executor enforces (R§4.4). Assigning
it is `sandbox-warden`'s call, not the integrator's.

## Edge entries

An edge is one verifiable capability claim.

```toml
[[edge]]
engine     = "libvips"
op         = "Transcode"           # one of the six operation classes
from       = "image/png"
to         = "image/jpeg"
from_kind  = "Raster"
to_kind    = "Raster"

[edge.loss]
alpha         = "lost"             # JPEG has no alpha
color_depth   = { value = "degraded", from = 16, to = 8 }
geometry      = "preserved"
color_profile = "preserved"
metadata      = { value = "degraded", note = "EXIF kept, XMP dropped" }
generational  = true               # lossy encode: compounds on repeat (§4)

[edge.cost]
time_model   = "linear_pixels"     # constant | linear_bytes | linear_pixels | linear_duration | superlinear
memory_model = "streaming"
base_ms      = 8

[edge.constraints]
max_input_bytes = 2_147_483_648
requires        = []               # capability tokens this edge needs on its input
provides        = []               # capability tokens this edge guarantees on its output

[edge.verification]
status     = "verified"            # claimed | verified | refuted
specimens  = ["corpus/raster/alpha-gradient.png"]
measured   = { ssim = 0.982 }
verified_at = "2026-08-20"
verified_by = "capability-cartographer"
```

Rules — all CI-enforced:

1. `op` must be one of the six classes in `docs/specs/content-model.md`.
2. If `from_kind != to_kind`, `op` may not be `Transcode` or `Transform`.
3. `op = "Synthesize"` requires a `confidence` field and forces
   `synthesized` entries in the loss vector for the properties it invents.
4. An edge may not claim `preserved` for a property absent from the **target
   format's** `preserves` list. You cannot preserve what the target cannot hold.
5. `status = "verified"` requires at least one specimen and a `measured` block.
6. The planner ignores `claimed` edges by default; `refuted` edges are retained
   deliberately, as documentation of what does not work.
7. Every property named must exist in the closed list in
   `docs/specs/loss-vector.md`.

### Known gap: our signature syntax is weaker than PRONOM's

PRONOM byte sequences use constructs our `magic` field cannot express:

| Construct | Example | Meaning |
|---|---|---|
| wildcard byte-count | `52494646{4}41564920` | skip exactly 4 bytes (the RIFF size field) |
| open range | `{14-*}` | skip 14 or more bytes |
| bounded range | `504B01{43-65531}504B0506` | ZIP central directory location |
| alternation | `(46\|460A\|460D)` | any of these byte strings |
| arbitrary gap | `...49484452*69545874` | any number of bytes between |
| EOF anchoring | `Absolute from EOF` | offset counted from end of file |

Consequences we accept for now, and should not forget:

- Our PNG signature cannot detect truncation; PRONOM's EOF anchor on the `IEND`
  chunk can.
- Our ZIP signature matches the local file header at BOF, while PRONOM locates
  the central directory — **the structurally correct approach, and notably the
  part a zip bomb manipulates** (R§4.3).
- Our PDF signature is version-agnostic by choice, which is a deliberate
  divergence rather than a gap.

`tools/verify_pronom.py` reports which entries have a PRONOM signature richer
than we can express, so the gap stays visible. Closing it means extending the
detector first and the schema second.

### Capability tokens

`requires` / `provides` are free-form strings naming a precondition an engine
imposes that the *format* alone does not express. whisper.cpp accepts WAV, but
not any WAV — it needs 16 kHz mono, so its edge declares
`requires = ["wav_16khz_mono"]` and FFmpeg's resample edge declares the matching
`provides`. The planner then treats the normalization as a hop to be scheduled
rather than a detail hidden inside an adapter (R§3.6), which is what makes the
downmix visible to the user in the final receipt.

Tokens are matched by exact string. The vocabulary is intentionally open —
unlike loss properties — because engine preconditions are open-ended. A token
with a `requires` and no `provides` anywhere in the registry is a dead end and
CI warns about it.

## Detection priority

When several formats' signatures match, PRONOM-style priority resolves it
(R§2.2): every DOCX is a valid ZIP, so `docx` declares
`priority_over = ["application/zip"]` and wins. Priority must form a DAG — CI
rejects cycles.

## Edge fields added after the seed schema

Two fields were added by measurement rather than by design, and both exist
because an edge without them turned out to be under-specified.

### `invocation` — how the engine was called (ADR-0013)

```toml
[[edge]]
engine     = "libvips"
invocation = "vips webpsave IN OUT"
```

Loss is a property of `(engine, version, invocation, from, to)`, not just the
engine. `vips webpsave` is **lossy** (SSIM 0.8362 on a photo) and
`vips webpsave --lossless` is not. Both are "libvips, PNG to WebP".

- **Different flags are different edges**, not variants of one.
- Defaults are worth recording precisely because they are what most callers get.
- An in-process library with fixed parameters may omit this; anything invoked as
  a subprocess should not (**rule 8**, a warning rather than an error, because
  the validator cannot reliably tell the two apart).

#### Which makes `invocation` part of the edge's identity (rule 9)

An edge is identified by `(engine, from, to, invocation)`. **No two entries may
share that tuple** — across the whole registry, not just within one file
(**rule 9**). Two entries for the same tuple give a consumer no way to know
which to believe, and the two will drift.

Note what the rule does *not* forbid. Two entries differing only in
`invocation` are the normal case and the reason the field exists: `vips
webpsave` and `vips webpsave --lossless` are separate rows, and must be.

This rule was written after the fact. A duplicate `image-rs png → gif` entry
reached the published dataset, because the same edge was measured twice for
different reasons — once for raster coverage, once for palettisation policy —
and landed in two different files. It was found by loading
`dataset/conversion-loss-registry.json` and grouping the edges the way a
consumer would, not by any check here. Files are how the registry is *stored*;
they are not a namespace.

```toml
[edge.loss]
alpha = { value = "lost", policy = "composite_black" }
```

`lost` says a property is gone. It does not say what took its place, and for
some properties that is the entire question a user has. libvips composites
transparency onto **black**; image-rs and ImageMagick keep whatever colour was
underneath. All three correctly report `alpha = lost`, and a logo comes out
looking completely different.

The vocabulary is **closed per property** (**rule 7**), for the same reason the
property list is closed: an open set makes two entries incomparable, and a typo
silently becomes a new category.

| Property | Allowed policies |
|---|---|
| `alpha` | `composite_black`, `composite_white`, `discard_channel`, `premultiplied` |
| `audio_channels` | `downmix_average`, `take_first`, `downmix_weighted` |
| `bit_depth` | `round`, `truncate` |
| `color_depth` | `dither_diffuse`, `palette_nearest`, `truncate` |

Extending a list is a documentation change plus a validator change, in the same
commit. Adding a policy to a property that has no vocabulary yet is an error —
define the vocabulary first, so the second person to need it agrees with the
first.

A policy is a **claim about engine behaviour** and carries the same verification
burden as the edge: only record one you have measured.

`bit_depth` is the cautionary example. When ffmpeg and SoX were first measured
7.3 dB apart, `dither` looked like the obvious policy name. It was wrong: the
cause turned out to be **rounding** (ffmpeg truncates, introducing half an LSB of
DC offset; SoX rounds to nearest), and dither is a separate axis already captured
by the invocation. Writing the vocabulary while the mechanism was still unknown
would have encoded the wrong distinction permanently. See `RESEARCH.md §14.6`.


## Evidence must come from a build you actually ship (rule 11)

An edge's `verification.measured.engine_build` must satisfy its engine's
`version_pin`. CI rejects the pair otherwise (**rule 11**).

`verified` is a promise that someone ran this conversion and watched what
happened. If the build they used is excluded by the engine's own pin, a consumer
who resolves the engine by that pin and then trusts `verified` is relying on
evidence from software they will never run. That is not a hypothetical: all four
FFmpeg edges were measured on 6.1.1 under a pin of `>=8.0,<10`, so *every*
verified FFmpeg edge rested on an out-of-pin build while reading as confirmed.

The check is deliberately tolerant about spelling, because `engine_build` is
free text written by whoever ran the measurement — `ffmpeg 6.1.1`,
`vips-8.15.1`, `SoX v14.4.2` all parse. The first dotted-number run is the
version. If no version can be read from either side, the rule stays quiet rather
than guessing.

Either end may move to satisfy it: widen the pin to cover what was measured, or
re-measure on an in-pin build. Widening is a licensing statement as well as a
compatibility one, so it needs the same care as any other registry claim.

## An unaudited licence cannot claim `lgpl` (rule 12)

An engine whose `license.verified` is `false` may declare the `gpl` build
profile but not `lgpl` (**rule 12**).

The two profiles are not symmetric. `gpl` means self-host only; `lgpl` means
safe to distribute inside a proprietary product (ADR-0007). The planner prunes
on this field *before* search, so a wrong `lgpl` never surfaces as an error — it
silently yields a route that should not have existed, in a shipped artifact.
Every other kind of wrong entry here costs a measurement; this one costs a
licence violation, and it cannot be taken back after a release.

`registry/engines/libreoffice.toml` reasoned this out first and narrowed itself
to `gpl`, noting that "a wrong `build_profiles` value produces routes we must not
ship". `imagemagick.toml` then claimed `lgpl` while stating in the same block
that an audit was required before its profiles could be trusted. Rule 12 is that
precedent, enforced.

## A measurement must be re-runnable (rule 13)

`verification.measured.tool` should name something that exists in this
repository. CI warns when a citation resolves to no runnable artifact
(**rule 13**). Both of these spellings resolve:

```toml
tool = "core/examples/libvips_edges.rs; vipsheader for the depth check"
tool = "cargo run --example probe_edges"   # -> core/examples/probe_edges.rs
```

A **warning**, not an error, on rule 8's precedent: some measurements genuinely
are one-liners and the validator cannot reliably tell those from a procedure
that should have been committed.

The gap it closes: every audio edge carried `status = "verified"` while citing
`"corpus/generate_audio.py + ffprobe band-energy analysis"`. The generator was
committed; the analysis never was. Those numbers could not be reproduced — and
therefore could not be **contradicted**, which `CONTRIBUTING.md` calls the most
valuable contribution anyone can make. An unfalsifiable number in a dataset that
sells falsifiability is a contradiction at the centre of the thing.

**This rule would not have caught that bug**, and saying so matters more than
the rule: the citation named one real file plus prose, so an artifact resolved
and it passed. What rule 13 catches is the blunter form — a citation naming
nothing runnable at all, which is what the remaining four audio edges did. See
`RESEARCH.md §17.1`.

## Coverage claims are data, not prose (rule 10)

The README states the registry's size in its own words:

```
**14 formats · 6 engines · 24 conversions (23 verified)**
```

CI recomputes those four numbers and rejects the line if it disagrees
(**rule 10**), printing the corrected line to paste in. Engines are counted as
those **carrying at least one edge**, not those merely described in
`registry/engines/` — the line claims what has been measured, and an engine with
a description and no edges has measured nothing.

The rule exists because the line read `7 engines · 19 conversions` for some time
against a registry holding 6 and 24. A wrong number in the README is worse than
one in a TOML file: it is the number people repeat.

## Versioning

The registry is versioned as a whole (`registry/VERSION`). Receipts pin the
version they were planned against, because a registry update that changes a
route silently changes a user's output (R§10.3).
