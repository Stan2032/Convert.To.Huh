# 0010 — Conversion is not a cipher; it is a canonicalizer

Status: **accepted** · 2026-08-21

Supersedes nothing. Redirects the project's stated purpose, recorded in the
repository description, from *encryption via file conversion* to *verified
sanitization*.

## Context

The original premise for this repository:

> "encryption via file conversion through an application which generated a key
> based on path of conversions taken"

The intuition is that a conversion chain is a transformation, transformations
are what ciphers do, and the route taken is secret information the recipient
would need. It deserves a real test rather than a shrug, so it got one.

### The experiment

`core/examples/path_channel.rs` runs seven genuinely different **lossless**
routes over one specimen and compares the outputs. The intermediates differ
enormously — 218 bytes through WebP, 65,750 through TIFF, a 300x spread — so
these are not variations on one route. The outputs:

| Route | Bytes | SHA-256 |
|---|---|---|
| `png` | 993 | `e2a4be89ea8184b0…` |
| `bmp → png` | 993 | `e2a4be89ea8184b0…` |
| `tiff → png` | 993 | `e2a4be89ea8184b0…` |
| `webp → png` | 993 | `e2a4be89ea8184b0…` |
| `bmp → tiff → png` | 993 | `e2a4be89ea8184b0…` |
| `webp → bmp → tiff → png` | 993 | `e2a4be89ea8184b0…` |
| `tiff → webp → bmp → png` | 993 | `e2a4be89ea8184b0…` |

**Byte-identical. Every route.**

So `output = f(input)`, not `f(input, path)`. The path is not concealed by the
output; it is *absent from* it.

### Why restricting to lossless conversions makes this worse, not better

The natural rescue is: lossy hops destroy information, so allow only lossless
ones and the chain becomes invertible. Every route in the table above is
already lossless-only. That restriction is not a fix — it is the mechanism of
the failure, and there is a one-line argument for why.

**A lossless conversion preserves content exactly. Therefore any two lossless
routes between the same endpoints compute the same function on content. Path
independence is not a side effect of losslessness; it is the definition of it.**

If two lossless routes produced different content, at least one of them was not
lossless. So the more strictly losslessness is enforced, the more completely the
path is erased.

> **CORRECTION 2026-08-23 — the theorem is about content, not bytes.**
>
> This section originally ended "the measurement above is a special case of a
> theorem", which claimed more than the argument supports. Losslessness fixes
> the **content**. It does not fix the **serialization**: a lossless encoder may
> write the same pixels many valid ways, and byte-identity additionally requires
> that the same encoder, version and settings produced the file.
>
> Measured, because asserting it here would repeat the original mistake — three
> lossless PNG encodings of one image, `vips pngsave` at default and at
> `--compression 0` against the original:
>
> | File | SHA-256 (first 16) |
> |---|---|
> | `rgb-photo.png` | `2c24e144484680f5` |
> | `vips pngsave` | `24480df09460b76b` |
> | `vips pngsave --compression 0` | `d3b2e1e80f30b7a4` |
>
> Three different files. `vips subtract` between the last two gives min and max
> both exactly `0.000000` — identical pixels, different bytes.
>
> **The decision does not change, and the argument against the cipher is
> unaffected** — it only ever needed content to be path-independent, which is
> what the theorem actually proves. Keying on the path requires the output to
> depend on the path; lossless routes agree on content, so nothing is left to
> key on. The seven byte-identical routes in the table above are a stronger
> empirical fact that also required a fixed encoder, and they remain true as a
> measurement of this toolchain rather than as a corollary of losslessness.
>
> The canonicalizer conclusion also survives intact, and is if anything stated
> more precisely by the distinction: output is a function of
> `(content, encoder, settings)` and **not** of the route taken. That is exactly
> the property a sanitizer needs — everything not reconstructed from parsed
> content is gone, whichever way the file arrived.
>
> Raised in review. The habit that produced the error is worth naming: the
> sentence generalised a measurement into a theorem, which is the same move
> ADR-0013 caught ADR-0012 making, and the same one the fidelity harness made in
> `RESEARCH.md §13.8`.

Stated as the tension that kills the idea:

- For a route to be reversible, its hops must be **lossless**.
- For a route to act as a key, the output must **depend on which route was
  taken**.
- Lossless hops are exactly those whose output does not depend on the route.

**Reversibility and key-dependence are mutually exclusive here.** No choice of
formats escapes it, because the constraint is on the meaning of "lossless"
rather than on any particular format's capabilities.

### Changing MEDIUM repeatedly does not help either

The strongest form of the proposal: convert image to audio, audio to image,
again and again, stripping every metadata trail, so nothing indicates what to
convert back into. `experiments/binary_waterfall.py` tests it — three changes of
medium, no extension, no filename, no metadata, no key.

The original was recovered **byte-identical** in a single pass. The scan shows
why: each layer is a header prepended to an untouched payload.

