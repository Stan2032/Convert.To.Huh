# Research Dossier

Everything in this repository's design traces back to something in this file.
Findings are grouped by question. Each finding notes the source and — more
importantly — **what it implies for our design**. A finding without a design
implication is trivia and does not belong here.

Last substantive revision: 2026-08-20.

---

## 1. The reference product and the field

### 1.1 What convert.to.it actually is

Convert To It (`converttoit.com`) is a browser-based converter that performs
conversion **client-side via WebAssembly**, drawing on WASM ports of FFmpeg and
ImageMagick plus custom parsers for Office documents. Stated limits: 256 MB per
file, 100 files / 512 MB per batch. Files never leave the device during
conversion. It also exposes a free API.

The line that matters most, and the reason this repository exists:

> Unlike typical online tools that limit you to converting images to images or
> videos to videos, it lets you convert between **any** formats — even unusual
> combinations like AVI video to PDF.

**Implication.** The interesting product claim is not "we wrap FFmpeg." It is
"the set of legal conversions is not block-diagonal." Everyone can do JPEG→PNG;
almost nobody has a principled answer for AVI→PDF.

**Correction, after reading their source (§1.6): convert.to.it is not a lookup
table.** It already implements Dijkstra pathfinding over a format graph with a
tuned cost model. An earlier draft of this dossier asserted that the whole field
ships lookup tables; that was true of the commercial services and false of the
one product this project was named after. The real gap is narrower and is stated
honestly in §1.6.

### 1.2 The field, honestly assessed

| Product | Execution | Model | What it does well | Where it stops |
|---|---|---|---|---|
| CloudConvert | Server | Job/Task graph, per-engine version pinning, arbitrary FFmpeg/ImageMagick commands | Operational maturity; explicit multi-step jobs; engine choice (`office` vs `libreoffice`) | Route is caller-specified, not planned. No cross-domain semantics. |
| Convertio / FreeConvert / Online-Convert | Server | Format-pair lookup | Breadth, SEO surface area | Opaque quality; upload-everything privacy model |
| VERT.sh | Browser (WASM) + optional server for video | FFmpeg / libvips / Pandoc compiled to WASM, Svelte UI | Local-first, open source, ~250 formats | Video punted to a server; pair-wise conversion only |
| **convert.to.it** | Browser (WASM), fully static | ~50 WASM/JS engines; **Dijkstra pathfinding over a format graph** | Local-first, any→any, genuinely ships, offline build | Scalar cost model; no fidelity measurement; no intent selection |
| ConvertX | Self-hosted server | Orchestrates ~20+ CLI tools (FFmpeg, ImageMagick, GraphicsMagick, Vips, libheif, resvg, Potrace, VTracer, LibreOffice, Pandoc, XeLaTeX, Calibre, Inkscape, Assimp) | Enormous breadth from Docker; honest self-hosting | A dispatch table over binaries; no fidelity model |
| Stirling-PDF / BentoPDF | Self-hosted server | PDF-only toolkit | Depth in one format family | Deliberately narrow |

**Implication.** Two axes are already contested — *breadth* (ConvertX wins on
raw tool count) and *locality* (VERT/ConvertToIt win on privacy). The
uncontested axis is **route quality**: nobody plans, ranks, explains, or
measures the conversion. That is where we compete.

### 1.3 Prior art that matters: NCSA Polyglot

*Towards a Universal, Quantifiable, and Scalable File Format Converter* —
McHenry, Kooper and Bajcsy, NCSA / University of Illinois at Urbana-Champaign.
**Read from the primary source**, not a summary; the detail below is quoted or
closely paraphrased from the paper.

**The I/O-Graph.** Vertices are the union of all input and output formats
supported by the available applications. Edges are *directed*, each edge
representing one application that converts source format A to target format B.
**Parallel edges are allowed**, indicating multiple applications capable of the
same conversion. Their 3D example graph covers 17 applications and is "densely
packed with edges." Chained conversions — those no single application can do —
are found by searching for a path. Worked example: `.lwo → .3ds` via Blender,
then `.3ds → .stp` via Adobe 3D Reviewer.

**Weights, and how they are estimated.** Weighting edges by information retained
requires comparing content before and after conversion — which requires loading
every format, which as the authors note is circular: "if we could do this we
would not need to use 3rd party software to perform conversions, or for that
matter even need a universal converter."

Their escape from that circularity is the most useful idea in the paper. They
support a few formats natively, then convert **A → B → A** and score the
*round trip* against the original. That single score is assigned to every edge
along the path and **averaged over the different paths that traverse them** — an
approach they describe as "similar to Q-Learning." The claim is that a bad edge
penalizing an otherwise good path will wash out, because "there will likely be
other paths that use that edge and the average score should stabilize."

With weights in place they replace hop-count search with **Dijkstra's shortest
weighted path** to minimize information loss. Scores are distances, so they are
flipped to similarities by subtracting from the maximum.

**Results, and the caveat that matters.** Weights run 0–100 as average
information retention. The authors are explicit that these "do not indicate a
percentage of information preserved. They are **relative values comparable
within a particular measure**." And the measure changes the answer: under a
light-fields descriptor the best single conversion is Adobe 3D Reviewer between
`.pdf` and `.stp` (61.67) and the best target format is `.stp` (40.73); under a
spin-image measure it is `.obj → .pdf` (59.07) with `.stl` best (34.89).

**Scale.** A prototype scripting 7 applications, evaluated on 40 models (20
`.stp` from NARA's TWR-841 set, 20 `.obj` from the web). Parallelism scaled at
roughly 1/n: 20 `.stp` files to every reachable format took 33m06s on one
daemon, 16m40s on two, 11m40s on three.

**Implications for us — four of them.**

1. **Adopt the graph wholesale.** Directed edges, parallel edges for competing
   engines, path search for chained conversions. This is settled design, and it
   is ~15 years old.
2. **Adopt round-trip scoring as a bootstrap technique**, because we inherit the
   exact same circularity: we cannot independently parse every target format we
   claim to write. Where we *can* measure directly against a specimen, we do;
   where we cannot, A→B→A is the fallback. See `fidelity-analyst`.
3. **Reject their credit assignment.** Averaging one scalar across every edge on
   a path blurs blame — it cannot say *which* hop destroyed *what*, and the
   authors' own defence ("the average should stabilize") is a hope about graph
   density, not a guarantee. Per-property vectors measured per-edge give
   attributable loss. This is the direct motivation for ADR-0004.
4. **Their metric-dependence result is a warning, not a footnote.** Two
   reasonable measures produced different "best format" answers on the same
   data. A single fidelity number will silently encode a preference. Our loss
   vector makes that preference explicit and inspectable.

**What they did not do, and why it never shipped.** The quality measures
"consider only geometry" — no materials, textures or animation, though Table 1
acknowledges those exist. The work is demonstrated on 3D alone, so cross-domain
conversion never arises and `Synthesize`-class operations are not contemplated
at all. And automation was **AutoHotKey driving Windows GUI applications**,
chosen because high-end 3D tools ship GUI-only. That is a remarkable engineering
decision and a fatal one for productization: it is unsandboxable, unparallelizable
beyond whole machines, and breaks on any UI change. Our headless, sandboxed,
capability-declared engines are the direct answer to that failure mode.

### 1.4 Related theory: chaining lossy adapters

*A Mathematical Basis for the Chaining of Lossy Interface Adapters* and
*Probabilistic Analysis of Loss in Interface Adapter Chaining* (arXiv) give a
matrix formulation for composing loss across chained adapters, motivated by
exactly our situation: chaining lets you cover N² pairs with far fewer than N²
adapters, but you must be able to reason about accumulated loss.

**Implication.** Loss composition is a matrix product over property vectors,
not addition of scalars. This directly shapes `docs/specs/loss-vector.md`.

### 1.5 The N² argument, quantified

Connecting N entities pairwise requires on the order of N×(N−1) links; a
hub-and-spoke topology needs on the order of N. For a registry targeting even
1,000 formats, pairwise coverage is ~10⁶ converters — impossible. Hub-and-spoke
via pivot formats is ~10³ — tractable.

**Implication.** Pivots are not an optimization, they are the only viable
structure. But pure hub-and-spoke forces every conversion through a hub even
when a direct edge exists and is better (JPEG→PNG should not route through a
pivot). So: **a graph with pivots as high-degree nodes**, and a planner that
prefers direct edges when they exist. Not a hub-and-spoke mandate.

### 1.6 The reference implementation, read from source

`github.com/p2r3/convert`, GPL-2.0, ~50 contributors, a fully static site on
GitHub Pages with an Electron offline build. Read at commit `2fc7143`. This is
the closest existing thing to what this project proposes, and it is further
along than the earlier competitive summary implied.

**It is a planner.** `src/TraversionGraph.ts` builds a graph whose nodes are
formats (keyed `mime(format)`) and whose edges are handler-specific conversions,
then runs **Dijkstra's algorithm** over it via `src/PriorityQueue.ts`.
`searchPath` is an `AsyncGenerator` that **yields successive paths**, so the
caller can try the next-best route when one fails.

**Its cost function** (all constants theirs):

| Term | Value | Meaning |
|---|---|---|
| `DEPTH_COST` | 1 | per hop — prefers short paths |
| category change | table | `image→video` 0.2, `video→image` 0.4, `text→image` 0.5, `audio→image` 1.0, `document→text` 1.0, `image→audio` 1.4; 100 to encode "FFmpeg *can't* do this" |
| `LOSSY_COST_MULTIPLIER` | ×1.4 | applied when the target format is not `lossless` |
| `HANDLER_PRIORITY_COST` | 0.02×index | tie-break toward preferred engines |
| `FORMAT_PRIORITY_COST` | 0.05×index | tie-break toward preferred formats |
| `ANY_INPUT_COST` | +2 | penalty on `supportAnyInput` edges |

**Three ideas worth stealing outright.**

1. **Path-dependent adaptive costs.** `categoryAdaptiveCosts` penalizes category
   *sequences*, not individual edges: `text→image→audio` costs +15,
   `image→video→audio` and `audio→video→image` cost +10,000. This is how they
   suppress technically-valid nonsense routes. Note it makes edge cost depend on
   the path, which formally breaks Dijkstra's optimality precondition — they add
   it at enqueue time and accept the approximation. Pragmatic, and worth copying
   *with* that caveat recorded.
2. **Runtime dead-end learning.** When a hop fails during execution,
   `main.ts` records the failing path *prefix* via `addDeadEndPath`, which
   `calculateAdaptiveCost` then scores at `Infinity`. The generator's next path
   therefore routes around the observed failure. Plan → execute → learn →
   replan, within a single user request.
3. **A guaranteed-output escape hatch.** Handlers with `supportAnyInput` get
   edges from *every* node, penalized by `ANY_INPUT_COST` so they are chosen
   last. This is what backs their promise that you are "almost guaranteed to get
   an output."

**Where this project still differs — narrower than claimed, but real.**

- **Loss is `lossless?: boolean` plus hand-tuned scalars.** The system cannot say
  *what* a route costs you, only that it costs 3.47. Those constants (0.2, 1.4,
  10000) are unexplainable to a user and uncalibratable against measurement.
  Our per-property vector exists to make exactly that explicable (ADR-0004).
- **No fidelity measurement anywhere.** Capabilities are discovered at runtime by
  probing handlers and cached to `cache.json`; nothing ever checks that a
  conversion *preserved* anything. There is no SSIM/VMAF loop (§5.4).
- **No intent selection.** The generator's yield order decides; the user is not
  offered the contact-sheet-vs-transcript choice. Their README concedes the
  consequence: "perhaps not always the one you expected."
- **No synthesis marking.** eSpeak text→audio is just another edge at cost 0.6.
  Nothing distinguishes inferred content from converted content (ADR-0009).

**Two philosophies, both defensible.** Theirs is *optimistic*: claim broadly, try
it, fall back, always return something. Ours is *pessimistic*: verify before
trusting, refuse rather than surprise, explain the cost. Theirs ships and
delights; ours is auditable. The synthesis worth building is to let runtime
dead-end evidence feed the verification loop — a hop that dead-ends repeatedly in
production is exactly the evidence needed to mark an edge `refuted`.

**Engine list worth noting**, because it is the practical answer to "what is
actually available as WASM in 2026": `@ffmpeg/core`, `@imagemagick/magick-wasm`,
`7z-wasm`, `@bokuweb/zstd-wasm`, `@sqlite.org/sqlite-wasm`, `typst.ts`
(compiler + renderer), `@yowasp/clang`, `wabt`, `@bjorn3/browser_wasi_shim`,
plus `pdf-parse`, `pdftoimg-js`, `opentype.js`, `woff2-encoder`, `three`,
`imagetracer`, `verovio`/`vexflow`/`js-synthesizer` for music, and a long tail of
deliberately niche parsers (`chess.js`, `nbtify` for Minecraft, `ts-flp` for FL
Studio, `turbowarp-packager` for Scratch, `pe-library` for Windows binaries).

**Typst is the find here.** It compiles *and* renders in WASM, which makes it a
better browser answer for `DocTree → PageLayout` than the LaTeX toolchain this
project had assumed (§3.3).

**And their niche long tail answers §10.5 empirically.** Converting Minecraft NBT
or an FL Studio project is exactly the "how far does any go?" question — and the
answer in the field is "very far, and it is the reason people share the tool."
That is evidence, not proof, but it shifts the prior.

### 1.8 Empirical check of this project's thesis

The thesis — *cross-domain conversions are underspecified, not absurd, and the
user should choose the interpretation* — was asserted before it was tested.
Tested against the reference implementation at commit `2fc7143`, four findings.

**Confirmed: interpretation mismatch is a real, recognised bug class.** Not
inferred — it is written into their contributor policy:

> "converting X to Y doesn't work" is **not** a bug report. However,
> "converting X to Y works **but not how I expected**" likely **is** a bug report.

A project only writes that rule after enough people hit it. Their front page
concedes the same thing: you are "almost guaranteed to get an output — perhaps
not always the one you expected."

**Confirmed: one interpretation is chosen silently.** Their own graph test is
titled *"should find the optimal path from image to audio"* — a cross-domain
conversion — and asserts on `extractedPaths[0]`, the single best path. There is
no interpretation picker in the UI.

**Correction to an earlier claim of mine.** I wrote that they "pick one path
rather than presenting alternatives," which understated their design. They *do*
generate ranked alternatives — `searchPath` is an `AsyncGenerator` yielding
successive paths. But the only consumer is the **failure-retry loop** in
`main.ts`: alternatives exist to route around a hop that just crashed, never to
offer the user a different reading of their request. The machinery is there; it
is pointed at reliability rather than intent. That is a meaningfully different
statement, and the distinction matters because it means our Phase 2 work is
*re-purposing* proven machinery, not inventing it.

**Convergent evidence for the content model.** Their contributor guide arrives
independently at the distinction this project builds on:

> "treat the file as the **media that it represents**, not the data that it
> contains. For example, if you were making an SVG handler, you should treat the
> file as an *image*, not as XML. In other words, avoid simple 'binary
> waterfalls', as they're not semantically meaningful."

That is `docs/specs/content-model.md §1` — formats are containers, content kinds
are what travels inside — reached by a different route. Two independent
derivations is the strongest support the content model has.

**What remains genuinely contested.** They treat "not how I expected" as a bug
to be fixed *by improving the single default*. This project treats it as
evidence the request was underspecified and the user should pick. Both are
coherent strategies, and theirs is cheaper and has less decision fatigue. Ours
is only better if users actually want the choice — which is §10.4's open
question, still unanswered, and the thing Phase 3 has to test with real users
rather than argument. Offering four options where one good default would do is a
real failure mode, not a hypothetical one.

### 1.7 Distribution is part of the threat model

Their `fail.html` — served from the vanity domain when the GitHub Pages app
cannot be reached — reports that **ISPs have begun blanket-blocking file
converter sites**, and asks users to request manual review, enable DNS-over-HTTPS,
use a VPN, or download the offline build.

We hit this ourselves: `convert.to.it` and `p2r3.github.io` are both blocked by
this environment's egress proxy (§11).

**Implication.** A local-first converter that cannot be *loaded* is as useless as
a server-side one that is down, so reachability belongs in the design, not in
marketing. Concretely: ship an installable/offline build (they use Electron;
a PWA with a service worker is the lighter option), do not depend on a single
domain, and expect category-based blocking rather than reputation-based — being
open source and privacy-preserving does not exempt you, and may not even be
legible to whoever maintains the blocklist.

This also *sharpens* the local-first argument in an unexpected direction: an
offline-capable converter survives an ISP block, and a hosted one does not.

---

---

## 2. Format identification

### 2.1 The extension is not the format

WHATWG's MIME Sniffing Standard exists because servers routinely lie in
`Content-Type`. Browsers sniff content to compensate, and that sniffing is
itself an attack surface — HTML smuggled inside a `.jpg` becomes stored XSS
when a browser sniffs it as HTML. `X-Content-Type-Options: nosniff` exists to
turn the compensation off.

**Implication.** Two hard rules. (a) Detection is content-first; the extension
is a *hint* used only to break ties. (b) Everything we serve back to a browser
ships `nosniff` plus a `Content-Disposition: attachment` default, because we
are, by construction, a service that hands users files derived from
attacker-supplied bytes.

### 2.2 Signature-based identification: PRONOM / Siegfried

PRONOM (UK National Archives) is the reference technical registry of file
formats, assigning each a persistent unique identifier (PUID). Siegfried
implements PRONOM signatures plus freedesktop.org MIME-info and Library of
Congress FDD signatures, matches them concurrently, applies PRONOM priority
rules during matching rather than after, and reads only as much of the file as
needed.

**PRONOM has a machine-readable API**, which makes this operational rather than
aspirational: `https://www.nationalarchives.gov.uk/PRONOM/{puid}.xml` returns
format name, version, PUIDs, MIME types and full internal signatures.
`tools/verify_pronom.py` uses it to check every entry in our registry.

**Verified 2026-08-20:** all 8 seed PUIDs are correct. Two findings came out of
doing it properly rather than trusting recall.

**(a) PRONOM identifies format *versions*; we identify *formats*.** `fmt/13` is
PNG **1.2**, `fmt/43` is JFIF **1.01**, `fmt/92` is SVG **1.1**, `fmt/276` is
PDF **1.7**. Their signatures are correspondingly narrow — fmt/43 requires an
APP0 segment containing "JFIF", so it does *not* match Exif JPEG, which carries
a different PUID. Our signatures are deliberately broader, because a converter
needs to know "this is a JPEG", not "this is JFIF 1.01". The `puid` field
therefore names a *representative version*, and saying so explicitly is the
difference between a useful cross-reference and a false equivalence.

**(b) PRONOM's signature syntax is strictly more expressive than ours.** It has
wildcards (`{4}`, `{14-*}`), bounded ranges (`{43-65531}`), alternation
(`(46|460A|460D)`), arbitrary gaps (`*`) and **EOF anchoring**. Three concrete
consequences: PRONOM detects a truncated PNG via an EOF anchor on the `IEND`
chunk and we cannot; PRONOM identifies ZIP by locating the *central directory*
rather than the local file header — which is the structurally correct approach
and notably the part a zip bomb manipulates (§4.3); and PRONOM confirms
`text/plain` has **no internal signature at all**, independently corroborating
our UNDETECTABLE classification.

**Implication.** We do not invent format identity. Every registry entry carries
its PUID where one exists, so our identifiers are joinable with the digital
preservation world — and verification is a repeatable command, not a promise.
Siegfried's *priority rules* concept (format A's signature beats format B's when
both match — every DOCX is also a ZIP) is adopted directly; see
`docs/specs/registry-schema.md`. Closing the signature-expressiveness gap means
extending the detector first and the schema second; until then
`verify_pronom.py` reports which entries are affected so it stays visible.

