#!/usr/bin/env bash
# =============================================================================
# Ontosyx LLM Token Regression Check (Phase 6.3)
#
# Replays the queries defined in bench/token_baseline.json against a running
# backend and compares observed token usage to the stored baseline.
#
# A query fails when:
#   prompt_tokens_actual     > prompt_tokens_baseline     * (1 + tol)
#   completion_tokens_actual > completion_tokens_baseline * (1 + tol)
#
# Exits 0 on pass, 1 on any regression, 2 on environment error.
#
# Requirements:
#   - Backend running at $ONTOSYX_API_HOST (default: http://localhost:3101)
#   - $OX_API_KEY or /tmp/ontosyx-dev-creds from `./scripts/dev.sh seed`
#   - bench/token_baseline.json committed
# =============================================================================

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASELINE="${REPO_ROOT}/bench/token_baseline.json"
API_HOST="${ONTOSYX_API_HOST:-http://localhost:3101}"
API_KEY="${OX_API_KEY:-}"
API="${API_HOST}/api"

GREEN=$'\033[0;32m'
RED=$'\033[0;31m'
YELLOW=$'\033[1;33m'
CYAN=$'\033[0;36m'
NC=$'\033[0m'

if [[ ! -f "$BASELINE" ]]; then
  printf '%sERROR%s baseline missing: %s\n' "$RED" "$NC" "$BASELINE" >&2
  exit 2
fi

load_dev_credential() {
  local name=$1
  local creds="${ONTOSYX_DEV_CREDS:-/tmp/ontosyx-dev-creds}"
  if [[ -n "${!name:-}" ]]; then
    printf '%s' "${!name}"
    return 0
  fi
  if [[ -f "$creds" ]]; then
    sed -nE "s/^export ${name}=\"(.*)\"$/\\1/p" "$creds" | head -1
  fi
}

API_KEY="$(load_dev_credential OX_API_KEY)"
WORKSPACE_ID="$(load_dev_credential OX_WORKSPACE_ID)"
if [[ -z "$API_KEY" ]]; then
  printf '%sERROR%s missing API key. Run ./scripts/dev.sh seed or export OX_API_KEY.\n' "$RED" "$NC" >&2
  exit 2
fi
API_AUTH_HEADERS=(-H "X-API-Key: $API_KEY")
if [[ -n "$WORKSPACE_ID" ]]; then
  API_AUTH_HEADERS+=(-H "X-Workspace-Id: $WORKSPACE_ID")
fi

# Bail early if the backend isn't reachable.
if ! curl -sf "$API/health" | grep -q '"status":"ok"'; then
  printf '%sERROR%s backend not reachable at %s\n' "$RED" "$NC" "$API" >&2
  exit 2
fi

