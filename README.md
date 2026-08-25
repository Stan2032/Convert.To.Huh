# Convert.To.Huh

**Every file conversion costs you something. Nobody writes down what.**

Convert a PNG to a JPEG and you lose transparency. Convert a PDF to images and
the text stops being text. Convert a WAV to an MP3 and detail disappears. These
are well known to the people who hit them and written down nowhere you can
query.

This project measures those costs and publishes them as data.

📦 **[`dataset/conversion-loss-registry.json`](dataset/conversion-loss-registry.json)** — CC0, free to use, no attribution required.

---

## The one-paragraph version

[PRONOM](https://www.nationalarchives.gov.uk/PRONOM/) is the standard database
for *identifying* files — ~2,000 format signatures, used by archives worldwide.
It tells you **what a file is**. Nothing tells you **what converting it costs**.
This is an attempt at that missing database, plus the tool that produces it by
running real conversions against real files and measuring the results.

## Who this is for

**If you build software that converts files** — you can look up what a
conversion destroys before you ship it, instead of finding out from a bug
report. Filter on `verification == "verified"` and you get measurements, not
guesses.

```python
import json
d = json.load(open("conversion-loss-registry.json"))

for e in d["edges"]:
    if e["from"] == "image/png" and e["to"] == "image/jpeg":
        print(e["engine"], "|", e.get("invocation"), "|",
              e["loss"]["alpha"], "->", e.get("loss_policy", {}).get("alpha"))
```
```
image-rs | None                                     | lost -> discard_channel
libvips  | vips jpegsave IN OUT                     | lost -> composite_black
libvips  | vips jpegsave IN OUT --background 255    | lost -> composite_white
```

Three rows for "PNG to JPEG", and the difference is the whole point. All three
**correctly** report that transparency is lost; they disagree about what
replaces it, so your logo comes out black, white, or with the channel simply
dropped. Note the third row: same engine, same formats, one different flag.
That is why loss is recorded against the *invocation*, not the engine.

**If you run an archive or library** — the digital preservation field discusses
conversion loss constantly, in prose. This is the same knowledge in a form you
can compute with when choosing a preservation format.

**If you need to clean a file you don't trust** — `huh clean` rebuilds a file
from its content and tells you what it removed *and what that cost you*. See
below.

**If you are just curious what your tools do to your files** — most of the
findings here surprised the people measuring them.

## Two things you can actually run

### `huh clean` — strip what shouldn't be there

A file can be **completely valid** and still carry things you never see: text
metadata naming you and your machine, private chunks decoders skip, data
appended after the end-of-image marker, or in a PDF, code set to run when the
document opens.

```console
$ huh clean report.pdf clean.pdf
detected application/pdf (by signature, confidence 1.00)
strategy render-and-rebuild (1090 -> 9287 bytes)
  ran: pdftoppm -r 150 -png in.pdf page
  ran: vips jpegsave page-1.png page-1.jpg --Q 90

removed:
  - all PDF structure (1 page(s) rebuilt from pixels)
  - document metadata (title, author, producer, creation tool)
  - any /OpenAction, /JavaScript, /EmbeddedFile or annotation

properties destroyed: text_as_text, structure, interactivity, metadata, fonts
```

It doesn't scan for threats. It parses the content, throws the container away,
and builds a new one — so anything that wasn't content doesn't make the trip,
including attacks nobody has catalogued yet.

**The second half of that output is the part nobody else gives you.** The file
is now safe *and* you can no longer search it or copy from it. Tools that hide
that cost are lying by omission.

> ⚠️ For real hostile-document work, use
> **[Dangerzone](https://dangerzone.rocks/)** — it does this with far better
> isolation (a gVisor sandbox inside a container). This is a reference
> implementation for generating registry data, not a replacement.

### `huh convert` — convert, and be told the price

```console
$ huh convert logo.png logo.jpg
detected image/png (by signature, confidence 1.00)
route:   image/png --libvips[Transcode]--> image/jpeg
effect:  Loses transparency and metadata — transparent areas become black.
         Reduces colour depth and fine detail. Keeps colour accuracy and resolution.
```

That "transparent areas become black" is measured, not assumed — and it is
engine-specific. The same conversion through ImageMagick keeps whatever colour
was underneath instead. Both are correct; they look completely different.

## What makes this different from just… knowing things

Everything here is **measured against a real file, or marked as unverified.**

Every entry carries a `verification` field:

| Value | Meaning |
|---|---|
| `verified` | Run against a real specimen and measured. Trust it. |
| `claimed` | Copied from documentation, **not tested**. A hint, not a fact. |
| `refuted` | Tested and found not to work. |

The discipline exists because the data is wrong surprisingly often. In this
repository the checks have repeatedly rejected hand-written entries — several
written minutes earlier by the person who had just written the spec they broke.
The cases are written up in [`registry/README.md`](registry/README.md),
including one the checks **missed**.

And once, memorably, the *measuring instrument itself* was wrong — a fidelity
score of 0.9950 that turned out to be scoring an unstated assumption rather than
the conversion. It reached a pull request description and two decision records
before a conversion that mathematically could not lose anything scored 0.47 and
gave it away. Both are written up in full rather than quietly fixed.

## Current scope — small, on purpose

**14 formats · 6 engines · 24 conversions (23 verified)**

Images, PDF and audio. It grows by measurement, not assertion, which is slow
deliberately. Coverage is not the goal; **trustworthy** coverage is.

(That line is checked against the registry by CI, because it went stale once
already while nobody noticed.)

## The project's origin, since it explains the shape

This began as a different idea: encrypting files by converting them, with the
key derived from the *path* of conversions taken. Rather than assume, it was
tested — and refuted three separate ways:

1. **Seven different lossless routes produce byte-identical output.** The path
   leaves no trace, so it cannot be a key.
2. **Restricting to lossless conversions makes it worse, not better** — path
   independence *is* what "lossless" means.
3. **Changing medium repeatedly doesn't hide anything.** Three media changes
   with every trail stripped, and the original comes back byte-identical in one
   pass. ([Run it yourself](experiments/binary_waterfall.py).)

That refutation is what revealed the useful property: conversion **reliably
destroys history while preserving content**. Which is what a sanitizer needs,
and what makes the loss data worth publishing.

The negative results are kept in [`docs/adr/`](docs/adr/), not deleted. They are
the most useful thing in the repository.

## Contributing

Measurements are the contribution. **Contradicting an existing entry with a
measurement is the single most valuable thing you can send** — see
[CONTRIBUTING.md](CONTRIBUTING.md).

You do not need to know Rust. Adding a specimen and running a conversion is
enough.

## Layout

| Path | What's in it |
|---|---|
| [`dataset/`](dataset/) | **The deliverable.** CC0 data + schema docs. |
| [`registry/`](registry/) | Source data: formats, engines, measured conversions. |
| [`core/`](core/) | Rust: detector, planner, sanitizer, exporter. |
| [`docs/adr/`](docs/adr/) | 14 binding decisions, each with what would change our mind. |
| [`docs/RESEARCH.md`](docs/RESEARCH.md) | Evidence base — every finding has a source and an implication. |
| [`experiments/`](experiments/) | Runnable refutations. |
| [`corpus/`](corpus/) | Test specimens, generated by script rather than committed. |

## Licence

Code **MIT**. Data **CC0** — measurements of how software behaves are facts, and
facts shouldn't carry conditions.
