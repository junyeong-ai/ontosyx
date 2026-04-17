#!/usr/bin/env bash
# =============================================================================
# Ontosyx Criterion Bench Regression Check (Phase 6.5)
#
# Parses bencher-format output from `cargo bench -- --output-format bencher`
# and compares each bench's ns/iter to the baseline in bench/baseline.json.
#
# Exits 0 on pass, 1 on regression > tolerance, 2 on env error.
#
# Usage: check-bench-regression.sh <bench-output.txt> <baseline.json>
# =============================================================================

set -euo pipefail

BENCH_OUTPUT="${1:-bench-output.txt}"
BASELINE="${2:-bench/baseline.json}"

GREEN=$'\033[0;32m'
RED=$'\033[0;31m'
YELLOW=$'\033[1;33m'
CYAN=$'\033[0;36m'
NC=$'\033[0m'

if [[ ! -f "$BENCH_OUTPUT" ]]; then
  printf '%sERROR%s bench output not found: %s\n' "$RED" "$NC" "$BENCH_OUTPUT" >&2
  exit 2
fi
if [[ ! -f "$BASELINE" ]]; then
  printf '%sERROR%s baseline not found: %s\n' "$RED" "$NC" "$BASELINE" >&2
  exit 2
fi

IS_PLACEHOLDER=$(python3 -c '
import json, sys
with open(sys.argv[1]) as f:
    meta = json.load(f).get("_meta", {})
print("yes" if meta.get("placeholder") else "no")
' "$BASELINE")

TOLERANCE_PCT=$(python3 -c '
import json, sys
with open(sys.argv[1]) as f:
    meta = json.load(f).get("_meta", {})
print(meta.get("tolerance_pct", 3))
' "$BASELINE")

if [[ "$IS_PLACEHOLDER" == "yes" ]]; then
  printf '%sWARN%s baseline is placeholder — running in report-only mode.\n' \
    "$YELLOW" "$NC"
fi

printf '%s============================================%s\n' "$CYAN" "$NC"
printf '%s Bench Regression Check (tol=%s%%)%s\n' "$CYAN" "$TOLERANCE_PCT" "$NC"
printf '%s============================================%s\n' "$CYAN" "$NC"

PASS=0
FAIL=0
TOTAL=0

# Parse bencher-format lines. Format:
#   test BENCH_NAME ... bench:       1330 ns/iter (+/- 42)
while IFS= read -r line; do
  # shellcheck disable=SC2001
  # Only handle bencher-format lines.
  if [[ "$line" != test* || "$line" != *ns/iter* ]]; then
    continue
  fi
  name=$(echo "$line" | sed -E 's/^test[[:space:]]+([^[:space:]]+).*$/\1/')
  # Strip commas from the ns-per-iter number (Criterion bencher uses "1,303").
  ns=$(echo "$line" | sed -E 's/.*bench:[[:space:]]+([0-9,]+)[[:space:]]+ns\/iter.*$/\1/' | tr -d ',')
  if [[ -z "$name" || -z "$ns" || "$ns" == "$line" ]]; then
    continue
  fi

  BASE=$(python3 -c '
import json, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
entry = data.get(sys.argv[2])
print(entry["median_ns"] if entry else 0)
' "$BASELINE" "$name")

  if [[ "$BASE" == "0" ]]; then
    printf '  %sSKIP%s %s: no baseline entry\n' "$YELLOW" "$NC" "$name"
    continue
  fi

  MAX=$(python3 -c "print(int($BASE * (1 + $TOLERANCE_PCT / 100.0)))")
  TOTAL=$((TOTAL + 1))

  if [[ "$ns" -le "$MAX" ]]; then
    printf '  %sPASS%s %s: %d ns/iter <= %d (baseline=%d)\n' \
      "$GREEN" "$NC" "$name" "$ns" "$MAX" "$BASE"
    PASS=$((PASS + 1))
  else
    printf '  %sFAIL%s %s: %d ns/iter > %d (baseline=%d, +%d%%)\n' \
      "$RED" "$NC" "$name" "$ns" "$MAX" "$BASE" \
      "$(python3 -c "print(int(100*($ns/$BASE - 1)))")"
    FAIL=$((FAIL + 1))
  fi
done < "$BENCH_OUTPUT"

printf '\n%s============================================%s\n' "$CYAN" "$NC"
printf ' Results: %s%d passed%s, %s%d failed%s, %d total\n' \
  "$GREEN" "$PASS" "$NC" "$RED" "$FAIL" "$NC" "$TOTAL"
printf '%s============================================%s\n' "$CYAN" "$NC"

if [[ "$IS_PLACEHOLDER" == "yes" ]]; then
  printf '%sNOTE%s placeholder mode — exit 0 regardless of regressions.\n' \
    "$YELLOW" "$NC"
  exit 0
fi

[[ "$FAIL" -eq 0 ]] || exit 1