### 2.3 ML-based identification: Magika

Google's Magika uses a compact deep-learning model over ~200+ content types,
trained on ~100M samples, achieving ~99% precision/recall with ~5 ms inference
on a single CPU — a double-digit absolute accuracy improvement over libmagic,
which relies on brittle handcrafted magic-number rules and struggles
particularly with textual formats (Python vs JavaScript vs JSON). Published at
ICSE 2025; Magika 1.0 is open source.

**Implication.** Layered detection (`docs/specs/registry-schema.md §Detection`):
container/magic signatures decide when they are decisive (they are precise and
auditable); an ML classifier arbitrates the textual and ambiguous cases where
signatures are structurally incapable. The ML tier is **advisory** — it can
raise confidence and break ties but may never override a decisive structural
parse, because we must be able to explain to a user why we thought their file
was what we thought it was.

---

## 3. Engines: what actually exists, and what it costs

### 3.1 Images: libvips over ImageMagick

Benchmarks consistently favour libvips: ~4× on JPEG single-threaded and ~8×
multi-threaded in one production evaluation, up to ~5.5× overall elsewhere, and
dramatically better memory behaviour — **~200 MB vs ~3 GB** on the same
variant-generation workload. libvips scales across cores; ImageMagick's
concurrency is weaker and PNG operations bottleneck. Counter-evidence exists:
at least one report of libvips being slower and heavier on *small* images
(144 KB–10 MB resize), so the win is size-dependent.

**Implication.** libvips is the default raster engine. ImageMagick is retained
only for formats libvips does not cover, and when retained it runs under the
hardened policy of §4.1. The small-image counter-case means the engine choice
is a *planner* decision informed by measured input size, not a global constant.

### 3.2 Documents: LibreOffice is powerful and fragile

LibreOffice headless is the only realistic engine for the Office family, but
operationally it is the worst citizen in the fleet. Reported behaviour: random
crashes during large batches (60k documents) with fatal exceptions in core
libraries; corrupted documents causing hangs at 100% CPU; `unoserver`
deliberately **does not restart LibreOffice on crash** — it exits and expects a
supervisor to restart it; and its `--conversion-timeout` kills LibreOffice,
which also takes unoserver down. Listener mode (vs cold-starting the binary per
document) cuts CPU load 50–75%, worth 2–4× throughput.

**Implication.** LibreOffice gets its own pool with its own supervisor,
one-document-at-a-time per instance, an aggressive wall-clock kill, and
crash-as-expected-outcome semantics. It never shares a worker with other
engines. It is the archetype for the "engines are untrusted and unreliable"
stance in `docs/adr/0003-engines-are-hostile.md`.

### 3.3 Documents: Pandoc's AST is the model to copy

Pandoc parses input with a *reader* into a format-agnostic **AST**, then a
*writer* emits the target. That single intermediate representation is what lets
one tool span 40+ input and output formats without N² code. Filters operate on
a JSON serialization of the AST between reader and writer, in any language.

**Implication.** This is the pivot-format thesis proven at scale in one domain.
Our `DocTree` content kind is explicitly Pandoc-AST-shaped, and the
reader/filter/writer split is the shape of our own pipeline stages. Where
possible we should *use* Pandoc as the DocTree engine rather than reinvent it.

### 3.4 PDF: four tools, four jobs

Ghostscript is the general converter/compressor; qpdf does structural
manipulation; Poppler (`pdftocairo`, `pdftotext`) underpins most Linux viewers
and is a stable repair path; mutool is faster but produces subtly different
output. PDF/A (preservation) and PDF/UA (accessibility) are ISO profiles;
veraPDF validates both.

**Implication.** "PDF" is not one capability. The registry must model PDF
*profiles* (PDF/A-2b, PDF/UA-1, tagged vs untagged) as distinct targets with
distinct achievable-by edges, and route accordingly. `pdftocairo` and `qpdf`
converging on similar output is a useful **normalization** step before
comparison in the fidelity harness (§6).

### 3.5 3D: Assimp's honest limits

Assimp loads 40+ formats into one unified structure — itself an I/O-Graph
argument. But export is narrower than import, and the docs mark glTF 1.0/2.0
export **partial**, FBX and 3MF export **experimental**. Basic imports may drop
animations and advanced features; materials may load while textures need
separate handling.

**Implication.** Import breadth ≠ export breadth ≠ fidelity. Capability claims
must be recorded per-direction and empirically verified, never copied from a
README. This is the founding justification for the `capability-cartographer`
agent.

### 3.6 Cross-modal engines make "Huh" conversions real

whisper.cpp is a dependency-light C/C++ Whisper implementation, integrated into
FFmpeg itself so transcription is a single shell command — but it consumes only
standard `.wav`, so FFmpeg must normalize first. Tesseract covers the raster→text
direction.

**Implication.** These are the *synthesis* edges of §5.2. They are what turn
"AVI → PDF" from a joke into a plan: demux audio → normalize to WAV → ASR →
DocTree → typeset → PDF. Note the mandatory normalization hop; the planner must
model it rather than assume engines accept arbitrary input.

---

## 4. Security: we are a remote code execution service unless we prove otherwise

This is the section that should scare us most. Our entire product is "accept a
hostile binary from a stranger and feed it to a C parser."

### 4.1 ImageMagick is a delegation engine, and delegation is the bug

CVE-2016-3714 ("ImageTragick") was shell injection: ImageMagick built shell
command strings incorporating filenames/parameters and executed them without
sanitizing, reachable through the MVG and HTTPS coders. Hardening guidance is
to disable MVG, HTTPS, HTTP, URL, FTP, EPHEMERAL, MSL, LABEL and TEXT coders in
`policy.xml`.

The structural problem is worse than any single CVE: **ImageMagick ships an
unrestricted default policy that delegates PostScript, EPS, PDF and XPS to
Ghostscript.** Recent research demonstrates a single uploaded `.jpg` or `.pdf`
achieving file writes on default installs, with bypasses defeating the default
policy, the recommended "limited" policy, **and the "secure" policy** in some
configurations.

**Implication.** Three rules. (a) The policy file is necessary and *not
sufficient* — it is defence in depth, never the boundary. (b) Delegation is
severed: engines run with no delegate binaries reachable, so ImageMagick
physically cannot invoke Ghostscript; if we want PDF handling we schedule an
explicit, separately-sandboxed Ghostscript hop that the planner can see and
reason about. (c) An engine's own security configuration is treated as
untrusted input to our threat model.

### 4.2 Ghostscript's sandbox has repeatedly not been one

