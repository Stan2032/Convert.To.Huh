#!/usr/bin/env python3
"""Fetch the real-world specimen tier from Govdocs1, verified by SHA-256.

    python3 tools/fetch_govdocs.py            # download what MANIFEST.tsv lists
    python3 tools/fetch_govdocs.py --build    # (re)select members, rewrite MANIFEST.tsv

WHY
---
Every other specimen in corpus/ is synthetic and built to exercise one
property. That is how a loss is found; it cannot say how often the loss bites
on files people actually have. Govdocs1 (digitalcorpora.org) is ~1 million real
files from US government web servers, published as ten ~300 MB "thread" zips.

LICENSING
---------
Digital Corpora describes Govdocs1 as files that "are freely available for
research and may be (to the best of our knowledge) freely redistributed". That
is not CC0, so the files are NOT committed. The manifest is: member names,
sizes and SHA-256 digests, which are facts about the files. Anyone can fetch
the same bytes and check them against it.

HOW
---
Each Govdocs1 thread is a ~300 MB zip on S3. This reads the zip's central
directory and the selected members with HTTP Range requests, so fetching ~110
files downloads ~20 MB rather than the whole archive. Selection is
deterministic: the first N members of each type in name order.

Standard library only, like the rest of tools/.
"""
import argparse
import hashlib
import io
import sys
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIR = ROOT / "corpus" / "govdocs1"
MANIFEST = DIR / "MANIFEST.tsv"
FILES = DIR / "files"
BASE = "https://digitalcorpora.s3.amazonaws.com/corpora/files/govdocs1/threads/"

# Per-archive, per-type quotas for --build (None = every member of that type).
# Chosen by the name extension only to decide what to download; every specimen
# is identified by content when it is used. PNG is rare in Govdocs1 (3-6 per
# thread), so it is taken from all ten threads.
QUOTA = {"thread0.zip": {"pdf": 40, "jpg": 40, "gif": 27, "png": None,
                         "doc": 20, "xls": 20, "ppt": 20}}
QUOTA |= {f"thread{i}.zip": {"png": None} for i in range(1, 10)}


class HttpRangeFile(io.RawIOBase):
    """A read-only, seekable view of a remote file, one Range request per read."""

    def __init__(self, url: str):
        self.url = url
        self.pos = 0
        req = urllib.request.Request(url, method="HEAD")
        with urllib.request.urlopen(req, timeout=60) as r:
            self.size = int(r.headers["Content-Length"])

    def readable(self):
        return True

    def seekable(self):
        return True

    def tell(self):
        return self.pos

    def seek(self, offset, whence=0):
        self.pos = {0: offset, 1: self.pos + offset, 2: self.size + offset}[whence]
        return self.pos

    def readinto(self, buf):
        if self.pos >= self.size:
            return 0
        end = min(self.pos + len(buf), self.size) - 1
        req = urllib.request.Request(self.url, headers={"Range": f"bytes={self.pos}-{end}"})
        with urllib.request.urlopen(req, timeout=120) as r:
            data = r.read()
        buf[:len(data)] = data
        self.pos += len(data)
        return len(data)


def open_zip(archive: str) -> zipfile.ZipFile:
    return zipfile.ZipFile(io.BufferedReader(HttpRangeFile(BASE + archive), 1 << 20))


def build() -> int:
    picked = []
    counts: dict[str, int] = {}
    for archive, quota in QUOTA.items():
        z = open_zip(archive)
        taken = {k: 0 for k in quota}
        for info in sorted(z.infolist(), key=lambda i: i.filename):
            ext = info.filename.rsplit(".", 1)[-1].lower()
            if ext in quota and (quota[ext] is None or taken[ext] < quota[ext]):
                data = z.read(info)
                picked.append((archive, info.filename, len(data),
                               hashlib.sha256(data).hexdigest()))
                taken[ext] += 1
                counts[ext] = counts.get(ext, 0) + 1
    DIR.mkdir(parents=True, exist_ok=True)
    with MANIFEST.open("w") as f:
        f.write(f"# source\t{BASE}\n")
        f.write("# archive\tmember\tbytes\tsha256\n")
        for row in picked:
            f.write("\t".join(map(str, row)) + "\n")
    print(f"wrote {MANIFEST.relative_to(ROOT)}: {len(picked)} members {counts}")
    return 0


def rows() -> list[list[str]]:
    return [line.rstrip("\n").split("\t") for line in MANIFEST.read_text().splitlines()
            if line and not line.startswith("#")]


def fetch() -> int:
    listed = rows()
    FILES.mkdir(parents=True, exist_ok=True)
    zips: dict[str, zipfile.ZipFile] = {}
    bad = 0
    for archive, member, size, digest in listed:
        dest = FILES / Path(member).name
        if not dest.is_file():
            if archive not in zips:
                zips[archive] = open_zip(archive)
            dest.write_bytes(zips[archive].read(member))
        data = dest.read_bytes()
        if len(data) != int(size) or hashlib.sha256(data).hexdigest() != digest:
            print(f"  MISMATCH {member}", file=sys.stderr)
            dest.unlink()
            bad += 1
    print(f"{len(listed) - bad}/{len(listed)} specimens present and verified in "
          f"{FILES.relative_to(ROOT)}")
    return 1 if bad else 0


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--build", action="store_true", help="reselect members and rewrite the manifest")
    sys.exit(build() if ap.parse_args().build else fetch())
