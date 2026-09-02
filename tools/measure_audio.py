#!/usr/bin/env python3
"""Measure what an audio conversion did to the signal.

    python3 tools/measure_audio.py --self-test
    python3 tools/measure_audio.py ORIGINAL.wav CONVERTED.mp3 [--bands 1000,15000]
    python3 tools/measure_audio.py ORIGINAL.wav CONVERTED.mp3 --broadband

WHY THIS EXISTS
---------------
It should have existed already. The audio edges in `registry/edges/ffmpeg.toml`
and `sox.toml` carry `status = "verified"` and cite

    tool = "corpus/generate_audio.py + ffprobe band-energy analysis"

The generator is committed. The analysis never was — it was done once, in a
session, and the numbers were written into the registry. Every RASTER edge cites
a committed harness (`core/examples/libvips_edges.rs`, `gif_dither.rs`,
`probe_edges.rs`); the audio edges cited a procedure nobody could run.

That makes those numbers unfalsifiable, and CONTRIBUTING.md says contradicting
an entry with a measurement is the most valuable contribution anyone can make.
You cannot contradict a measurement you cannot reproduce. So the first job of
this file is to make the existing audio claims checkable, including by someone
who wants to prove them wrong.

METRIC
------
Band energy retained: the fraction of the original's power in a frequency band
that survives in the converted file. 1.0 is untouched, 0.0 is gone. Reported per
band, never averaged into a single number — a codec that keeps the bass and
deletes the treble is not "90% good" (ADR-0004: loss is a vector, not a scalar).

Comparison is on DECODED SAMPLES, so it works across formats: both sides are
decoded to canonical PCM first. That is the audio analogue of the raster lesson
in RESEARCH.md §13.8, where comparing stored representations rather than decoded
content made the harness measure its own assumptions.

Pure stdlib, like the other tools here. The DFT is a plain radix-2 FFT; at these
sizes it is fast enough and avoids a numpy dependency in a repository whose
validators are deliberately dependency-free.
"""
from __future__ import annotations

import argparse
import cmath
import math
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

RATE_DEFAULT = 44100


# --------------------------------------------------------------------------
# WAV reading. Deliberately strict: a specimen we cannot parse exactly is a
# specimen we should not report numbers about.
# --------------------------------------------------------------------------

def read_wav(path: Path) -> tuple[list[list[float]], int]:
    """Return (channels as float lists in [-1, 1], sample rate)."""
    b = path.read_bytes()
    if b[:4] != b"RIFF" or b[8:12] != b"WAVE":
        raise ValueError(f"{path.name}: not a RIFF/WAVE file")

    i = 12
    fmt = None
    data = None
    while i + 8 <= len(b):
        cid = b[i:i + 4]
        size = struct.unpack("<I", b[i + 4:i + 8])[0]
        body = b[i + 8:i + 8 + size]
        if cid == b"fmt ":
            fmt = struct.unpack("<HHIIHH", body[:16])
        elif cid == b"data":
            data = body
        i += 8 + size + (size & 1)  # chunks are word-aligned

    if fmt is None or data is None:
        raise ValueError(f"{path.name}: missing fmt or data chunk")
    _, nch, rate, _, _, bits = fmt
    width = bits // 8
    if bits not in (16, 32):
        raise ValueError(f"{path.name}: unsupported {bits}-bit PCM")

    unpack = {16: "<h", 32: "<i"}[bits]
    peak = float(2 ** (bits - 1))
    frames = len(data) // (width * nch)
    chans: list[list[float]] = [[] for _ in range(nch)]
    for f in range(frames):
        base = f * width * nch
        for c in range(nch):
            off = base + c * width
            v = struct.unpack(unpack, data[off:off + width])[0]
            chans[c].append(v / peak)
    return chans, rate


def decode_to_wav(path: Path, rate: int, tmp: Path) -> tuple[list[list[float]], int]:
    """Decode anything ffmpeg understands into canonical 16-bit PCM.

    A .wav input is still routed through ffmpeg, so both sides of a comparison
    go through exactly the same decode path and any decoder quirk cancels
    instead of showing up as loss.
    """
    out = tmp / (path.stem + ".decoded.wav")
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-i", str(path),
         "-ar", str(rate), "-acodec", "pcm_s16le", "-f", "wav", str(out)],
        check=True,
    )
    return read_wav(out)


# --------------------------------------------------------------------------
# Spectrum
# --------------------------------------------------------------------------

