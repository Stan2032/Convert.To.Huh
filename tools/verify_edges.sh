#!/usr/bin/env bash
# Run every corpus specimen through an edge and report measured fidelity.
#
# This is how a capability edge earns `status = "verified"`: the registry
# schema (rule 5) refuses the status without a `measured` block, and this
# produces it. A claim copied from a README is not a capability.
#
#   ./tools/verify_edges.sh [quality]
set -euo pipefail

cd "$(dirname "$0")/.."
HUH=core/target/release/huh
[ -x "$HUH" ] || { echo "build first: (cd core && cargo build --release)"; exit 1; }

Q="${1:-85}"
OUT=$(mktemp -d)
trap 'rm -rf "$OUT"' EXIT

echo "edge: image/png -> image/jpeg via image-rs  (quality=$Q)"
echo
printf "  %-22s %-10s %-9s %s\n" SPECIMEN SSIM BYTES PER-PROPERTY
printf "  %-22s %-10s %-9s %s\n" ---------------------- ---------- --------- ------------
fail=0
ssim_sum=0; ssim_n=0; ssim_min=1

for f in corpus/raster/*.png; do
  n=$(basename "$f" .png)
  if ! "$HUH" convert "$f" "$OUT/$n.jpg" --quality "$Q" --receipt "$OUT/$n.json" >/dev/null 2>"$OUT/$n.err"; then
    if grep -q "CONTRADICTS" "$OUT/$n.err"; then fail=1; fi
  fi
  read -r ssim props < <(python3 - "$OUT/$n.json" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
s = r.get("ssim")
props = ",".join(f"{m['property']}={m['measured']}" for m in r["measurements"])
print(f"{s:.4f}" if s is not None else "n/a", props)
PY
)
  sz=$(stat -c%s "$OUT/$n.jpg")
  printf "  %-22s %-10s %-9s %s\n" "$n" "$ssim" "$sz" "$props"
  ssim_sum=$(python3 -c "print($ssim_sum + $ssim)")
  ssim_n=$((ssim_n+1))
  ssim_min=$(python3 -c "print(min($ssim_min, $ssim))")
done

echo
python3 -c "print(f'  mean SSIM {$ssim_sum/$ssim_n:.4f}   min SSIM {$ssim_min:.4f}   specimens {$ssim_n}')"
echo
if [ "$fail" -ne 0 ]; then
  echo "  RESULT: a measurement CONTRADICTS the registry — that is a registry bug"
  echo "          for capability-cartographer, not a conversion failure."
  exit 4
fi
echo "  RESULT: every measurement agrees with the declared loss vector."
echo "          Copy the numbers above into [edge.verification.measured]."
