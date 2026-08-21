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
  than silently passed through. `huh sanitize` already refuses unidentifiable
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
`experiments/pdf_flatten.py` is a weaker version of what Dangerzone ships to
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
(rule 6), specimen-backed measurement, and a validator that has caught four
genuine errors in hand-written data.

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
  passenger and is worthless. `huh sanitize` exits non-zero when SSIM shows the
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

Every specimen agrees closely except the one with **real transparency**, where
the gap is enormous. Both engines record `alpha = lost`, which is true for both,
and both are doing something entirely different.

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

**Next:** widen to more specimens and a third engine (Ghostscript or ImageMagick)
before proposing the vocabulary change. If the policy split holds, ADR-0004
needs amending.


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