| Offset | Found |
|---|---|
| 0 | RIFF container (outer "audio") |
| 44 | BMP header ("image" layer) |
| 98 | RIFF container (first "audio") |
| 142 | **the original PNG, intact** |

Reinterpreting bytes as another medium does not change the bytes. File carving
is standard forensics, automated in `binwalk` and `foremost`. Notably the
reference implementation's own contributor guide forbids this style —
"avoid simple binary waterfalls, as they're not semantically meaningful."

The alternative, *genuine* semantic conversion (sonify the image, transcribe the
audio), does change the bytes — and destroys information at every hop, for the
sender as much as the attacker. Sonify an image and run speech recognition on
it: there is no speech, so the output bears no relation to the input. Not
concealed. Gone.

**Anything decodable is identifiable.** For a recipient to reverse the chain,
every intermediate must be a well-formed file of its type; well-formed means it
carries that type's structure; and that structure is precisely what a detector
keys on. A file cannot be decodable-by-the-right-person yet structurally
anonymous.

Stripping metadata does not touch this. Identification is content-based —
ADR-0005, this project's founding rule. Our own detector returned `audio/wav` at
confidence 1.00 on the stripped blob, from structure alone. PRONOM holds ~2,000
such signatures; `libmagic`, `siegfried` and Magika do the same.

Nor is guessing the path required. Each layer announces itself: WAV, then BMP,
then PNG — signposts, not a search space. And the attacker never needs the path
anyway, because they are extracting content, not reconstructing a route.

### Two further objections, each independently fatal

**Entropy.** Suppose the path did survive. A 128-bit key needs 3.4 × 10³⁸
distinct routes. At a realistic out-degree of ~10 that is a 38-hop chain, and
each hop costs time and risks quality. Worse, a chooser is not uniform: a human
picking a "sensible" route picks from the planner's top handful, which is
**~3.3 bits** for ten options. A six-character random lowercase password is 28.

**Self-description.** Formats announce themselves. The output of every route
above is a valid PNG with PNG magic bytes that any viewer opens. There is no
ciphertext at any stage — only a picture that took a scenic route.

### The strongest version, and why it also fails

The idea can be rescued by making encoders **key-dependent**: choose encoder
parameters from key material so the bytes vary while the content does not. That
genuinely works, and it is a real technique. But at that point the conversion is
not doing cryptographic work — a hand-rolled cipher is, with the codec as a
codebook. Security rests wholly on the key, the construction is unreviewed, and
AES does the same job with decades of analysis behind it. This is the classic
homebrew-crypto trap, entered from an unusual direction.

## Decision

**Conversion is not a cipher, and this project will not present it as one.**

The property the experiment actually demonstrates is more valuable than the one
it refutes: lossless conversion **reliably destroys provenance while preserving
content**. Seven histories, one output. That is canonicalization, and it is the
foundation of Content Disarm and Reconstruction — rebuilding a file from its
parsed content so that anything not part of that content (hidden payloads,
tracking metadata, exploit structures, steganographic passengers) does not
survive the round trip.

The project's differentiator carries over intact. Existing CDR tools sanitize
and report success. This one already measures, property by property, **what was
destroyed** — and can hand back a receipt saying so.

So the goal becomes: **a sanitizer that proves what it removed.**

## Consequences

- The repository description should be updated; it currently describes the
  refuted premise.
- `docs/RESEARCH.md` needs a CDR section, and the threat model shifts. The
  adversary is no longer someone reading a file they should not — it is someone
  who **planted something in a file** that should not survive conversion.
- Canonicalization becomes a **testable property**, not an assumption: for a
  given format pair, do all lossless routes converge? `path_channel.rs` is the
  first instance of that test and should generalize into the fidelity harness.
- `sandbox-warden` gains a second remit. Sanitization means deliberately feeding
  hostile input to parsers, so isolation moves from precaution to load-bearing.
- The loss vector acquires a second reading. `metadata = lost` is a *defect* for
  a converter and a *feature* for a sanitizer. Same measurement, opposite sign —
  which means the vector needs a per-property notion of intent, and the ranking
  policy in `loss-vector.md §7` is now underspecified in a new way.
- Determinism becomes security-relevant rather than merely convenient: a
  sanitizer whose output varies run to run cannot be audited.

## What would change our mind

A construction in which the output of a **reversible** conversion demonstrably
depends on the route taken, without smuggling key material into encoder
parameters. That would falsify the tension above and reopen the question. The
argument given here is that no such construction exists, because losslessness is
precisely path independence.

Separately: if measurement shows conversion does *not* reliably canonicalize —
if some lossless routes turn out to leave recoverable traces — that breaks the
sanitization case just as firmly as it would have rescued the cipher case. The
same test decides both. It has been run for six raster formats through one
engine; that is a start, not a result.

## References

- `core/examples/path_channel.rs` — the experiment.
- `docs/RESEARCH.md §5` — engine hostility; why sanitization needs isolation.
- ADR-0004 — loss as a vector, which supplies the "what was destroyed" report.