def fft(x: list[complex]) -> list[complex]:
    """Iterative radix-2 FFT. Input length must be a power of two."""
    n = len(x)
    if n & (n - 1):
        raise ValueError("FFT length must be a power of two")
    # bit-reversal permutation
    out = list(x)
    j = 0
    for i in range(1, n):
        bit = n >> 1
        while j & bit:
            j ^= bit
            bit >>= 1
        j |= bit
        if i < j:
            out[i], out[j] = out[j], out[i]
    length = 2
    while length <= n:
        ang = -2 * math.pi / length
        wl = cmath.exp(1j * ang)
        for i in range(0, n, length):
            w = 1 + 0j
            for k in range(i, i + length // 2):
                u = out[k]
                v = out[k + length // 2] * w
                out[k] = u + v
                out[k + length // 2] = u - v
                w *= wl
        length <<= 1
    return out


def hann(n: int) -> list[float]:
    if n == 1:
        return [1.0]
    return [0.5 - 0.5 * math.cos(2 * math.pi * i / (n - 1)) for i in range(n)]


def power_spectrum(samples: list[float], rate: int) -> tuple[list[float], float]:
    """Averaged power spectrum over Hann-windowed half-overlapping frames.

    Averaging across frames matters for anything non-stationary: a single frame
    of a transient says whatever that frame happened to catch.
    """
    size = 4096
    if len(samples) < size:
        samples = samples + [0.0] * (size - len(samples))
    win = hann(size)
    hop = size // 2
    acc = [0.0] * (size // 2 + 1)
    frames = 0
    for start in range(0, len(samples) - size + 1, hop):
        frame = [samples[start + i] * win[i] for i in range(size)]
        spec = fft([complex(v, 0.0) for v in frame])
        for k in range(size // 2 + 1):
            acc[k] += abs(spec[k]) ** 2
        frames += 1
    if frames:
        acc = [v / frames for v in acc]
    return acc, rate / size  # spectrum, Hz per bin


def band_energy(spec: list[float], hz_per_bin: float, lo: float, hi: float) -> float:
    a = max(0, int(math.floor(lo / hz_per_bin)))
    b = min(len(spec) - 1, int(math.ceil(hi / hz_per_bin)))
    return sum(spec[a:b + 1])


# --------------------------------------------------------------------------
# Comparison
# --------------------------------------------------------------------------

# A band holding less than this share of the original's total power is treated
# as UNEXERCISED rather than measured.
#
# Not a nicety. On `highfreq-mono.wav` — a 1 kHz plus 15 kHz two-tone — the
# 250-500 Hz band originally reported "345.16 retained", because the specimen
# has essentially nothing there and the codec's own quantisation noise is
# larger than the near-silence it replaced. Dividing negligible by negligible
# produces a number, and the number is meaningless.
#
# This is the audio form of a distinction the raster side already makes:
# `opaque_alpha_is_reported_as_unexercised_not_lost`. A property the specimen
# never exercised has no measurement, and inventing one is worse than
# admitting the gap.
UNEXERCISED_SHARE = 1e-6


def retained(orig: list[float], conv: list[float], rate: int,
             bands: list[tuple[float, float, str]]) -> list[tuple[str, float]]:
    so, hz = power_spectrum(orig, rate)
    sc, _ = power_spectrum(conv, rate)
    total = sum(so)
    out = []
    for lo, hi, label in bands:
        eo = band_energy(so, hz, lo, hi)
        ec = band_energy(sc, hz, lo, hi)
        # Nothing meaningful in the ORIGINAL band: not applicable, not lost.
        if total <= 0 or eo / total < UNEXERCISED_SHARE:
            out.append((label, float("nan")))
        else:
            out.append((label, ec / eo))
    return out


def snr_db(orig: list[float], conv: list[float]) -> float | None:
    """Signal-to-noise ratio in dB, treating the difference as noise.

    The metric the `bit_depth` edges cite. Requires sample-aligned inputs of the
    same length, which holds for a re-quantisation but NOT for a codec that
    pads or delays — MP3 famously adds encoder delay. Returns None rather than a
    confidently wrong number when the lengths disagree, because a misaligned SNR
    looks like catastrophic loss and would be read as one.
    """
    if len(orig) != len(conv):
        return None
    signal = sum(v * v for v in orig)
    noise = sum((a - b) ** 2 for a, b in zip(orig, conv))
    if signal <= 0:
        return None
    if noise <= 0:
        return float("inf")  # bit-identical
    return 10.0 * math.log10(signal / noise)


def octave_bands(rate: int) -> list[tuple[float, float, str]]:
    """Octave bands up to Nyquist — the honest shape for broadband content.

    Two numbers for two tones tells you about those tones. Real content has
    energy everywhere, and a codec treats the top octave very differently from
    the bottom one.
    """
    bands = []
    lo = 62.5
    while lo < rate / 2:
        hi = min(lo * 2, rate / 2)
        label = f"{int(lo)}-{int(hi)}Hz"
        bands.append((lo, hi, label))
        lo *= 2
    return bands


def self_test() -> int:
    """Check the instrument before trusting it, per this repo's habit."""
    failures = 0
    rate = RATE_DEFAULT

    def tone(freq, secs, amp):
        return [amp * math.sin(2 * math.pi * freq * t / rate)
                for t in range(int(secs * rate))]

    # 1. A signal compared with itself must retain everything.
    a = tone(1000, 0.5, 0.5)
    for label, v in retained(a, a, rate, [(900, 1100, "1k")]):
        ok = abs(v - 1.0) < 1e-9
        print(f"  [{'PASS' if ok else 'FAIL'}] identical signal retains 1.0 ({v:.6f})")
        failures += not ok

    # 2. Energy must land in the right band, not a neighbouring one.
    spec, hz = power_spectrum(tone(1000, 0.5, 0.5), rate)
    in_band = band_energy(spec, hz, 900, 1100)
    out_band = band_energy(spec, hz, 3000, 8000)
    ok = in_band > out_band * 1e3
    print(f"  [{'PASS' if ok else 'FAIL'}] 1 kHz tone's energy is in the 1 kHz band "
          f"(in={in_band:.3e} out={out_band:.3e})")
    failures += not ok

    # 3. Halving amplitude quarters power — the metric is power, not amplitude.
    half = tone(1000, 0.5, 0.25)
    v = retained(a, half, rate, [(900, 1100, "1k")])[0][1]
    ok = abs(v - 0.25) < 1e-6
    print(f"  [{'PASS' if ok else 'FAIL'}] half amplitude retains 0.25 power ({v:.6f})")
    failures += not ok

    # 4. Removing a band entirely reads as 0.0, not as a small number.
    two = [x + y for x, y in zip(tone(1000, 0.5, 0.4), tone(15000, 0.5, 0.4))]
    one = tone(1000, 0.5, 0.4)
    v = retained(two, one, rate, [(14000, 16000, "15k")])[0][1]
    ok = v < 1e-6
    print(f"  [{'PASS' if ok else 'FAIL'}] a deleted band retains ~0.0 ({v:.2e})")
    failures += not ok

    # 5. A band absent from the ORIGINAL is not-applicable, not lost.
    v = retained(one, one, rate, [(18000, 20000, "18k")])[0][1]
    ok = math.isnan(v)
    print(f"  [{'PASS' if ok else 'FAIL'}] empty band in the original reads nan ({v})")
    failures += not ok

    # 6. A band the specimen barely exercises must not produce a ratio.
    #    Real regression: highfreq-mono reported "345.16 retained" at 250-500 Hz,
    #    where it has near-silence and the codec has quantisation noise.
    noisy = [x + 1e-4 * math.sin(2 * math.pi * 400 * t / rate)
             for t, x in enumerate(one)]
    v = retained(one, noisy, rate, [(250, 500, "400")])[0][1]
    ok = math.isnan(v)
    print(f"  [{'PASS' if ok else 'FAIL'}] a barely-exercised band reads nan, "
          f"not a huge ratio ({v})")
    failures += not ok

    # 7. SNR: identical is infinite, and misaligned lengths refuse to guess.
    ok = snr_db(a, a) == float("inf")
    print(f"  [{'PASS' if ok else 'FAIL'}] SNR of a signal with itself is infinite")
    failures += not ok
    ok = snr_db(a, a[:-10]) is None
    print(f"  [{'PASS' if ok else 'FAIL'}] SNR refuses misaligned lengths")
    failures += not ok

    print(f"\nself-test: {8 - failures}/8 passed")
    return 1 if failures else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("original", nargs="?")
    ap.add_argument("converted", nargs="?")
    ap.add_argument("--bands", help="comma-separated centre frequencies, e.g. 1000,15000")
    ap.add_argument("--broadband", action="store_true",
                    help="report octave bands across the spectrum")
    ap.add_argument("--snr", action="store_true",
                    help="report signal-to-noise ratio in dB (needs equal lengths)")
    ap.add_argument("--rate", type=int, default=RATE_DEFAULT)
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()

    if args.self_test:
        return self_test()
    if not args.original or not args.converted:
        ap.error("need ORIGINAL and CONVERTED (or --self-test)")

    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        co, ro = decode_to_wav(Path(args.original), args.rate, tmp)
        cc, _ = decode_to_wav(Path(args.converted), args.rate, tmp)

    if args.broadband:
        bands = octave_bands(ro)
    elif args.bands:
        bands = []
        for tok in args.bands.split(","):
            f = float(tok)
            bands.append((f * 0.9, f * 1.1, f"{int(f)}Hz"))
    else:
        bands = [(900, 1100, "1000Hz"), (13500, 16500, "15000Hz")]

    print(f"{Path(args.original).name} -> {Path(args.converted).name}")
    print(f"  channels {len(co)} -> {len(cc)}   rate {ro}")
    # Channel 0 only: downmix is a separate property (`audio_channels`) with its
    # own observation, and mixing the two questions is how a stereo-collapse
    # gets misreported as signal loss.
    for label, v in retained(co[0], cc[0], ro, bands):
        shown = "n/a (absent in original)" if math.isnan(v) else f"{v:.4f}"
        print(f"  {label:<16} retained {shown}")

    if args.snr:
        v = snr_db(co[0], cc[0])
        if v is None:
            print(f"  {'SNR':<16} n/a — lengths differ "
                  f"({len(co[0])} vs {len(cc[0])} samples), so alignment is unknown")
        elif v == float("inf"):
            print(f"  {'SNR':<16} infinite (bit-identical)")
        else:
            print(f"  {'SNR':<16} {v:.2f} dB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
