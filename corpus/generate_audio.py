#!/usr/bin/env python3
"""Generate audio specimens deterministically, as pure-stdlib WAV.

Same discipline as the raster corpus: a script beats a committed blob because
it is reproducible, tiny in git, unambiguously ours, and regenerable.

WAV is written by hand because it is simple enough to, and because doing so
means the specimens do not inherit whatever an encoder felt like doing — the
point of a specimen is that you know exactly what is in it.

    python3 corpus/generate_audio.py
"""
import math
import struct
from pathlib import Path

OUT = Path(__file__).resolve().parent / "audio"
RATE = 44100


def write_wav(path: Path, channels: list[list[int]], rate: int, bits: int) -> None:
    """channels: one list of samples per channel, interleaved on write."""
    n = len(channels[0])
    nch = len(channels)
    width = bits // 8
    fmt = {16: "<h", 32: "<i"}[bits]

    frames = bytearray()
    for i in range(n):
        for ch in channels:
            frames += struct.pack(fmt, ch[i])

    data = bytes(frames)
    hdr = (b"RIFF" + struct.pack("<I", 36 + len(data)) + b"WAVE"
           + b"fmt " + struct.pack("<IHHIIHH", 16, 1, nch, rate,
                                   rate * nch * width, nch * width, bits)
           + b"data" + struct.pack("<I", len(data)))
    path.write_bytes(hdr + data)
    print(f"  {path.name:<26} {len(hdr)+len(data):>8} bytes  "
          f"{nch}ch {rate}Hz {bits}-bit {n/rate:.2f}s")


def tone(freq, seconds, rate, amp, bits):
    peak = 2 ** (bits - 1) - 1
    return [int(amp * peak * math.sin(2 * math.pi * freq * t / rate))
            for t in range(int(seconds * rate))]


def gen_stereo_distinct():
    """Left and right carry DIFFERENT tones.

    Exercises `audio_channels`: downmixing to mono is only observable if the
    channels differ. Identical channels would make a destructive downmix look
    lossless, the audio equivalent of testing alpha with an opaque image.
    """
    left = tone(440, 1.0, RATE, 0.7, 16)     # A4
    right = tone(659.25, 1.0, RATE, 0.7, 16)  # E5
    write_wav(OUT / "stereo-distinct.wav", [left, right], RATE, 16)


def gen_highfreq():
    """Content above the cutoff most lossy codecs apply.

    A 15 kHz tone survives WAV untouched and is attenuated or removed by
    perceptual codecs. Exercises whatever property lossy compression belongs
    to — which is the open question in docs/RESEARCH.md §13.7.
    """
    s = [a + b for a, b in zip(tone(1000, 1.0, RATE, 0.4, 16),
                               tone(15000, 1.0, RATE, 0.4, 16))]
    write_wav(OUT / "highfreq-mono.wav", [s], RATE, 16)


def gen_quiet_detail():
    """Very low amplitude, where quantisation noise is proportionally largest.

    16-bit to 8-bit is barely audible on loud content and destroys quiet
    content. Exercises `bit_depth` in the case where it actually matters.
    """
    write_wav(OUT / "quiet-mono.wav", [tone(440, 1.0, RATE, 0.01, 16)], RATE, 16)


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    print("generating audio specimens:")
    gen_stereo_distinct()
    gen_highfreq()
    gen_quiet_detail()
    print("done")
