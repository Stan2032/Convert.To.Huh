# Contributing

The deliverable is the **dataset**: what each file-format conversion destroys,
measured rather than asserted (ADR-0011). Everything else exists to produce and
check it.

The most valuable contribution is a **measurement**. The second most valuable is
a measurement that **contradicts** one already here.

## The rule that matters

> **A claim copied from documentation is not a capability.**

Assimp documents glTF export as "partial" and FBX as "experimental." Engine
READMEs describe intent. Neither tells you what actually survives a conversion.
So an entry is only `verified` once someone has run it against a real file and
recorded what happened.

Unmeasured entries are still welcome — record them as `claimed`. They are
published as untested hints and the planner ignores them.

## Adding a measured conversion

You need the conversion to work locally, plus a specimen that actually exercises
the properties in question. A JPEG with no transparency proves nothing about
whether a conversion preserves alpha.

1. **Add a specimen** to `corpus/`. Prefer *generating* it — see
   `corpus/generate.py`. A script is reproducible, tiny in git, unambiguously
   ours, and regenerable when a format detail changes. Say what the specimen
   exercises and why.

2. **Measure it.** `core/examples/probe_edges.rs` measures raster pairs across
   the corpus and reports SSIM plus per-property observations. Extend it, or
   write an equivalent for your domain.

   Two distinctions the harness cares about, and both are easy to get wrong:
   - **"The format supports it" is not "this file uses it."** Dropping alpha
     from a fully opaque image is not an observable loss. Report
     `unexercised`, not `preserved`.
   - **"It decoded" is not "it is correct."** Always compare content. WebP
     looked fine until SSIM confirmed it; GIF looked fine and scored 0.81.

3. **Write the entry** in `registry/edges/<engine>.toml` with a `measured`
   block containing the actual numbers, the specimens used, and the date.

4. **Run the gate** — all four must exit 0:

   ```bash
   python3 tools/validate_registry.py --self-test   # validator checks itself
   python3 tools/validate_registry.py
   python3 tools/validate_agents.py
   python3 tools/check_links.py
   cargo test  --manifest-path core/Cargo.toml
   cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings
   ```

5. **Regenerate the dataset** so the published file matches the registry:

   ```bash
   ./core/target/release/huh export --out dataset/conversion-loss-registry.json
   ```

### If the validator rejects your entry

**The usual fix is not to weaken your data.** Rule 4 rejections — claiming
`preserved` for a property the target format cannot carry — almost always mean a
*format entry* is wrong, or the chosen target is. Both seed-data rejections in
this repository turned out to be real bugs elsewhere. Follow the error to its
cause before editing your numbers.

## Contradicting an existing entry

Genuinely the most useful thing you can send. If you measure `image/png →
image/jpeg` through some engine and get a different result to what is recorded,
that is a finding, not a conflict.

Open an issue or PR with your specimen, your engine version, and your numbers.
Engine version matters enormously — loss is a property of
`(engine, version, from, to)`, and whether results are stable across versions is
an open question this dataset should eventually answer (ADR-0011).

If confirmed, the entry is corrected and the disagreement recorded in `[notes]`.
Nothing is quietly overwritten.

## Adding a format

`registry/formats/<mime>.toml`. The parts people get wrong:

- **`preserves` is what the format CAN carry**, not what some engine writes.
  GIF omits `color_depth` because it is palettised to 256 colours — that is a
  property of GIF, not of any encoder.
- **Signatures**: same `group` means alternatives (OR); different or absent
  groups mean all are required (AND). TIFF's two byte-order magics are
  alternatives; AVI's `RIFF` at 0 and `AVI ` at 8 are both required. Getting
  this backwards silently broke ZIP detection here for a day.
- **PUIDs** are checked against PRONOM's live API by
  `python3 tools/verify_pronom.py` (network-bound, not in CI). If you cannot
  find a PUID, **leave it empty and say so in `[notes]`** rather than guessing.
  A plausible-looking wrong identifier is worse than an absent one.

## Checking an entry against real files

`corpus/govdocs1/` is a tier of 146 real files from Govdocs1, fetched on
demand and verified by SHA-256 (`tools/fetch_govdocs.py`), then run through
`huh` by `tools/run_govdocs.py`. Use it to show whether a loss you measured on
a synthetic specimen applies to real files, and how often. Both scripts are
network-bound and not in CI. See `corpus/govdocs1/README.md`.

## Adding an engine

Read `docs/specs/engine-manifest.md` and ADR-0003 first. Engines are assumed
hostile: they parse untrusted input written by strangers, and several have CVE
histories to prove the point. Adapters build **argument vectors, never shell
strings**, and map failures to the error taxonomy rather than "non-zero exit."

An engine's `[security]` block needs `sandbox-warden`'s eye, and its `[license]`
block gates which build profiles may ship it — that field is load-bearing on
correctness, not paperwork. LibreOffice was recorded as `MPL-2.0` here until an
audit found GPL-3.0 components, which removed every Office route from the LGPL
profile.

## Style

- **Cite the dossier.** Design claims reference `docs/RESEARCH.md` sections. If
  the evidence is not there, add it there first — with a design implication, or
  it is trivia.
- **Record what surprised you.** The `[notes]` fields exist because those facts
  cost someone real time to rediscover.
- **ADRs are binding.** If your work needs to violate one, that is a new ADR
  superseding it and saying what changed our mind — not a quiet edit.
- **Negative results are results.** `refuted` is a valid status and a conversion
  that does not work is useful to know about.

## Licensing

Code is MIT. **Data is CC0** — measurements of how software behaves are facts,
and facts should not carry conditions. By contributing you agree your data goes
out under CC0 and your code under MIT.
