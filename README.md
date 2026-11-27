# Convert.To.Huh

An open dataset of what file-format conversions destroy, measured against real
files.

[PRONOM](https://www.nationalarchives.gov.uk/PRONOM/) identifies what a file
*is*. Nothing records what converting it *costs*. Convert a PNG to JPEG and
transparency is lost; convert a PDF to images and text stops being text;
convert WAV to MP3 and the top octave disappears. This project records those
losses, per engine and per invocation, as data.

**Dataset:** [`dataset/conversion-loss-registry.json`](dataset/conversion-loss-registry.json) · CC0

**20 formats · 8 engines · 34 conversions (33 verified)**

Images, PDF, audio, office documents and video. Coverage grows only by measurement.

## The data

Each entry is one conversion: source format, target format, engine, the exact
invocation, and a loss vector recording, property by property, whether it was
`preserved`, `degraded`, `lost` or `synthesized`.

```python
import json
d = json.load(open("dataset/conversion-loss-registry.json"))

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

All three lose transparency; they differ in what replaces it. Loss is recorded
against the invocation, not just the engine, because one flag changes the
result.

Every entry carries a verification status:

| Status | Meaning |
|---|---|
| `verified` | Run against a committed specimen and measured. |
| `claimed` | From documentation, not tested. |
| `refuted` | Tested and found not to hold. |

Schema and field reference: [`dataset/README.md`](dataset/README.md).

## The CLI

`huh` produces and checks the dataset. It also exposes two commands that use it
directly.

**`huh convert`** — converts a file and states what the conversion lost.

```console
$ huh convert logo.png logo.jpg
detected image/png (by signature, confidence 1.00)
route:   image/png --libvips[Transcode]--> image/jpeg
effect:  Loses transparency and metadata — transparent areas become black.
         Reduces colour depth and fine detail. Keeps colour accuracy and resolution.
```

Where engines lose a property in different ways, say which you want:
`--prefer alpha=composite_white` picks the route that turns transparency white.
`huh plan image/png image/jpeg -k 5` lists every distinct choice.

**`huh clean`** — rebuilds a file from its parsed content, discarding metadata,
hidden chunks, trailing data and active content, and reports what that cost.

```console
$ huh clean report.pdf clean.pdf
detected application/pdf (by signature, confidence 1.00)
strategy render-and-rebuild (1090 -> 9354 bytes, 82 ms)
  ran: pdftoppm -r 150 -png in.pdf page
  ran: vips jpegsave page-1.png page-1.jpg --Q 90

removed:
  - all PDF structure (1 page(s) rebuilt from pixels)
  - document metadata (title, author, producer, creation tool)
  - any /OpenAction, /JavaScript, /EmbeddedFile or annotation
properties destroyed: text_as_text, structure, interactivity, metadata, fonts
```

`huh clean` is a reference implementation for generating registry data. For
untrusted documents in practice, use [Dangerzone](https://dangerzone.rocks/).

**`huh export --out <file>`** — regenerates the dataset from `registry/`.

## Contributing

Measurements are the contribution, and a measurement that contradicts an
existing entry is the most valuable kind. No Rust required — a specimen and a
conversion run are enough. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Layout

| Path | Contents |
|---|---|
| [`dataset/`](dataset/) | The published dataset and its schema. |
| [`registry/`](registry/) | Source data: formats, engines, measured conversions. |
| [`corpus/`](corpus/) | Synthetic specimens (committed, reproducible via `corpus/generate*.py`) and a real-world tier of 206 Govdocs1 files, fetched by digest. |
| [`core/`](core/) | Rust: detection, planning, sanitization, export. |
| [`tools/`](tools/) | Validators and measurement harnesses, run in CI. |
| [`docs/`](docs/) | Architecture, decision records, research notes. |

## Licence

Code: MIT. Data: CC0.
