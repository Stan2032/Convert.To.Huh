# 0005 — Never trust the extension

Status: **accepted** · 2026-08-20

## Context

CVE-2024-29510 was exploited in the wild using **EPS files disguised as JPEGs**
(R§4.2). Browsers sniff content precisely because declared types are unreliable,
and that sniffing is itself an attack surface — HTML hidden in a `.jpg` becomes
stored XSS (R§2.1).

## Decision

Detection is content-first and layered (R§2):

1. Structural signatures with PRONOM-style priority rules — DOCX beats ZIP.
2. Deep container inspection — an MP4 with no video track is not a video.
3. ML classification (Magika-class, ~99% accuracy, ~5 ms) as an **advisory**
   tier for textual and ambiguous cases where signatures are structurally
   incapable (R§2.3).

The ML tier may raise confidence and break ties. It **may never override a
decisive structural parse**, because every detection must be explainable.

The extension is recorded separately as `declared_extension`, so a mismatch is
visible rather than silently trusted.

Everything we serve back to a browser carries `X-Content-Type-Options: nosniff`
and defaults to `Content-Disposition: attachment` — we hand users files derived
from attacker-supplied bytes, which is exactly the situation those headers exist
for.

## Consequences

- Detection costs a read of the file's structure, not just its first 8 bytes.
- We inherit PRONOM identifiers, joining our registry to the digital
  preservation world for free.
- Extension/content mismatch is a signal worth surfacing to the user — often
  it is an honest mistake, occasionally it is an attack.

## What would change our mind

Nothing. This is a security invariant.