CVE-2018-19475 (`-dSAFER` bypass), CVE-2023-36664 (CVSS 9.8 RCE),
CVE-2024-29510 (format string, **exploited in the wild** via EPS files disguised
as JPEGs) — a sustained pattern of escapes from the mechanism that is supposed
to contain it.

**Implication.** Ghostscript is classified `hostility: extreme` in its engine
manifest and is only ever scheduled inside the strongest available isolation
tier (§4.4). The "EPS disguised as JPEG" case is also the sharpest possible
argument for content-based detection (§2.1): trusting the extension here is
directly exploitable.

### 4.3 Bombs: compression and pixels

Modern zip bombs reach 281 TB from a 10 MB payload, and ~4.5 PB with Zip64,
using overlapping entries and DEFLATE quoting to exceed 28,000,000:1 ratios.
Recommended defences: compression-ratio limits at the gateway, memory caps,
isolated decompression with hard CPU/memory/disk quotas (systemd-run, container
limits) that can kill runaway work, and alerting when compressed:uncompressed
ratio exceeds ~10,000:1. The same class of attack exists for images — a small
file declaring an enormous pixel grid (Pillow ships an explicit decompression
bomb guard for this).

**Implication.** Every plan carries a declared **output budget** — bytes,
pixels, duration, entry count, recursion depth — computed *before* execution
from the declared header values, and enforced during execution by the sandbox.
Exceeding budget is a normal, user-visible outcome ("this file expands to 4 PB;
refusing"), not a crash. Archive recursion depth is capped, and nested archives
are re-budgeted at each level.

### 4.4 Isolation tiers

Established 2026 positioning: containers alone are not an isolation boundary
for hostile code; **gVisor** intercepts and reimplements syscalls in user space,
giving stronger-than-container isolation at 10–30% overhead on I/O-heavy work
and near-zero on compute-heavy work, with occasional syscall-compatibility
gaps; **Firecracker** microVMs give each workload its own kernel on KVM, boot in
~125 ms with <5 MiB overhead, and are the current gold standard for untrusted
code because a guest kernel exploit cannot reach the host. Guidance: gVisor for
medium-threat multi-tenant SaaS, Firecracker/Kata for high-threat untrusted
execution. WebAssembly isolates at a finer granularity and is strong for
stateless, portable tasks.

**Implication.** We define three tiers and assign every engine to one in its
manifest — WASM for pure-computation engines we can compile ourselves, gVisor
as the default for well-maintained native engines, Firecracker for
`hostility: extreme` (Ghostscript, LibreOffice, anything parsing EPS/PS). The
tier is a property of the *engine*, declared in data, enforced by the executor.
Compute-heavy conversion is precisely the workload where gVisor's overhead is
cheapest, which makes the default affordable.

### 4.5 The network is not needed

ImageTragick's HTTPS/URL coders, FFmpeg's HLS/protocol handling, and XXE in
XML-based formats (SVG, DOCX, ODF) all share one precondition: the converter
can reach the network.

**Implication.** Conversion sandboxes get **no network namespace at all**. Not
a proxy, not an allowlist — no interface. Any format feature requiring remote
fetch (external entities, remote stylesheets, HLS segments) is unsupported by
construction and must be reported to the user as a refused feature.

---

## 5. The thesis: any→any needs a type system, not a bigger lookup table

This section is our own synthesis. It is the part of the design that is not
borrowed.

### 5.1 Content kinds

Formats are containers; what travels inside them is a **content kind**:
`Raster`, `Vector`, `AudioPCM`, `VideoStream`, `Text`, `DocTree`, `PageLayout`,
`Table`, `Mesh3D`, `Container`, `Bytes`. A format declares which kinds it can
carry (PDF carries `PageLayout`, usually `Text`, often `Raster` and `Vector`;
MP4 carries `VideoStream` + `AudioPCM` + optional `Text` as subtitles).

Conversion is then not "format → format" but a path through kinds, where each
hop is one of six operation classes:

| Class | Meaning | Example | Information |
|---|---|---|---|
| **Transcode** | same kind, different encoding | JPEG → PNG | preserved or lossily re-encoded |
| **Transform** | same kind, different parameters | 4K → 1080p | deliberately reduced |
| **Project** | kind A → kind B, defined loss | SVG → PNG (rasterize) | structurally lost |
| **Extract** | pull a sub-kind out of a composite | PDF → Text; MP4 → AudioPCM | subset retained |
| **Assemble** | many → one | frames → contact sheet | composed |
| **Synthesize** | kind A → kind B by **inference** | audio → text (ASR); image → text (OCR); text → audio (TTS) | *estimated, not preserved* |

**Synthesize is the category everyone else is missing.** It is the honest name
for the hops that make AVI→PDF possible, and it is the one class where the
output is a *guess* about the input. It must be visually and structurally
distinct everywhere — in the plan, in the UI, in the receipt — because a user
who receives a transcript needs to know a model wrote it.

### 5.2 "AVI → PDF" is not one conversion, it is a question

Given AVI→PDF, at least four plans are defensible:

1. **Contact sheet** — sample frames at 1 fps, tile onto pages. (Extract →
   Assemble → Project)
2. **Frame-per-page** — every keyframe becomes a page. (Extract → Project)
3. **Transcript** — demux audio, normalize, ASR, typeset as a document.
   (Extract → Synthesize → Project)
4. **Storyboard** — scene-detect, one representative frame per scene with
   timecodes. (Extract → Transform → Assemble → Project)

**Implication.** The planner returns *ranked alternatives with plain-language
descriptions*, and the UI asks. "Huh?" is the correct user reaction to the
request, and resolving it is the product. This is the single most important
interaction design decision in the project.

### 5.3 Loss is a vector

A scalar "quality: 0.8" cannot express "kept every pixel, destroyed all text
selectability." We track loss per **preservable property** — geometry, color
depth, color profile, alpha, animation, layers, text-as-text, semantic
structure, metadata, embedded fonts, interactivity, audio channels, sample
rate, bit depth, provenance — and compose along the chain per the adapter-
chaining matrix formulation (§1.4). Route explanations are then generated, not
written: *"loses transparency and text selectability; preserves color profile."*

### 5.4 Fidelity must be measured, not asserted

VMAF correlates with human MOS at ~0.93–0.96 Pearson versus ~0.6–0.7 for PSNR;
SSIM decomposes similarity into luminance, contrast and structure. These are
full-reference metrics — they need the original, which we have.

**Implication.** Declared loss vectors are *hypotheses* that a harness checks
against measured reality on the corpus (VMAF/SSIM for visual, structural diff
for DocTree, text diff for Text). A route whose measured fidelity contradicts
its declared loss vector is a **bug in the registry**, and it is detectable
automatically. This closes the loop that the Polyglot work left open, and is
why `fidelity-analyst` and `corpus-keeper` exist as first-class roles.

---

## 6. Execution: browser, server, or both

### 6.1 WASM is viable but not fast

ffmpeg.wasm's multithreaded core (`@ffmpeg/core-mt`, ~32 MB) is ~2× the
single-threaded build but requires `SharedArrayBuffer`, which requires
cross-origin isolation (`COOP: same-origin` + `COEP: require-corp`) — headers
that break iframe embeds, some third-party scripts and some CDN setups. The MT
build is described upstream as unstable; iOS Safari lacks `SharedArrayBuffer`
in workers as of iOS 17; most production users stay single-threaded and accept
the ceiling. ffmpeg.wasm is explicitly not as fast as native FFmpeg.

### 6.2 WebCodecs is the fast path, with narrow codec coverage

WebCodecs exposes the browser's hardware encoders/decoders. Reported gaps:
3–10× faster than ffmpeg.wasm for H.264; ~200 fps vs ~25 fps on 1080p H.264
encode on identical hardware. The reason is structural — ffmpeg.wasm copies
into linear memory, bounds-checks every pixel access, and **no stage can reach a
GPU encoder**. But WebCodecs guarantees only H.264 and VP9 broadly; AV1 encode
is patchy, HEVC encode essentially Safari-only. Common production pattern: both
— WebCodecs for playback/scrubbing, ffmpeg.wasm for final encode and format
coverage.

### 6.3 Memory and storage ceilings in the browser

wasm32 caps linear memory at 4 GB by construction. Memory64 ships by default in
recent Chromium and Firefox but carries a performance penalty from bounds
checks that wasm32 avoided by reserving the full address space. `ArrayBuffer`s
held on the JS side do not count against the wasm limit. OPFS provides a real,
sandboxed, streaming-capable filesystem with byte-level access; quotas
typically range from ~300 MB to several GB, WebKit allows up to ~60% of disk
per origin, and iOS is aggressive about eviction with `persist()` hard to
obtain.

**Implication.** A local-first hybrid is correct, and the split is *computable
in advance*, not a coin flip. The planner estimates peak memory and wall time
per plan and picks a runtime profile: browser when it fits inside wasm and
quota budgets, server when it does not, with an explicit user-visible
"this one has to leave your device — proceed?" boundary. Because the planner is
the same code in both places (§7), the estimate and the plan cannot drift.
VERT's decision to punt video to a server is the empirically-validated version
of exactly this trade-off.

### 6.4 WASI and the component model

WASI Preview 2 stabilized in early 2026 with a standard interface to files,
networking, HTTP and clocks outside the browser; the Component Model plus WIT
lets one binary run on Wasmtime, WasmEdge, Node, browsers or the edge, with
strict deny-by-default sandboxing. Server-side Wasm has overtaken browser-only
usage — 52% of production deployments target non-browser environments.

**Implication.** This is the strongest available support for our central
architectural bet: **compile the core once, run it in both places.** It also
makes WASM a legitimate *isolation tier* for engines we compile ourselves
(§4.4), not merely a browser delivery mechanism.

---

## 7. Orchestration

### 7.1 Queues vs durable execution

Queues (BullMQ on Redis) guarantee delivery to a worker; what happens *inside*
the job is outside the queue's contract, and long-running jobs need careful
heartbeating because stalled-job detection is timeout-based — exceed
`stalledInterval` without progress and the job is failed. Durable execution
(Temporal) persists every step to an event history and resumes exactly where it
stopped after a crash. The stated dividing line: use a queue when work is
atomic, independently retryable and safe to restart from scratch; the moment
you have chained steps and compensation, you are building a workflow engine on
top of a queue, badly.

**Implication.** A conversion plan is *by definition* chained steps with
expensive intermediates. That is the textbook durable-execution case. But
adopting Temporal on day one is disproportionate, so: model plan execution as
an explicit state machine over persisted per-hop artifacts from the start — so
that the durable-execution semantics are ours regardless of the runtime — and
keep the queue swappable. Long hops emit heartbeats. See
`docs/adr/0008-plan-execution-state-machine.md`.

### 7.2 Upload

S3 multipart with presigned URLs is the industry standard at ≥100 MB and keeps
the server out of the data path; its weakness is that presigned URLs expire, so
resuming after a browser refresh requires re-minting them. tus is a
standardized resumable protocol (POST to create, PATCH with `Upload-Offset`,
HEAD to discover offset after failure); Supabase implemented tus rather than
exposing S3 directly, supporting 50 GB files.

**Implication.** Local-first means many conversions never upload at all — the
upload path only exists for the server-profile overflow. Presigned multipart is
the default; a tus front door is the escape hatch for genuinely large media and
flaky networks. Either way, uploaded bytes are content-addressed so a repeat
conversion of the same input is free.

---

## 8. Licensing and patents

FFmpeg is LGPL 2.1+ by default, which permits closed-source commercial use;
passing `--enable-gpl` or `--enable-nonfree` changes that. **libx264 is GPL** —
linking it propagates GPL obligations to the distributed product. Separately
and independently of licensing, H.264/H.265 carry **patent** obligations
(MPEG-LA), which apply regardless of the FFmpeg license path taken.

**Verified 2026-08-20** against the actual license files of each engine, not
from memory. Four confirmed, two corrected.

| Engine | SPDX | Source | Result |
|---|---|---|---|
| FFmpeg | LGPL-2.1-or-later | ffmpeg.org/legal.html | confirmed |
| libvips | LGPL-2.1-or-later | `LICENSE`, includes "any later version" | confirmed |
| whisper.cpp | MIT | `LICENSE` | confirmed — **but model weights are licensed separately and are not covered** |
| Ghostscript | AGPL-3.0-or-later | `LICENSE` ("GNU Affero General Public License") | confirmed |
| LibreOffice | MPL-2.0 **+ LGPL-3.0 + GPL-3.0** | `COPYING.MPL`, `COPYING.LGPL`, `COPYING` | **corrected** |

**Correction 1 — a single SPDX string cannot describe a multi-licensed engine.**
LibreOffice core ships three license files: `COPYING.MPL` (the primary grant),
`COPYING.LGPL`, and `COPYING`, which is **GPL-3.0**. Our manifest claimed a bare
`MPL-2.0`, and that omission produced a wrong `build_profiles = ["lgpl", "gpl"]`.
Corrected to `["gpl"]` only, with an `spdx_components` field added to the schema.
Because the planner prunes ineligible engines before search, the consequence is
concrete: **no Office-format route exists in the LGPL profile** until someone
does a component-level audit. That is the correct conservative default — a wrong
eligibility value produces routes we must not ship.

**Correction 2 — a stale version pin is a licensing risk, not just an ops one.**
We pinned FFmpeg at `>=7.1,<8` while 9.0.1, 8.1.2 and 8.0.3 were the shipping
releases: the pin excluded every current version. Licenses and GPL-dependency
sets change between major versions, so re-checking on bump is part of the
licensing obligation. A pin nobody has built against is a guess with a version
number attached.

**Implication.** Build profiles are a first-class concept, not a Dockerfile
detail: an LGPL profile (safe to distribute in a proprietary product) and a GPL
profile (self-host only). Every engine manifest declares its license and its
profile eligibility, and the planner will not route through an engine excluded
from the active profile — meaning a legally-unavailable route simply does not
exist rather than producing a runtime failure. Patent exposure is tracked
separately from license because they are separate obligations that are
routinely conflated.

---

## 9. Testing

Coverage-guided fuzzing (libFuzzer, AFL, OSS-Fuzz) is the established practice
for file-format parsers, and seed corpora "work especially well for strictly
defined file formats" — the guidance is explicitly to seed from valid files in
your test suite. The central difficulty is that complex formats reject
non-compliant input early, so naive mutation yields shallow coverage. Fuzz
corpora double as regression suites.

**Implication.** The corpus is infrastructure, not test fixtures — hence a
dedicated owning role. Every format entry needs at least one real specimen
before its capability claims are trusted; specimens are the seed corpus for
fuzzing *and* the reference inputs for fidelity measurement (§5.4). Specimen
licensing must be tracked, because a corpus of scraped media is a legal
liability that surfaces at the worst possible time.

---

## 10. Open questions

These are unresolved and deliberately recorded as unresolved. Each is cited
elsewhere by number, so they are subsections rather than a list.

### 10.1 Cost model calibration

Loss vectors start as expert judgement. How many corpus specimens per edge are
needed before measured fidelity can replace declared values? Unknown;
`fidelity-analyst` should answer empirically. Blocks confident ranking.

### 10.2 Synthesis provenance

If ASR produces a transcript, the output contains model-generated text. What is
the right *durable* marker — XMP metadata, a visible cover page, both? The
in-product marking is settled (ADR-0009); the marking that survives the file
leaving our hands is not. Legal review needed before any synthesis edge ships.

### 10.3 Plan stability across registry versions

Users bookmark and re-run conversions. Does the same input and target always
produce the same plan? A registry update that changes a route silently changes
someone's output. Pinning the registry version in the receipt is necessary but
probably not sufficient — we may need a stability policy for edges the planner
already chose.

### 10.4 Browser/server boundary consent

§6.3 requires asking the user before their file leaves the device. Does asking
mid-flow destroy the experience? An up-front mode choice ("private" vs "fast")
is the alternative. Needs user testing, not argument.

### 10.5 How far does "any" go?

Is `XLSX → MP3` (sonify the numbers) a legitimate route, or a joke that damages
trust in everything else the product claims? There is a line between delightful
and absurd and we do not know where it is. Owned by `semantic-bridge-designer`.

**Evidence has arrived, and it points one way** (§1.6). convert.to.it ships
parsers for Minecraft NBT, FL Studio projects, Scratch, chess positions and
Windows PE binaries, and that long tail is a large part of why people share it.
The prior should shift toward "further than you think." What remains genuinely
open is whether the *presentation* can distinguish a delightful novelty from a
result the user might mistake for authoritative — which is ADR-0009's problem,
not a reason to withhold the route.

---

## 11. Research access notes

Recorded because the next person will hit the same wall.

**Two separate allowlists exist.** `curl` (via the egress proxy) and the
`WebFetch` tool are gated independently. `www.archives.gov` is reachable by
`curl` and *blocked* for `WebFetch`. So when a domain is whitelisted, reach for
`curl` first; if the content is a PDF, fetch it and extract locally.

**Extracting a PDF here:** no `pdftotext`, `mutool`, `gs` or `qpdf` is
installed, and `pip install pypdf` succeeds but then panics on import because
the system `cryptography` module is broken. The workaround is to block
`cryptography` on `sys.meta_path` so pypdf falls back to its no-crypto provider;
that is only needed for encrypted PDFs. This is how §1.3 was read from source.

**Reachable:** `raw.githubusercontent.com`, `github.com`, PyPI, npm, crates.io,
`convert.to.it` (302 → `p2r3.com/convert`, itself blocked), `www.archives.gov`.

**Blocked and worth requesting**, in priority order:

| Domain | Unblocks |
|---|---|
| `p2r3.com` | The reference product itself. `convert.to.it` redirects here. |
| `pronom.nationalarchives.gov.uk`, `www.nationalarchives.gov.uk` | The 8 PUIDs flagged `puid_verified = false`, and canonical signatures |
| `www.iana.org` | The media-type registry — canonical MIME names |
| `www.loc.gov` | Library of Congress format descriptions (FDD) |
| `nvd.nist.gov`, `www.cve.org` | `sandbox-warden`'s standing CVE watch on pinned engines |
| `www.w3.org`, `developer.mozilla.org` | Format specs; browser API support data |
| `ffmpeg.org`, `www.libvips.org`, `pandoc.org`, `imagemagick.org`, `ghostscript.com` | Engine capability matrices, verified rather than recalled |

`WebSearch` works without a whitelist and returns summaries, which is how most
of this dossier was built. It is adequate for orientation and **not** adequate
for the kind of claim that ends up in a registry entry — which is exactly why
every PUID here carries `puid_verified = false`.


## 12. Content Disarm and Reconstruction

The direction ADR-0010 pivots to. Recorded after the cipher premise was refuted
by measurement, so this section post-dates most of the dossier.

### 12.1 What CDR is, and why conversion already implements it

CDR does not scan for threats — it assumes it cannot recognise them. Instead it
**parses a file to its content and writes a fresh container**, so anything that
was not part of the content simply does not make the trip. No signature
database, no heuristics, no arms race.

That is exactly what `path_channel.rs` measured: seven lossless routes, one
byte-identical output. The property that made the path useless as key material
is precisely the property CDR needs.

**Design implication:** the sanitizer is not a new subsystem. It is the existing
decode/encode pipeline with the target format pinned to the source format, plus
a report. `core/src/sanitize.rs` is ~200 lines because the machinery already
existed.

### 12.2 Where a passenger hides in a perfectly valid file

Measured against `corpus/raster/png-with-passengers.png`:

| Vector | Bytes | Why it survives ordinary handling |
|---|---|---|
| `tEXt` / `zTXt` / `iTXt` chunks | 32, 54 | Free-form metadata. Rendered by nothing, copied by everything. |
| Private ancillary chunk (`prVt`) | 94 | PNG's case convention makes unrecognised lowercase chunks *skippable by design*. A decoder that ignores it is spec-compliant. |
| Data appended after `IEND` | 264 | Structurally invisible. The file is bit-for-bit valid to every viewer. |

The third is the one worth internalising: **a file can be entirely valid and
still carry an arbitrary payload.** Nothing is malformed. Every decoder is
behaving correctly. This is why validation is not sanitization — a validator
would pass this file, because it *is* valid.

Sanitizing it removes 444 bytes across 4 passengers at SSIM 1.0000.

### 12.3 The documented weaknesses of existing CDR — two of which we answer

From vendor and critic sources (OPSWAT, Deep Instinct, Fortinet, Wikipedia):

1. **Images are largely uncovered.** CDR is built around documents; the
   commonly cited gap is that it "cannot sanitize portable executable files and
   most image files," which is called out as a source of false confidence.
2. **Sanitized output is sometimes unusable, with no explanation.** Removing
   exploitable content can leave a document that renders but no longer serves
   its purpose — and the tool does not say what it took.
3. **The sanitizer is itself attack surface.** Parsing hostile input is the
   whole job, so the parser is the target. Commercial CDR has been shown
   vulnerable.
4. Encrypted or password-protected files cannot be sanitized without the key.
5. Archives are handled poorly or not recursively.

**Design implications, one per finding:**

- (1) is a *gap we are already in*. Raster conversion is where this project
  started; images are the covered case, not the missing one.
- (2) is answered directly by the loss vector. `properties destroyed: metadata`
  and an itemised passenger list is the report the complaint asks for. This is
  the clearest instance yet of the project's thesis paying off somewhere it was
  not designed for.
- (3) hardens ADR-0003 rather than complicating it. Engine hostility was already
  the assumption; sanitization makes isolation load-bearing instead of
  precautionary, and moves `sandbox-warden` onto the critical path.
- (4) and (5) are honest scope limits and should be refused explicitly rather
  than silently passed through. `huh clean` already refuses unidentifiable
  input for this reason: passing bytes through unchanged is the one genuinely
  dangerous outcome for a tool whose users believe it cleans files.

### 12.3b Prior art in open source — most of the tool already exists

Checked on 2026-08-21, prompted by the right question: *are we reinventing the
wheel?* Largely yes, and the honest accounting matters more than the pitch.

**Dangerzone** (Freedom of the Press Foundation, `dangerzone.rocks`) converts
potentially dangerous documents into safe PDFs by rendering them in disposable
containers, and strips all metadata. Its isolation is considerably better than
anything here: a **gVisor sandbox inside a container**, networking disabled,
nothing mounted from the host. That is precisely what ADR-0003 demands and what
this project has *not* built. The PDF flattening measured in
`experiments/pdf_flatten.py` (since superseded by `huh clean`) is a weaker version of what Dangerzone ships to
journalists today.

**mat2** (`jvoisin/mat2`, used by Tails) removes metadata across roughly 25
format families — avi, bmp, epub, flac, gif, jpeg, mp3, mp4, ODF, PDF, png,
pptx/xlsx/docx, svg, tar, tiff, torrent, wav, wmv, zip — at the file-structure
level, writing `.cleaned` copies rather than editing in place. Six raster
formats here is not competitive with that. **It is now archived (read-only)**,
which is the one opening: a widely-depended-on tool is unmaintained.

**Design implication, and a correction.** §12.3 positioned "images are
uncovered" and "nobody says what was removed" as market gaps. The first is
wrong as stated — mat2 covers images. The second is narrower than claimed:
neither tool reports what sanitization *cost*, but that is a missing feature,
not a missing product. **Building a parallel sanitizer is reinventing
Dangerzone and mat2 with less rigour.**

### 12.3c What is actually unfilled: the registry as public data

Searched for a machine-readable database of conversion loss — what each format
can carry, and what a given conversion destroys. **None found.** The digital
preservation field discusses conversion loss continuously (OpenAIRE, CESSDA,
USGS all publish prose guidance about resolution, layers, formatting and
precision being lost) but publishes no structured, measured data.

PRONOM identifies *what a file is*, with ~2,000 signatures and an XML API. There
is no equivalent for *what conversion costs you*. That is the asset this project
is uniquely arranged to produce, because everything here already exists to
generate it: a closed property vocabulary (ADR-0004), verified-not-claimed edges
(rule 6), specimen-backed measurement, and a validator that has repeatedly
caught genuine errors in hand-written data (cases in `registry/README.md`).

**Design implication:** the registry is the contribution; the CLI is the
instrument that produces it. A verified loss registry would be useful to
Dangerzone, to a mat2 successor, to `convert.to.it`, and to preservation
institutions — none of whom have one — in a way that a fourth sanitizer would
not be. This reframes the deliverable from *a tool that competes* to *data that
other tools consume*.

### 12.4 Two properties a sanitizer must have that a converter need not

- **Idempotence.** Sanitizing a clean file must be a no-op. If a second pass
  changes bytes, the output was never canonical, and "sanitized" means nothing.
  Enforced by `sanitize::tests::sanitizing_is_idempotent`.
- **Content preservation as a hard gate.** Destroying the file removes every
  passenger and is worthless. `huh clean` exits non-zero when SSIM shows the
  rebuild was unfaithful, so "safe but ruined" is a failure rather than a pass.

### 12.5 Open question: what does canonicalization guarantee?

We have measured that six raster formats through one engine converge. That is
evidence, not a theorem, and the honest scope of the claim is narrow:

- It holds for **content this engine parses**. A format with encoder freedom
  (JPEG quantization tables, PNG filter selection) could in principle carry
  information through a rebuild if the encoder preserved the input's choices.
  `image-rs` does not — it re-derives them — but that is an implementation
  property, not a guarantee, and it is unversioned.
- It says nothing about formats with **executable or scripted content** (PDF
  actions, Office macros, SVG scripts), where "content" and "code" are not
  separable by parsing alone.

**Design implication:** canonicalization must be a *tested property per format
pair*, recorded in the registry like any other capability claim, not an
architectural assumption. The same discipline as `verified` edges, for the same
reason — see `docs/RESEARCH.md §3.5`.


## 13. Cross-engine measurement — the first finding the registry produced

ADR-0011 predicted that recording loss per `(engine, from, to)` rather than per
`(from, to)` would matter. It does, and more than expected.

### 13.1 Two engines, one conversion, incompatible results

`core/examples/cross_engine.rs` runs `PNG → JPEG` at nominal quality 85 through
**image-rs** (Rust, in-process) and **libvips 8.15.1** (C, subprocess), scoring
both against the original.

| Specimen | vips bytes | image-rs bytes | vips SSIM | image-rs SSIM |
|---|---|---|---|---|
| gray16 | 836 | 1,023 | 0.9985 | 0.9989 |
| rgb-photo | 4,275 | 5,506 | 0.9950 | 0.9956 |
| rgba-opaque | 1,211 | 1,290 | 0.9967 | 0.9970 |
| **rgba-gradient** | 1,848 | 2,833 | **0.6022** | **0.9950** |

> **CORRECTED — see §13.8.** The SSIM column above is *invalid* for the
> transparency case. It compares stored RGB and ignores alpha, which scores the
> comparison's own choice of baseline rather than the engines. Re-measured
> against a defined background the ranking **reverses** depending on which
> background is chosen. The finding that survives is the policy difference
> below, established by pixel arithmetic rather than by SSIM.

Every specimen agrees closely except the one with **real transparency**. Both
engines record `alpha = lost`, which is true for both, and both are doing
something entirely different.

### 13.2 The mechanism, confirmed by pixel arithmetic

Sampling row 64 along an alpha ramp that runs opaque → transparent:

| x | original RGB / alpha | libvips | image-rs |
|---|---|---|---|
| 0 | (0,128,128) α=255 | (1,127,126) | (0,130,128) |
| 64 | (128,128,128) α=126 | (64,64,64) | (128,128,126) |
| 127 | (255,128,128) α=0 | (0,0,0) | (252,128,126) |

**libvips composites onto black**, scaling RGB by alpha: 128 × 126/255 ≈ 63,
measured 64. **image-rs discards the alpha channel** and keeps RGB untouched:
predicted 128, measured 128. Both match prediction to within JPEG quantisation
noise.

In practice: convert a logo with a transparent background, and libvips hands you
a **black box** while image-rs hands you whatever colour was hiding beneath the
transparency. Neither is wrong. Neither is documented at the point of use.

### 13.3 The honest caveat about that SSIM gap

The SSIM comparison is against the **un-composited** original RGB, which favours
image-rs by construction. Composite the original onto black first and libvips
would score better. The pixel table is the unambiguous evidence; the SSIM column
is the metric encoding a preference — exactly the Polyglot finding in §1.3, met
again from a different direction.

**Design implication:** report the flattening policy, never a winner.

### 13.4 `alpha = lost` is true and insufficient — a schema gap

This is the important consequence. Two engines carry identical loss vectors and
produce visually incompatible output. A user routing on the registry would be
badly surprised.

`lost` says the property is gone. It does not say **what replaced it**, and for
alpha that is the whole question. The vocabulary needs a qualifier — a
`flatten_policy` of `composite_black`, `composite_white`, `discard_channel` or
`premultiplied` — recorded per edge and measured, not assumed.

This is a genuine gap in ADR-0004's model, found by measurement rather than
argument, and it is the first thing this dataset has produced that its own
designers did not already know. Recorded here rather than fixed immediately:
changing the closed property vocabulary is an ADR-level decision, and one
specimen through two engines is grounds for a hypothesis, not a schema change.

### 13.5 Third engine, second specimen: the split is real

Done 2026-08-21. **ImageMagick 6.9.12** added as a third independent parser
family, and a second specimen (`corpus/raster/rgba-hard-edge.png` — fully opaque
beside fully transparent, no ramp) added so a policy cannot be an artifact of
one image.

| Engine | Transparent pixel becomes | Policy |
|---|---|---|
| libvips 8.15.1 | `(0,0,0)` | `composite_black` |
| image-rs 0.25 | `(220,40,41)` — original colour | `discard_channel` |
| ImageMagick 6.9.12 | `(220,40,41)` — original colour | `discard_channel` |

Identical results on both specimens, so **the policy belongs to the engine, not
the image**. libvips is the outlier, two to one.

That was sufficient grounds to amend the schema — see **ADR-0012**, which adds
an optional `policy` qualifier to a loss value, emitted in the dataset as
`loss_policy`. The qualifier is deliberately descriptive and stays out of the
composition algebra: once alpha is gone, a later hop's policy is irrelevant, and
pretending otherwise would corrupt ADR-0004's join.

**Design implication, and the honest limit:** this may be *configuration* rather
than engine identity. libvips can probably be told to discard instead of
composite. If most callers change it, the field records a default rather than a
behaviour and should record the invocation instead. That is the first thing to
test when a fourth engine is added, and it is written into ADR-0012's
"what would change our mind".


### 13.6 Loss belongs to the invocation — ADR-0012 falsified in a day

ADR-0012 closed with an explicit falsification condition: *if the flattening
policy is configuration rather than engine identity, the field records a default
and should record the invocation instead.* Two measurements met it within hours.

**Alpha flattening is a flag.** `vips jpegsave IN OUT` composites transparency
onto black; `vips jpegsave IN OUT --background 255` composites onto white. Same
engine, same version, same pair, different loss.

**WebP lossiness is a flag, and the default is the surprising one:**

| Invocation | Bytes | SSIM |
|---|---|---|
| `vips webpsave IN OUT` | 1,942 | **0.8362** |
| `vips webpsave IN OUT --lossless` | 15,512 | 1.0 |
| image-rs WebP encoder | 37,098 | 1.0 |

The consumer risk is concrete: read "PNG → WebP" from the image-rs edge —
lossless, SSIM 1.0 — then run libvips in your own pipeline and silently get
**0.84 SSIM**. Visible degradation, published as lossless.

**Design implication:** an edge is `(engine, version, invocation, from, to)`.
See **ADR-0013**. And the cross-engine WebP gap must be reported as a
*default-flags* difference, not a capability difference — calling libvips
"worse" would repeat §13.3's mistake of letting the measurement setup dictate
the verdict.

### 13.7 The vocabulary has no word for lossy compression

Found while writing the libvips WebP edge, and the most fundamental gap so far.

`PNG → JPEG` through image-rs on an 8-bit specimen: 8 bits in, 8 bits out,
geometry identical, colour profile intact, **SSIM 0.9950**. Something was
clearly lost. The registry records `color_depth = degraded`.

That label is wrong for this case. Nothing about colour depth changed. What
changed is that pixel values were *approximated* by quantisation. Scanning the
closed vocabulary for a term that means this:

- `color_depth` — bits per channel. Structural, and unchanged here.
- `geometry` — dimensions. Unchanged.
- `color_profile` — colour space. Unchanged.
- `precision` — "numeric precision of stored values", written for numeric data.

**Nothing covers it.** The most common form of loss in the entire raster domain —
lossy compression — has no property to name it, so it has been getting recorded
under `color_depth` where it does not belong.

Two distinct problems are tangled in that one label:

1. **Genuine colour-depth loss** — `gray16.png` (16-bit) into JPEG (8-bit). Real,
   correctly named.
2. **Quantisation loss** — 8-bit into JPEG at quality 85. Real, misnamed.

Both appear on the same edge because one edge covers specimens with different
properties. The measurement harness already distinguishes them by reporting
`unexercised`; the registry flattens that distinction away.

**Design implication, deliberately not acted on yet.** The vocabulary probably
needs a `signal_fidelity` property, valued like the others and measured by the
domain's own metric — SSIM for raster, spectral distance for audio. That is an
ADR-0004 change, and ADR-0004 has already been amended twice today (0012, 0013).
Making a third change in the same session, on a finding an hour old, would be
exactly the haste this project's discipline exists to prevent.

Recorded, proposed, and left for a future session with fresh evidence. The
existing edges stay as they are and are **known to be imprecise on this point** —
which is better documented than silently wrong.


### 13.8 The measuring instrument was wrong

The most important correction in this dossier, found while measuring the flag
variants ADR-0013 called for.

`vips webpsave --lossless` scored **SSIM 0.4691** on `rgba-hard-edge` while
*preserving* alpha. A lossless conversion cannot lose content, so the instrument
was at fault, not the engine.

**Cause.** `fidelity::ssim` calls `to_luma8()`, which ignores alpha and reads
**stored RGB**. A fully transparent pixel is invisible, so whatever RGB sits
beneath it cannot matter to a viewer — but it dominates a naive pixel
comparison. libvips premultiplies (storing `(0,0,0)` under transparency);
image-rs leaves the original colour there. Identical on screen, wildly different
to `to_luma8()`.

Compositing both images onto the same background before comparing fixes it:

| Invocation | before fix | after fix |
|---|---|---|
| `vips webpsave --lossless`, rgba-hard-edge | 0.4691 | **1.0000** |
| `vips webpsave --lossless`, rgba-gradient | 0.9731 | **1.0000** |
| `vips jpegsave --background 255`, rgba-hard-edge | 0.7193 | **0.9855** |

`--lossless` is genuinely lossless, alpha included. The earlier number was an
artifact.

### 13.9 The background choice decides the winner — demonstrated

§13.3 warned that a fidelity metric encodes a preference. `background_bias.rs`
now demonstrates it. The same two conversions, the same original, scored twice:

| Specimen | vips / white | image-rs / white | vips / black | image-rs / black |
|---|---|---|---|---|
| rgba-gradient | 0.4010 | **0.7946** | **0.9933** | 0.6069 |
| rgba-hard-edge | 0.4431 | **0.7281** | **0.9858** | 0.4691 |

**The ranking reverses.** Against white, image-rs wins by a wide margin. Against
black, libvips wins by a wider one. Neither engine changed; only the question
did.

This is not a subtlety to note in passing — it means **any published SSIM for an
alpha-losing conversion is meaningless without stating the background**, and
that §13.1's headline comparison was scoring the author's unstated choice of
baseline. It has been corrected in place rather than deleted.

**What survives unchanged.** ADR-0012 and ADR-0013 both rest on *directly
observed pixel values*, not on SSIM: libvips composites onto black, image-rs and
ImageMagick discard the channel, and `--background` / `--lossless` change the
behaviour. Those are arithmetic, and arithmetic does not have a baseline
problem. Only the quality *scores* were wrong.

**Design implications, three of them.**

1. `fidelity::ssim` now documents that it reads stored RGB and must not be used
   where transparency is involved; `ssim_composited` takes the background as an
   explicit argument because there is no neutral default and hiding the choice
   is what caused this.
2. Registry `measured` blocks for alpha-bearing edges must record the
   comparison background alongside the number. A score without it is not
   reproducible.
3. The general lesson, which applies well beyond alpha: **a measurement harness
   is itself an untested claim until something forces it to disagree with
   arithmetic.** This one went unchallenged for a day and produced a headline
   figure that was quoted in a PR description and two ADRs. The cross-check that
   caught it — a conversion that *must* score 1.0 scoring 0.47 — is the kind of
   invariant the harness should assert automatically.


## 14. Audio: does the vocabulary generalise?

The dataset was raster-only, which meant ADR-0004's property list had never been
tested outside the domain it was designed for. §13.7 also left the
`signal_fidelity` proposal open pending evidence "ideally from audio, where the
same question arises with a different metric". Both are now answered.

Engine: ffmpeg 6.1.1. Specimens generated by `corpus/generate_audio.py` as
hand-written PCM WAV, for the same reason the raster corpus is generated — you
have to know exactly what is in a specimen for it to prove anything.

### 14.1 What works

Three properties transferred without modification:

| Conversion | Property | Observed |
|---|---|---|
| stereo → mono (`-ac 1`) | `audio_channels` | 2 → 1, cleanly detectable |
| 16-bit → 8-bit PCM | `bit_depth` | 16 → 8, SNR **3.8 dB** on quiet content |
| WAV → FLAC | everything | lossless, 100.0% band energy retained |

The `bit_depth` case is a specimen-design lesson as much as a result. The same
conversion on loud content scores around 50 dB and is barely audible; on the
quiet specimen it is catastrophic. **A property is only measurable on a specimen
that exercises it**, which is the same trap as testing alpha with an opaque
image — recorded in `corpus/generate_audio.py` so the next person does not have
to rediscover it.

Similarly, `stereo-distinct.wav` carries *different tones* in each channel (A4
and E5), because downmixing identical channels is not observably lossy.

### 14.2 What breaks — and it is the same hole

`highfreq-mono.wav → MP3 64k`. Every tracked property:

| Property | Observed | Verdict |
|---|---|---|
| `audio_channels` | 1 → 1 | preserved |
| `sample_rate` | 44100 → 44100 | preserved |
| `bit_depth` | 16 → n/a | **not applicable** — MP3 has no fixed sample width |
| `time` | 1.00s → 1.00s | preserved |
| `metadata` | none → none | unexercised |

…while **~10% of the signal energy is gone**: the 1 kHz band retains 90.3% and
the 15 kHz band 90.2%. FLAC on the identical specimen through the identical
harness retains 100.0%, so the loss is the codec and not the instrument — a
control this project has learned to insist on (§13.8).

**Every property says preserved or not-applicable, and the audio is measurably
worse.** In raster the same loss was at least being misfiled under `color_depth`;
in audio there is nowhere to file it at all, because `bit_depth` does not apply
to the target format. ffprobe reports `bits_per_sample = 0` for MP3, which is the
tooling agreeing that the question is wrong.

**Design implication:** this is the second independent domain, with a different
metric, showing the same gap. That was the bar §13.7 set, so `signal_fidelity`
is now adopted — **ADR-0014**.

### 14.3 A second policy qualifier falls out

`audio_channels = lost` has exactly the problem `alpha = lost` had (ADR-0012):
it says the channels are gone and not what replaced them. Averaging left and
right is one choice; taking the left channel, or a weighted downmix, are others,
and they sound different.

Recorded as `policy = "downmix_average"` on the ffmpeg edge. **Whether other
engines choose differently is unmeasured**, and it is not assumed — that is the
mistake ADR-0012 was written to avoid, and repeating it here on the strength of
one engine would be worse than leaving it open.

### 14.4 Honest limits of this section

- **One engine.** Every audio measurement is ffmpeg 6.1.1. The alpha finding
  only became trustworthy at three engines; audio has one, so nothing here
  should be read as a property of the *format pair* rather than of this
  invocation.
- **Synthetic specimens.** Pure tones are the easy case for a perceptual codec,
  which is why MP3 attenuated both bands roughly evenly instead of showing the
  sharp lowpass that real music would. That makes 90.3% an *optimistic* figure,
  not a representative one.
- **Audio PUIDs are unverified.** `fmt/141`, `fmt/134` and `fmt/279` are
  recorded from memory and flagged `puid_verified = false`; PRONOM has not been
  consulted for them.


### 14.5 Second audio engine: one agreement, one disagreement

§14.4 flagged that every audio measurement came from ffmpeg alone, and that the
alpha finding only became trustworthy at three engines. **SoX 14.4.2** added as
the second. It produced one result of each kind, which is the useful outcome.

**Agreement — stereo downmix.** ffmpeg and SoX produce mono output differing by
**0.0% of signal RMS** on a specimen carrying different tones per channel. Both
average. `downmix_average` is therefore a genuine convention rather than one
engine's habit — unlike alpha flattening, where three engines split two-to-one.

Worth stating plainly because it is the easier finding to forget: **cross-engine
agreement is a result.** A registry that only records divergence would imply
every property is contested, which is not what the measurements say.

**Disagreement — bit depth.** 16-bit to 8-bit on quiet content:

| Invocation | SNR |
|---|---|
| `ffmpeg -c:a pcm_u8` | 3.8 dB |
| `ffmpeg -dither_method 0 -c:a pcm_u8` | 3.8 dB (flag had no visible effect) |
| `sox -b 8 -e unsigned-integer` | 5.2 dB |
| `sox -D -b 8 -e unsigned-integer` | **11.1 dB** |

Dither explains SoX's own 5.9 dB spread — it trades SNR for the removal of
correlated distortion, which is the intended bargain and confirms the flag does
what it claims. It does **not** explain the 7.3 dB gap between `sox -D` and
ffmpeg.

**Cause found — see §14.6.** It was one of the candidates listed here when this
section was first written: rounding to offset-binary `u8`. Left in place rather
than rewritten, because the sequence matters — the measurement was recorded as
unexplained first, and the explanation had to be earned.

**Design implication.** `bit_depth = degraded` is under-specified in precisely
the way `alpha = lost` was before ADR-0012: it says the property was reduced, not
*how*. It wants a policy vocabulary — `dither`, `truncate`, `round` are the
obvious candidates.

**The label waited for the explanation, and got it** — §14.6. `dither` would
have been the wrong label, which is the point of having waited.

**Also worth noting:** the specimen is quiet content, where 8-bit quantisation is
catastrophic. On loud content both engines score around 50 dB and would look
identical. This divergence is only visible because the corpus was built to
exercise the property, which is the §14.1 lesson arriving a second time.


### 14.6 The 7.3 dB gap explained: half an LSB

§14.5 recorded an unexplained 7.3 dB difference between ffmpeg and SoX on
16-to-8-bit reduction, and refused to label it. Running it down took two
measurements.

**Histogram of the written bytes.** The specimen peaks at ±327 in 16-bit, which
is ±1.28 counts in 8-bit — the signal barely spans two quantisation levels.

| Engine | Distinct values | Most common |
|---|---|---|
| ffmpeg | 4 | 128 (12,640), **127 (12,600)**, 129, 126 |
| sox | 5 | 127, 129, 128, 130, 126 — visibly dithered |
| `sox -D` | 3 | 129 (16,420), 127 (16,360), 128 (11,320) |

ffmpeg's two most common values are 128 and 127 in near-equal proportion. SoX's
are symmetric about 128. That asymmetry is the tell.

**Mean of the output.** In offset-binary `u8` a sine centred on zero must land on
128; any drift is a DC offset the conversion introduced.

| Engine | Mean | Offset from 128 |
|---|---|---|
| ffmpeg | 127.5025 | **−0.4975** |
| sox | 128.0010 | +0.0010 |
| `sox -D` | 128.0014 | +0.0014 |

**ffmpeg truncates; SoX rounds to nearest.** Half an LSB of DC offset, exactly.

On a signal peaking at 1.28 counts that offset is **39% of peak**, which is why
it dominates SNR here and would be invisible on loud content — the §14.1
specimen lesson for the third time.

**Design implications, and one correction to my own reasoning.**

1. `bit_depth` gets a policy vocabulary after all: **`round`** and **`truncate`**.
   Both are now measured rather than assumed, so rule 7 is satisfied.
2. **`dither` would have been the wrong label.** §14.5 named it as the leading
   candidate. Dither is real — it accounts for SoX's own 5.9 dB internal spread —
   but it is a *separate axis* from rounding, and it is already captured by the
   invocation (`sox` versus `sox -D`) under ADR-0013. Had the vocabulary been
   written when the gap was still unexplained, it would have encoded the wrong
   distinction and every subsequent entry would have inherited it.
3. That is the concrete argument for the rule this project keeps applying:
   **record the measurement immediately, and the label only once the mechanism
   is understood.** The cost of waiting was one section; the cost of guessing
   would have been a permanent vocabulary error.


### 14.7 Third audio engine: ffmpeg is the outlier

§14.6 established that ffmpeg truncates and SoX rounds, but two engines cannot
say which one is unusual. **GStreamer 1.24.2** (`audioconvert`, LGPL) was added
as the third, measured 2026-11-26 with `tools/measure_audio.py --snr --dc` on
the same quiet specimen:

| Invocation | SNR | DC shift (8-bit steps) |
|---|---|---|
| `ffmpeg -c:a pcm_u8` | 3.76 dB | **−0.4975** |
| `sox -D … -b 8` | 11.11 dB | +0.0014 |
| `gst … audioconvert dithering=none` | 11.11 dB | +0.0014 |
| `sox … -b 8` (dithered) | 5.09–5.16 dB | ≈0 |
| `gst … audioconvert` (TPDF dither, default) | 5.12 dB | +0.0018 |

With dither off, GStreamer's samples are **byte-identical** to `sox -D`'s. Two
independent engines round to nearest; ffmpeg truncates. Downmix agreed across
all three: GStreamer's mono output is bit-identical to `ffmpeg -ac 1`.

Two smaller findings:

- **GStreamer's dither is deterministic and SoX's is not.** Three GStreamer
  runs gave 5.12 dB each; SoX gave 5.09, 5.14, 5.16. A dithered SNR recorded
  from SoX is one sample from a distribution and should be read that way.
- **The DC-offset finding was itself unreproducible until now.** §14.6's
  −0.4975 came from a one-off histogram script. `measure_audio.py --dc` now
  reports it, with self-tests, so the claim that ffmpeg truncates can be
  checked by anyone.

**Design implication.** `policy = "truncate"` on the ffmpeg edge describes an
engine-specific choice, not a common convention, which is the case ADR-0012
and ADR-0013 exist for: the same `bit_depth = degraded` hides a half-step DC
shift in one of three engines. A consumer choosing a 16-to-8-bit route on
quiet material should prefer a `round` edge.

## 15. The pattern behind the policy qualifiers

Three policy vocabularies now exist — `alpha`, `audio_channels`, `bit_depth` —
and all three arrived the same way: engines disagreeing in a manner the four-value
scale could not express. That looked like it might be the shape of the problem
rather than three coincidences, so it was turned into a prediction and tested.

### 15.1 The generalisation

> When a property is destroyed or reduced, **something takes its place**, the
> choice is not determined by the format, and implementations choose differently.
> The loss value says *that* it happened. Only a policy says *what*.

Stated that way it predicts where the next divergence will be found: any
property whose destruction leaves a hole someone has to fill.

### 15.2 The prediction, and the test

GIF palettisation is the same shape — 16.7M colours reduced to 256 — so three
engines should disagree about *how*. Measured with
`core/examples/gif_dither.rs` on `rgb-photo.png`:

| Engine | Bytes | SSIM | Adjacent-pixel changes | Texture retained |
|---|---|---|---|---|
| image-rs | 13,703 | 0.9105 | 16% | 17% |
| **libvips** | **22,882** | 0.9065 | **56%** | **58%** |
| ImageMagick | 14,774 | 0.8799 | 19% | 20% |

**Confirmed.** libvips retains roughly three times the texture and writes 67%
more bytes — the signature of error diffusion. image-rs and ImageMagick sit
together, so **libvips is the outlier two-to-one, exactly as it was for alpha
flattening.**

### 15.3 A metric failure, found by accident

The SSIM column does not separate them. All three land within 0.03 while doing
visibly different things.

That is not SSIM misbehaving — it is a spatial-window statistic, and dithering
trades low-frequency banding for high-frequency noise, which such a statistic
barely registers. **Fidelity scores cannot detect dithering, and cannot rank
these engines.** Only the policy describes them.

The instrument that *did* separate them — the rate at which horizontally
adjacent pixels differ — was chosen after SSIM failed, not before. Worth
recording because the first version of the harness classified all three as
"flat palette map" using an SSIM threshold, which was wrong, and the second
version used the original image's own 96% adjacent-change rate as a baseline,
which was also wrong (palettisation always reduces it). **Third attempt.** The
measuring instrument being harder to get right than the measurement is now a
recurring theme — see §13.8.

No automatic classification is emitted, because any absolute threshold would be
invented rather than derived. The numbers are reported and compared to each
other.

### 15.4 What this does not license

The obvious next move is to add `color_depth` policies for every engine, and
then policies for `geometry`, `text_as_text`, `layers` and the rest on the same
reasoning. **That would be the mistake §14.6 just demonstrated.**

The `bit_depth` vocabulary was nearly written as `dither` while the mechanism
was unexplained; the real cause was rounding, and dither turned out to be a
separate axis already captured by the invocation. A vocabulary written from a
pattern rather than from a measured mechanism encodes the pattern's assumptions
permanently.

So: the generalisation is recorded as a **prediction that has survived one
test**, and each future policy still needs its own measurement and its own
explanation before it is named. The pattern says where to look. It does not say
what you will find.


## 16. PRONOM splits WebP the same way the invocation does

**Source:** PRONOM XML API, `fmt/566`, `fmt/567`, `fmt/568`, retrieved
2026-08-23. Measurement: libvips 8.15.1 against `corpus/raster/`.

`image-webp.toml` had carried no PUID since 2026-08-20, with a note saying
PRONOM's search returned nothing and predicting one existed "under a name the
search did not surface." Retried on the API directly, and the prediction held —
but the answer is not one identifier:

| PUID | Name | Version |
|---|---|---|
| `fmt/566` | WebP | Lossy |
| `fmt/567` | WebP | Lossless |
| `fmt/568` | WebP | Extended |

**PRONOM treats lossy and lossless WebP as different formats.** That is the
same line ADR-0013 draws between `vips webpsave` and `vips webpsave --lossless`
— arrived at independently and from the opposite direction. This project split
them because they *lose different things*; PRONOM split them because the files
*are different things*. Two disciplines, one boundary.

The first plausible guess at the identifier, `fmt/1508`, turned out to be
Microsoft Visio Drawing 2. Checking cost one API call; guessing would have put a
wrong PUID in the published dataset, which is exactly what §2.2's audit exists
to prevent. **The habit is the finding here, not the number.**

### 16.1 …and then the encoder ignores the split

Measured immediately after, because the neat correspondence above invites an
inference that turns out to be wrong. The first RIFF chunk after `WEBP` names
the variant — `VP8 ` lossy, `VP8L` lossless, `VP8X` extended:

| Input | Invocation | Chunk | Variant |
|---|---|---|---|
| opaque | `vips webpsave` | `VP8X` | Extended |
| opaque | `vips webpsave --lossless` | `VP8X` | Extended |
| alpha | `vips webpsave` | `VP8X` | Extended |
| alpha | `vips webpsave --lossless` | `VP8X` | Extended |

libvips 8.15.1 writes the extended container **every time**, whatever the flag
and whatever the content. So every libvips WebP edge in this registry emits
`fmt/568`, including the one whose entire purpose is being lossless.

**Design implication.** Format identity is not derivable from the edge, even
when a format registry and a conversion registry agree on where the boundary
lies. A consumer mapping our edges onto PRONOM identifiers must read the
produced bytes; inferring the PUID from `invocation` would be wrong for every
libvips edge here. This is ADR-0013's lesson one layer down: the invocation
determines what is *lost*, and the encoder determines what is *written*, and
those are not the same question.

image-rs is unmeasured and must not be assumed to match — assuming one encoder
resembles another is what §13.4 caught in the alpha work.


## 17. The audio numbers were unfalsifiable, then unrepresentative

**Source:** `tools/measure_audio.py` (added by this section's work), ffmpeg
6.1.1, `corpus/audio/`. Measured 2026-08-25.

### 17.1 A verified claim nobody could check

Every audio edge carried `status = "verified"` and cited a tool that did not
exist. `registry/edges/ffmpeg.toml` named

    tool = "corpus/generate_audio.py + ffprobe band-energy analysis"

The generator is committed. The analysis was done once, in a session, and only
its outputs were written down. Every *raster* edge cites a committed harness —
`core/examples/libvips_edges.rs`, `gif_dither.rs`, `probe_edges.rs` — so the gap
was specific to audio and invisible unless you went looking for the file.

This is worse than an ordinary documentation gap, because of what this project
claims to be. `CONTRIBUTING.md` says contradicting an existing entry with a
measurement is the single most valuable contribution anyone can make. **You
cannot contradict a measurement you cannot reproduce.** An unfalsifiable number
in a dataset whose entire pitch is falsifiability is a contradiction at the
centre of the thing.

The harness now exists, and the first job was to check whether the old numbers
survived it. **Every audio claim in the registry reproduced:**

| Claim | Recorded | Re-measured |
|---|---|---|
| MP3 64k, 1 kHz band | 0.903 | 0.9027 |
| MP3 64k, 15 kHz band | 0.902 | 0.9026 |
| FLAC control | 1.0 | 1.0000, bit-identical |
| ffmpeg `pcm_u8` SNR | 3.8 dB | 3.76 dB |
| SoX `-b 8` SNR | 5.2 dB | 5.09 dB |
| SoX `-b 8 -D` SNR | 11.1 dB | 11.11 dB |

Including the cross-engine gap §14.6 rests on: 11.11 − 3.76 = **7.35 dB**
against the recorded 7.3. The original measurements were honest. They were
simply unverifiable, and those are different faults with the same remedy.

**Design implication, implemented as rule 13.** `verification.measured.tool`
should name something runnable in this repository, and CI now warns when a
citation resolves to no such artifact. It is a warning rather than an error on
rule 8's precedent — some measurements genuinely are one-liners, and the
validator cannot tell those from a procedure that should have been committed.

**The rule would not have caught the bug that prompted it**, which is worth
stating plainly rather than quietly enjoying the new check. The audio citation
named `corpus/generate_audio.py` — a real file — *plus* a procedure that was
never committed, so an artifact resolved and it passed. Rule 13 catches the
blunter form: a citation naming nothing runnable at all, which is what four
other audio edges did. All six now cite `tools/measure_audio.py`.

### 17.2 The specimens were the easy case

All the original audio specimens are pure sine tones — stationary, narrowband,
and the simplest possible input for a perceptual codec. Two new specimens make
the hard cases available: `broadband-mono.wav` (deterministic PRNG, energy
everywhere, no tonal structure to model) and `transient-mono.wav` (clicks
against silence, which is what provokes pre-echo).

Measured across octave bands, MP3 at 64 kbit/s through ffmpeg 6.1.1:

| Band | highfreq (original specimen) | broadband | transient |
|---|---|---|---|
| 500–2000 Hz | 0.9027 | 0.9054 | 0.9221 |
| 8–16 kHz | 0.9026 | 0.9869 | 0.9046 |
| **16–22 kHz** | **n/a — absent** | **0.0564** | **0.0003** |

**MP3 at 64k deletes the top octave, and the original specimen could not see
it.** `highfreq-mono.wav` is a 1 kHz plus 15 kHz two-tone, so it has no energy
above 16 kHz at all — its high component sits *inside* the 8–16 kHz band. The
lowpass is the most conspicuous thing this codec does to real audio, and the
registry did not record it because nothing in the corpus reached up there.

FLAC on the same two specimens returns **bit-identical** output: 1.0000 in every
band, infinite SNR. That rules out the harness as the source of the 0.0564 —
if it fabricated loss on hard content, it would fabricate it here too.

**Design implication, and it generalises past audio.** `band_1khz_retained =
0.903` is true, reproducible, and misleading on its own: a reader concludes "MP3
costs about 10%", when the honest summary is "about 10% in the midrange, and
everything above 16 kHz". A single measurement is not a summary of an edge. It
is a summary of *what that specimen exercised*, which is why `specimens` is
published alongside `measured` and why both belong in the dataset (§16 makes the
same point about encoders and format identity).

The old number is kept rather than replaced. It remains the right answer to the
question it was asked.

### 17.3 Lossy codecs also ADD

The transient specimen returns retention **above 1.0** in the lowest bands —
1.4161 at 62–125 Hz, 1.1256 at 125–250 Hz. The codec put energy where the
original had almost none. That is pre-echo: quantisation noise spread backwards
in time across the encoding window, so noise arrives before the click that
caused it, and stationary tones cannot exhibit it at all.

**Design implication.** The vocabulary's four values describe what happened to a
property, and `signal_fidelity = degraded` is correct here — but "degraded"
reads as subtractive, and this is additive. No change is proposed: inventing a
fifth value on one measurement is exactly the haste ADR-0014 was careful to
avoid, and §14.6 records what it costs to name a mechanism before understanding
it. Noted so the next person measuring a lossy codec is not surprised, and so a
second independent instance has somewhere to land.

### 17.4 An instrument bug, caught by an implausible number

The first broadband run reported **345.16** retained in the 250–500 Hz band of
the two-tone specimen. The specimen has near-silence there, the codec has
quantisation noise there, and dividing negligible by negligible produces a
number that means nothing.

The guard was an absolute epsilon (`> 1e-12`), which catches exactly-zero and
nothing else. It is now a share of the original's total power, and such bands
report **not applicable** instead of a ratio — the audio form of a distinction
the raster side already draws in
`opaque_alpha_is_reported_as_unexercised_not_lost`. A property the specimen did
not exercise has no measurement, and inventing one is worse than admitting the
gap. Locked in by a self-test case reproducing the 345× number.

Third time an instrument in this repository has been wrong in a way that
produced plausible output (§13.8, §15.3, here). The pattern each time: a number
that could not be right, noticed because someone looked at it rather than
because a test failed.


## 18. Real files: the first real-world tier

Every specimen until now was synthetic, built to exercise one property. That is
how a loss is found, but it cannot say how often the loss bites on files
people actually have. Govdocs1 (digitalcorpora.org: ~1 million files from US
government web servers) supplied the first real-world tier, measured
2026-11-26.

**Access.** The corpus is on S3, which this environment reaches. Each thread is
a ~300 MB zip; `tools/fetch_govdocs.py` reads the zip directory and selected
members with HTTP Range requests, so the first 110 files cost ~40 MB and 45
seconds.
Digital Corpora says the files "may be (to the best of our knowledge) freely
redistributed". That is not CC0, so only a manifest (member, size, SHA-256) is
committed; the bytes are fetched and verified on demand. The OPF format corpus
is on GitHub, which the proxy blocks except for raw files.

**Sample.** The first members by name of `thread0.zip`: 40 PDF, 40 JPEG, 27
GIF. PNG is rare in Govdocs1 (3 to 6 per thread), so all 39 PNGs from the ten
threads were taken (added in §18.1; the first run had only thread0's 3).
Identified by content (`huh detect`), all as named.
`tools/run_govdocs.py` writes `corpus/govdocs1/results.json`.

| Set | Run | Result |
|---|---|---|
| 40 PDF | `huh clean` | 40/40 succeeded; **40/40 had extractable text, 0/40 kept it**; page count preserved in all 40 (1,788 pages); output **median 23× the input size** (1.4× to 179×) |
| 40 JPEG | `huh convert` → PNG | 40/40, SSIM 1.0, no contradictions; colour depth never exercised |
| 27 GIF | `huh convert` → PNG | 27/27, SSIM 1.0, no contradictions; transparency present in 2 |
| 39 PNG | `huh convert` → JPEG | 39/39, no contradictions; the 2 with transparency lost it as declared (SSIM 0.334 and −0.067, §18.1) |

**Findings.**

1. **The declared PDF cost is the typical case.** `huh clean` declares
   `text_as_text = lost`. Every real PDF sampled had text, so every one paid
   that cost. The synthetic specimen could only show that the loss exists.
2. **Size is a cost the loss vector cannot express.** Rendering pages to
   images made cleaned PDFs a median 23 times larger. No tracked property
   covers it, because size is not a property of the content. A consumer
   choosing a sanitisation route would want to know it.
3. **Real files rarely exercise what the synthetic corpus targets.** No real
   JPEG, GIF or PNG had more than 8 bits per channel, and only 4 of 106 had
   transparency. The synthetic specimens are still the right tool for finding
   a loss; the real tier says how often it matters.
4. **No contradictions.** Across 146 real files, no measured property
   contradicted a declared one. That is evidence the verified raster entries
   hold beyond the specimens they were measured on, within the properties
   these files exercise.

**Design implications.** Real-world prevalence ("exercised in N of M real
files") is worth publishing beside each loss, since it separates a loss that
is theoretical for most users from one that hits everyone. And output size
needs a home: either a measured cost field on edges or a documented reason it
stays out of scope.


### 18.1 Every verified image edge, on real files

`huh convert` exercises only the planner's cheapest route, so most verified
edges never met a real file. `Planner::plan_edge` builds a one-hop plan for a
named edge, and `core/examples/real_edges.rs` runs every verified raster edge
over every real specimen of its source format (identified by content), through
the same `execute` path and declared-vs-measured checks as `huh convert`. The
PNG sample grew to 39 by taking every PNG from all ten Govdocs1 threads.
Output: `corpus/govdocs1/edges.json`.

14 edges ran on 39 PNG, 40 JPEG or 27 GIF files each: **no failures and no
contradictions.** No real file had more than 8 bits per channel, so
`color_depth` was never exercised; 2 PNGs and 2 GIFs had transparency.

The harness lists every result with SSIM below 0.5 for a person to read. Two
edge families produced them:

- **PNG to JPEG (three edges): explained.** The low scores are exactly the two
  PNGs with transparency, which all three edges declare `alpha = lost`. The
  worst-case-background SSIM (§13.9) scores the flattened result against the
  other background, as designed.
- **ImageMagick PNG to GIF: not explained.** 303329.png and 189036.png scored
  0.0 and 0.0006, and no declared property accounts for it.

**The cause** is the PNG `oFFs` chunk, a print-page offset that changes nothing
about the pixels. `convert IN OUT.gif` writes it as the GIF frame position but
leaves the logical screen at the image size, so the frame falls outside the
screen. The same GIF decodes four ways:

| Decoder | Result |
|---|---|
| image-rs | 117×64, all black |
| libvips | 226×183, canvas enlarged |
| ffmpeg | refuses: "Invalid top position: 119" |
| ImageMagick | intact |

libvips `gifsave` and image-rs both write the frame at (0, 0). A negative
offset (671351.png, x = −720) is clamped to 0 and harmless. Reproduced on a
synthetic specimen (`corpus/raster/png-with-offset.png`) and pinned by a
characterisation test that CI runs with ImageMagick installed.

**Design implications.**

1. **Receipts did not catch this.** Every declared-vs-measured row agreed:
   size unchanged, alpha as declared. The falsification step checks only the
   properties it can observe, and placement is not one. SSIM caught it, and
   only because a person read the low scores. A loss the vocabulary has no
   word for is invisible to the checks built on the vocabulary, the same hole
   §13.7 found for lossy compression.
2. **Synthetic specimens could not have found it.** Nobody writes an `oFFs`
   chunk into a test image without already suspecting it. That is the case
   for the real-world tier in one example.
3. **Not forced into `geometry`.** Placement is not resolution. If more
   placement defects appear, a property for canvas placement belongs in the
   vocabulary via an ADR; one engine's bug does not justify one yet.


## 19. Office documents to PDF: what LibreOffice loses on real files

60 real Govdocs1 documents (the first 20 by name of each of `.doc`, `.xls` and
`.ppt` in `thread0.zip`) through `soffice --convert-to pdf`, LibreOffice
24.2.7.2, measured 2026-11-26 with `tools/measure_office.py`. Output:
`corpus/govdocs1/office.json`.

**All 60 converted. None crashed**, despite `crash_expected = true` on the
engine (§3.2); that describes large batches through unoserver, not one
document per process.

| Source | Distinct words in the PDF | Numbers in the PDF exactly | Requested font embedded |
|---|---|---|---|
| `.doc` (20) | median 0.997, min 0.968 | (not a loss; see 3 below) | 0 of 20 |
| `.xls` (18 with text) | median 0.983, min 0.701 | median 0.976, min 0.251 | 0 of 20 |
| `.ppt` (18 with text) | median 0.990, min 0.768 | 1.0 (6 with numbers) | 0 of 20 |

Reference text comes from catdoc, xls2csv and catppt, not from LibreOffice,
so the engine does not grade itself.

**The instrument was wrong four times before these numbers held.** Each error
produced a plausible figure:

1. **Counting occurrences.** catppt prints slide text several times over, so a
   faithful PDF scored 0.11 to 0.15 on word recall. Recall is now over distinct
   words.
2. **Numbers as words.** A spreadsheet stores `2179678` and the PDF prints
   `2,179,678`; every missing token in the worst `.xls` was a number. Grouping
   commas are now removed and numbers measured separately.
3. **Hidden field codes.** catdoc prints table-of-contents bookmark ids such as
   `_Toc124583466`, which nobody sees. They read as missing numbers in `.doc`,
   so `.doc` number recall is reported but is not a loss.
4. **Fonts LibreOffice invents.** Its flat-ODF font list includes its own
   defaults, so a substitute counted as a surviving requested font. A font now
   counts as requested only if its whole name occurs in the source bytes
   (`Inter` matched "international" and "inter-modal" until the match was
   bounded).

**Findings.**

1. **Text survives.** `.doc` and `.ppt` keep about 99% of distinct words. For
   PDF output, `text_as_text = preserved` is the right declaration.
2. **Spreadsheets lose stored precision.** What survives exactly is what was
   displayed: `1.79197397571441` becomes its formatted value. Median 97.6% of
   distinct numbers survive; the worst file keeps 25%. This is
   `precision = degraded`, inherent to printing a spreadsheet rather than a
   LibreOffice defect.
3. **Spreadsheet cells clip.** The worst `.xls` word score (0.70, `490684.xls`)
   is a long text cell, a glossary, cut at its neighbour's edge, so its words
   never reach the PDF.
4. **Fonts are always substituted here.** No document kept a requested font:
   Arial and Times New Roman became Liberation Sans and Serif (metric
   compatible), others fell back to DejaVu or Inter. **This depends on the
   installed fonts, not only on the engine**: this environment has no Microsoft
   core fonts. A `fonts` loss recorded for this edge has to name the font
   environment, or it describes a machine rather than a conversion.
5. **One unresolved case.** `300839.ppt` keeps 77% of distinct words. The
   missing text (`dominates`, `slide after next`) is in the raw file but in
   neither the slides, the speaker notes, the hidden slides nor LibreOffice's
   own model of the presentation. It may be PowerPoint "fast save" residue,
   which a PDF dropping would be correct, or an import gap. Telling them apart
   needs PowerPoint.

**Design implications.** These are the first office edges with evidence.
Making them first-class needs (a) format entries for the three binary Office
formats, (b) detection that reads the OLE2 directory, since all three share the
signature `D0 CF 11 E0`, and (c) a way for a `fonts` value to state the font
environment it was measured in.


### 19.1 Telling .doc, .xls and .ppt apart, and the first office edges

The three binary Office formats are now registry entries (PRONOM fmt/40,
fmt/61 and fmt/126, verified with `tools/verify_pronom.py`; the 6.0/95
versions fmt/39, fmt/59 and fmt/125 share their containers and are covered by
the same family-level entries). All three begin with the OLE2 header
`D0CF11E0A1B11AE1`, as does PRONOM's own signature for each, so that header
says only "OLE2". Detection now reads the compound file's directory and
matches on a stream name (`ole2_streams`: `WordDocument`; `Workbook` or
`Book`; `PowerPoint Document`). The parser runs on untrusted bytes before
anything else, so it is bounds-checked and its chain walks are capped; unit
tests cover a looping directory chain and truncated input.

**Real files caught a bug a synthetic specimen would not have.** The first
version collected every name in the directory. `098807.doc` is a Word document
with an Excel chart embedded in its `ObjectPool` storage, so it carried a
`Workbook` stream too and detected as Excel, flagged as an extension mismatch.
Only the root storage's direct children identify the file; the parser now
walks that tree alone. After the fix all 60 real files detect correctly: 20
Word, 20 PowerPoint, 18 Excel by content, and 2 Excel 4.0 worksheets (BIFF4,
which predates the OLE2 container) by extension at confidence 0.3, which is
the honest answer for a format the registry does not describe.

**Three LibreOffice edges**, `soffice --headless --convert-to pdf --outdir
OUTDIR IN`, each verified on a synthetic specimen with known content
(`corpus/generate_office.py`, compared against a `.truth.txt` sidecar) and
checked against the real files:

| Edge | Declared | Synthetic | Real (median, min) |
|---|---|---|---|
| `.doc` → PDF | `text_as_text` preserved, `fonts` degraded | words 1.0 | words 0.997, 0.968 |
| `.xls` → PDF | `text_as_text` degraded, `precision` degraded, `fonts` degraded | words 0.67 (clipped cell), stored values 0.0 | words 0.983, 0.70; values 0.976, 0.25 |
| `.ppt` → PDF | `text_as_text` preserved, `fonts` degraded | words 1.0 | words 0.99, 0.77 |

Two things the synthetic specimens could not measure, recorded so nobody tries:
catppt reads no text from a LibreOffice-written `.ppt` (hence the ground-truth
sidecars), and a LibreOffice-written file names LibreOffice's own fonts, so
font substitution is measured on real files only.

`structure` is not declared. LibreOffice 24.2 defaults to `UseTaggedPDF =
true` and all 60 real outputs are tagged, which contradicts the assumption in
`application-pdf.toml` that PDF output is untagged. A tag tree being present
is not a measurement of its fidelity, so it is noted rather than claimed;
tagged PDF probably wants its own format entry, as that entry's notes already
suggest.

**Executor.** `soffice` takes an output directory, not a file, so the
executor gained an `OUTDIR` placeholder: an empty directory that must hold
exactly one file afterwards. `soffice --headless` silently writes nothing
when another instance holds its profile, and that now surfaces as an engine
crash instead of a missing-file error. LibreOffice is GPL-only in the registry
(ADR-0007), so under the default `lgpl` profile these routes are pruned;
`huh convert` now says a route exists under `--profile gpl` instead of
implying none does.


## Sources

- [Convert To It](https://converttoit.com/) · [Grokipedia: Convert to it](https://grokipedia.com/page/Convert_to_it) · [Convert To It API](https://converttoit.com/api)
- [CloudConvert API v2](https://cloudconvert.com/blog/api-v2) · [CloudConvert: custom ffmpeg/imagemagick commands](https://cloudconvert.com/blog/convert-using-your-custom-ffmpeg-or-imagemagick-commands) · [Execute Commands](https://cloudconvert.com/docs/operations/execute-commands)
- [VERT.sh](https://github.com/VERT-sh/vert) · [VERT: Open Source WebAssembly File Converter](https://ascii.co.uk/news/article/news-20251208-3b0e427a/vert-open-source-webassembly-file-converter)
- [ConvertX](https://github.com/c4illin/ConvertX) · [ConvertX overview](https://noted.lol/convertx/)
- [Towards a Universal, Quantifiable, and Scalable File Format Converter (IEEE)](https://ieeexplore.ieee.org/document/5380873/) · [PDF via NARA](https://www.archives.gov/files/applied-research/ncsa/5-toward-a-universal-quantifiable-and-scalable-file-format-converter.pdf)
- [convertr — universal file-format converter CLI](https://github.com/Mark1708/convertr)
- [A Mathematical Basis for the Chaining of Lossy Interface Adapters](https://arxiv.org/pdf/0901.4835) · [Probabilistic Analysis of Loss in Interface Adapter Chaining](https://arxiv.org/pdf/1104.3469)
- [WHATWG MIME Sniffing / X-Content-Type-Options](https://developer.mozilla.org/en-US/docs/Web/HTTP/headers/X-Content-Type-Options) · [MIME Sniffing security implications (Coalfire)](https://coalfire.com/the-coalfire-blog/mime-sniffing-in-browsers-and-the-security)
- [PRONOM](https://www.nationalarchives.gov.uk/PRONOM/) · [Siegfried](https://github.com/richardlehane/siegfried) · [Siegfried introduction (OPF)](https://openpreservation.org/blogs/siegfried-pronom-based-file-format-identification-tool)
- [Magika (Google)](https://github.com/google/magika/tree/main) · [Magika: Deep Learning Meets Content Type Detection](https://research.google/pubs/magika-deep-learning-meets-content-type-detection/)
- [Boosting image processing performance, ImageMagick → libvips (Criteo)](https://medium.com/criteo-engineering/boosting-image-processing-performance-from-imagemagick-to-libvips-268cc3451d55) · [vips-benchmarks](https://github.com/jcupitt/vips-benchmarks) · [libvips slower on small images (issue #3654)](https://github.com/libvips/libvips/issues/3654)
- [unoserver](https://github.com/unoconv/unoserver) · [unoserver random crashes (issue #83)](https://github.com/unoconv/unoserver/issues/83) · [unoconv](https://github.com/unoconv/unoconv)
- [Pandoc filters / AST](https://pandoc.org/filters.html) · [Pandoc document representation](https://deepwiki.com/jgm/pandoc/4.1-document-representation-(pandoc-ast))
- [PDF processing with open-source tools (Bits & Bytes)](https://www.bitsgalore.org/2021/09/06/pdf-processing-and-analysis-with-open-source-tools)
- [Assimp file format support](https://github.com/assimp/assimp/blob/master/doc/Fileformats.md) · [Assimp format support (DeepWiki)](https://deepwiki.com/assimp/assimp/4-file-format-support)
- [ImageTragick](https://imagetragick.com/) · [CVE-2016-3714 (Red Hat)](https://access.redhat.com/security/vulnerabilities/ImageTragick) · [ImageMagick zero-days bypass security policies (Hadrian)](https://hadrian.io/blog/imagemagick-zero-days-bypass-multiple-security-policies-what-defenders-need-to-know) · [ImageMagick Security Policy Evaluator (Doyensec)](https://blog.doyensec.com/2023/01/10/imagemagick-security-policy-evaluator.html) · [GhostScript RCE bypass in ImageMagick](https://www.softwaresecured.com/post/imagemagick-rce-take-2)
- [A better zip bomb (Fifield)](https://www.bamsoftware.com/hacks/zipbomb/) · [Zip bomb (Wikipedia)](https://en.wikipedia.org/wiki/Zip_bomb) · [Pillow decompression bomb protection](https://github.com/python-pillow/Pillow/issues/515)
- [Firecracker vs gVisor (Northflank)](https://northflank.com/blog/firecracker-vs-gvisor) · [Sandboxing AI agents in 2026](https://dev.to/manveerchawla/how-to-sandbox-ai-agents-in-2026-firecracker-gvisor-runtimes-isolation-strategies-14pk) · [Comparing isolation technologies (SoftwareSeni)](https://www.softwareseni.com/firecracker-gvisor-containers-and-webassembly-comparing-isolation-technologies-for-ai-agents/)
- [ffmpeg.wasm performance](https://ffmpegwasm.netlify.app/docs/performance/) · [ffmpeg.wasm multi-threading](https://deepwiki.com/ffmpegwasm/ffmpeg.wasm/4.4-multi-threading)
- [WebCodecs vs ffmpeg.wasm benchmark (Remotion)](https://github.com/remotion-dev/webcodecs-benchmark) · [WebCodecs vs ffmpeg.wasm (BurnSub)](https://burnsub.com/blog/webcodecs-vs-ffmpeg-wasm/) · [Why ffmpeg.wasm can't use the GPU](https://dayverse.id/en/articles/why-ffmpeg-wasm-fails-leverage-gpu-acceleration/)
- [Up to 4GB of memory in WebAssembly (V8)](https://v8.dev/blog/4gb-wasm-memory) · [wasm64 / memory64 spec issue](https://github.com/WebAssembly/spec/issues/1892)
- [OPFS for WASM/SQLite/AI in the browser](https://webspecification.com/blog/origin-private-file-system/) · [OPFS explained](https://renderlog.in/blog/origin-private-file-system/)
- [WASI and the Component Model: current status](https://eunomia.dev/blog/2025/02/16/wasi-and-the-webassembly-component-model-current-status/) · [WebAssembly beyond the browser in 2026](https://zeonedge.com/blog/webassembly-server-side-wasm-wasi-component-model-2026)
- [When a queue isn't enough (Inngest)](https://www.inngest.com/blog/when-a-queue-isnt-enough) · [BullMQ going to production](https://docs.bullmq.io/guide/going-to-production)
- [Supabase Storage v3: resumable uploads](https://supabase.com/blog/storage-v3-resumable-uploads) · [tus protocol](https://tus.io/)
- [FFmpeg legal](https://www.ffmpeg.org/legal.html) · [x264 licensing](https://x264.org/licensing/) · [FFmpeg commercial license guide](https://32blog.com/en/ffmpeg/ffmpeg-commercial-license-guide)
- [Objective quality metrics: PSNR, SSIM, MS-SSIM, VMAF](https://www.forasoft.com/learn/video-encoding/articles/quality-metrics-psnr-ssim-vmaf) · [VMAF explained](https://www.transcodely.com/blog/vmaf-explained)
- [OSS-Fuzz ideal integration](https://google.github.io/oss-fuzz/advanced-topics/ideal-integration/) · [libFuzzer](https://llvm.org/docs/LibFuzzer.html) · [Efficient fuzzing guide (Chromium)](https://chromium.googlesource.com/chromium/src/+/main/testing/libfuzzer/efficient_fuzzing.md)
- [whisper.cpp via one FFmpeg command](https://medium.com/@vpalmisano/run-whisper-audio-transcriptions-with-one-ffmpeg-command-c6ecda51901f)
- [Claude Code: create custom subagents](https://code.claude.com/docs/en/sub-agents)
