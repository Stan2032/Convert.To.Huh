#!/usr/bin/env bash
# Measure libvips against the corpus for every raster pair it can do.
#
# capability-cartographer's instrument for the libvips edges. Deliberately a
# shell script rather than Rust: libvips is a subprocess here, and the point is
# to measure what the ACTUAL binary does, not what a binding claims.
#
# Usage: tools/measure_libvips.sh
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=/tmp/huh-libvips
mkdir -p "$OUT"

command -v vips >/dev/null || { echo "vips not found — apt-get install libvips-tools"; exit 1; }
echo "libvips $(vips --version 2>&1 | head -1)"
echo

printf '%-18s %-12s %9s %9s  %s\n' SPECIMEN TARGET IN OUT NOTE
for src in corpus/raster/*.png; do
  name=$(basename "$src" .png)
  for target in jpeg png webp tiff gif; do
    dst="$OUT/$name.$target"
    if vips "${target}save" "$src" "$dst" 2>/dev/null; then
      printf '%-18s %-12s %9s %9s  ok\n' "$name" "$target" \
        "$(stat -c%s "$src")" "$(stat -c%s "$dst")"
    else
      printf '%-18s %-12s %9s %9s  UNSUPPORTED\n' "$name" "$target" \
        "$(stat -c%s "$src")" "-"
    fi
  done
done