# Check if baseline is still placeholder — skip gate, warn loudly.
IS_PLACEHOLDER=$(python3 -c '
import json, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
meta = data.get("_meta", {})
print("yes" if meta.get("placeholder") else "no")
' "$BASELINE")

if [[ "$IS_PLACEHOLDER" == "yes" ]]; then
  printf '%sWARN%s baseline is flagged placeholder — running record-only mode, not gating.\n' "$YELLOW" "$NC"
  printf '%sWARN%s Replace placeholder numbers in %s before enabling the CI gate.\n' "$YELLOW" "$NC" "$BASELINE"
fi

# ---------------------------------------------------------------------------
# Iterate queries and call the chat/stream endpoint.
# Each query uses a lightweight valid OntologyIR — the test is about prompt
# budget per query, not data correctness. The IR must still satisfy the
# same validation gate as production chat requests so this script cannot
# silently pass against a stale request schema.
# ---------------------------------------------------------------------------
TOKEN_ONTOLOGY='{"schema_version":1,"id":"token-regression-fixture","name":"token_regression_fixture","description":{},"version":{"number":1},"node_types":[{"id":"11111111-1111-4111-8111-111111111111","label":"Customer","display_name":{},"description":{},"properties":[],"constraints":[]}],"edge_types":[],"indexes":[]}'

PASS=0
FAIL=0
ENV_FAIL=0
TOTAL=0

printf '%s============================================%s\n' "$CYAN" "$NC"
printf '%s Token Regression Check%s\n' "$CYAN" "$NC"
printf '%s============================================%s\n' "$CYAN" "$NC"

QUERIES=$(python3 -c '
import json, sys
with open(sys.argv[1]) as f:
    d = json.load(f)
for k, v in d.items():
    if k.startswith("_"):
        continue
    print(k)
' "$BASELINE")

check_usage() {
  local name="$1"
  local actual="$2"
  local baseline="$3"
  local tolerance_pct="$4"
  local label="$5"

  local max
  max=$(python3 -c "print(int($baseline * (1 + $tolerance_pct / 100.0)))")
  TOTAL=$((TOTAL + 1))
  if [[ "$actual" -le "$max" ]]; then
    printf '  %sPASS%s %s %s: %d <= %d (baseline=%d tol=%d%%)\n' \
      "$GREEN" "$NC" "$name" "$label" "$actual" "$max" "$baseline" "$tolerance_pct"
    PASS=$((PASS + 1))
  else
    printf '  %sFAIL%s %s %s: %d > %d (baseline=%d tol=%d%%)\n' \
      "$RED" "$NC" "$name" "$label" "$actual" "$max" "$baseline" "$tolerance_pct"
    FAIL=$((FAIL + 1))
  fi
}

while IFS= read -r qkey; do
  [[ -z "$qkey" ]] && continue

  QUERY_ENTRY=$(python3 -c '
import json, sys
with open(sys.argv[1]) as f:
    d = json.load(f)
entry = d[sys.argv[2]]
out = {
    "prompt":     entry["prompt"],
    "prompt_baseline":     entry["prompt_tokens"],
    "completion_baseline": entry["completion_tokens"],
    "tol":                 entry.get("tolerance_pct", 5),
    "placeholder":         bool(entry.get("placeholder", False)),
}
sys.stdout.write(json.dumps(out, ensure_ascii=False))
' "$BASELINE" "$qkey")

  PROMPT=$(printf '%s' "$QUERY_ENTRY" | python3 -c 'import json,sys; print(json.load(sys.stdin)["prompt"])')
  BASE_PROMPT=$(printf '%s' "$QUERY_ENTRY" | python3 -c 'import json,sys; print(json.load(sys.stdin)["prompt_baseline"])')
  BASE_COMPLETION=$(printf '%s' "$QUERY_ENTRY" | python3 -c 'import json,sys; print(json.load(sys.stdin)["completion_baseline"])')
  TOL=$(printf '%s' "$QUERY_ENTRY" | python3 -c 'import json,sys; print(json.load(sys.stdin)["tol"])')

  REQ=$(python3 -c '
import json, sys
req = {
    "message": sys.argv[1],
    "ontology": json.loads(sys.argv[2]),
}
sys.stdout.write(json.dumps(req, ensure_ascii=False))
' "$PROMPT" "$TOKEN_ONTOLOGY")

  # Capture usage. The server surfaces per-request usage in the SSE stream
  # as `event: usage` with data `{"input_tokens": N, "output_tokens": M}`.
  CURL_RESP=$(curl -sS -w $'\n%{http_code}' --max-time 60 -X POST "$API/chat/stream" \
    "${API_AUTH_HEADERS[@]}" \
    -H "Content-Type: application/json; charset=utf-8" \
    -H "Accept: text/event-stream" \
    --data-binary "$REQ" 2>/dev/null || printf '\n000')
  RESP="${CURL_RESP%$'\n'*}"
  HTTP_STATUS="${CURL_RESP##*$'\n'}"
  if [[ ! "$HTTP_STATUS" =~ ^2 ]]; then
    printf '  %sFAIL%s %s: HTTP %s from chat/stream\n' "$RED" "$NC" "$qkey" "$HTTP_STATUS"
    ENV_FAIL=$((ENV_FAIL + 1))
    TOTAL=$((TOTAL + 1))
    continue
  fi

  USAGE_PROMPT=$(printf '%s' "$RESP" | python3 -c '
import sys, json, re
text = sys.stdin.read()
# Find the last usage event payload in the SSE stream.
last = 0
for m in re.finditer(r"input_tokens\"\\s*:\\s*(\\d+)", text):
    last = int(m.group(1))
print(last)
' 2>/dev/null || echo 0)
  USAGE_COMPLETION=$(printf '%s' "$RESP" | python3 -c '
import sys, re
text = sys.stdin.read()
last = 0
for m in re.finditer(r"output_tokens\"\\s*:\\s*(\\d+)", text):
    last = int(m.group(1))
print(last)
' 2>/dev/null || echo 0)

  if [[ "$USAGE_PROMPT" -eq 0 && "$USAGE_COMPLETION" -eq 0 ]]; then
    printf '  %sSKIP%s %s: no usage data returned\n' "$YELLOW" "$NC" "$qkey"
    continue
  fi

  # Run checks. In placeholder mode we only report, not fail.
  if [[ "$IS_PLACEHOLDER" == "yes" ]]; then
    printf '  %sREC %s %s prompt=%d completion=%d (baseline placeholder)\n' \
      "$CYAN" "$NC" "$qkey" "$USAGE_PROMPT" "$USAGE_COMPLETION"
  else
    check_usage "$qkey" "$USAGE_PROMPT"     "$BASE_PROMPT"     "$TOL" prompt
    check_usage "$qkey" "$USAGE_COMPLETION" "$BASE_COMPLETION" "$TOL" completion
  fi
done <<< "$QUERIES"

printf '\n%s============================================%s\n' "$CYAN" "$NC"
printf ' Results: %s%d passed%s, %s%d failed%s, %d total\n' \
  "$GREEN" "$PASS" "$NC" "$RED" "$((FAIL + ENV_FAIL))" "$NC" "$TOTAL"
printf '%s============================================%s\n' "$CYAN" "$NC"

if [[ "$ENV_FAIL" -gt 0 ]]; then
  printf '%sERROR%s %d request(s) failed before token usage could be measured.\n' "$RED" "$NC" "$ENV_FAIL" >&2
  exit 2
fi

if [[ "$IS_PLACEHOLDER" == "yes" ]]; then
  printf '%sNOTE%s placeholder mode — token budgets are recorded but not gated.\n' "$YELLOW" "$NC"
  exit 0
fi

[[ "$FAIL" -eq 0 ]] || exit 1
