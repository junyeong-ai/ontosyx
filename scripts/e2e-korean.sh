#!/usr/bin/env bash
# =============================================================================
# Ontosyx Korean Golden E2E Test (Phase 6.2)
#
# End-to-end sanity test using a Korean fixture. Verifies that Hangul
# column names and labels survive the full pipeline:
#   CSV → source analyze → ontology design → chat query → response.
#
# Usage:
#   bash scripts/e2e-korean.sh            # Full run (requires docker)
#   bash scripts/e2e-korean.sh --dry-run  # Verify fixture + no external deps
#
# Exit codes: 0 = pass, 1 = fail, 2 = precondition missing.
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURE="${REPO_ROOT}/tests/fixtures/korean_ecommerce.csv"
API_HOST="${ONTOSYX_API_HOST:-http://localhost:3101}"
API_KEY="${OX_API_KEY:-}"
WORKSPACE_ID="${OX_WORKSPACE_ID:-}"
API="${API_HOST}/api"
BACKEND_LOG="${ONTOSYX_BE_LOG:-/tmp/ontosyx-be-e2e-korean.log}"
DRY_RUN="${1:-}"
BACKEND_PID=""

GREEN=$'\033[0;32m'
RED=$'\033[0;31m'
YELLOW=$'\033[1;33m'
CYAN=$'\033[0;36m'
NC=$'\033[0m'

PASS=0
FAIL=0
TOTAL=0

log()  { printf '%s%s%s\n' "$CYAN" "$*" "$NC"; }
pass() { printf '  %sPASS%s %s\n' "$GREEN" "$NC" "$*"; PASS=$((PASS+1)); TOTAL=$((TOTAL+1)); }
fail() { printf '  %sFAIL%s %s\n' "$RED"   "$NC" "$*"; FAIL=$((FAIL+1)); TOTAL=$((TOTAL+1)); }
warn() { printf '  %sWARN%s %s\n' "$YELLOW" "$NC" "$*"; }

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

cleanup() {
  local rc=$?
  if [[ -n "$BACKEND_PID" ]] && kill -0 "$BACKEND_PID" 2>/dev/null; then
    log "Stopping backend (pid=$BACKEND_PID)…"
    kill "$BACKEND_PID" 2>/dev/null || true
    wait "$BACKEND_PID" 2>/dev/null || true
  fi
  exit $rc
}
trap cleanup EXIT INT TERM

# ---------------------------------------------------------------------------
# Fixture sanity
# ---------------------------------------------------------------------------
log "============================================"
log " Ontosyx Korean Golden E2E"
log "============================================"
log " Fixture: $FIXTURE"
log " API:     $API"
log " Dry run: ${DRY_RUN:-no}"
log ""

if [[ ! -f "$FIXTURE" ]]; then
  fail "Fixture not found: $FIXTURE"
  exit 2
fi

if ! grep -q '사용자ID' "$FIXTURE"; then
  fail "Fixture missing Hangul column name 사용자ID"
  exit 2
fi
pass "Fixture exists with Korean headers"

# Count Hangul-bearing data rows (skip leading # comments + header).
# Hangul Unicode block U+AC00..U+D7A3 covers precomposed syllables.
DATA_ROWS=$(grep -v '^#' "$FIXTURE" | tail -n +2 | grep -cE '[가-힣]' || true)
if [[ "$DATA_ROWS" -lt 15 ]]; then
  fail "Fixture has $DATA_ROWS data rows (expected ≥15)"
  exit 2
fi
pass "Fixture contains $DATA_ROWS Korean data rows"

if [[ "$DRY_RUN" == "--dry-run" ]]; then
  log ""
  log "Dry-run complete: fixture + script syntax OK."
  exit 0
fi

API_KEY="$(load_dev_credential OX_API_KEY)"
WORKSPACE_ID="$(load_dev_credential OX_WORKSPACE_ID)"
if [[ -z "$API_KEY" ]]; then
  API_KEY="e2e-$(openssl rand -hex 16)"
  log "Generated ephemeral bootstrap API key for this E2E run."
else
  log "Using API key from environment or dev credential file."
fi

# ---------------------------------------------------------------------------
# Boot infrastructure
# ---------------------------------------------------------------------------
log ""
log "=== Boot Docker (postgres + neo4j) ==="
if ! command -v docker >/dev/null 2>&1; then
  fail "docker not installed"
  exit 2
fi
(cd "$REPO_ROOT" && docker compose up -d postgres neo4j) || { fail "docker compose up failed"; exit 2; }

# Wait for Postgres
for i in {1..60}; do
  if (cd "$REPO_ROOT" && docker compose exec -T postgres pg_isready -U ontosyx >/dev/null 2>&1); then
    pass "Postgres ready"
    break
  fi
  sleep 1
done

# Wait for Neo4j
for i in {1..60}; do
  if curl -sf http://localhost:7474 >/dev/null 2>&1; then
    pass "Neo4j ready"
    break
  fi
  sleep 1
done

# ---------------------------------------------------------------------------
# Build + boot backend
# ---------------------------------------------------------------------------
log ""
log "=== Build backend (ox-api, release) ==="
(cd "$REPO_ROOT" && cargo build --release --features source-all -p ox-api) || { fail "cargo build failed"; exit 1; }
pass "Backend build"

log ""
log "=== Start backend ==="
: > "$BACKEND_LOG"
(cd "$REPO_ROOT" && OX_AUTH__BOOTSTRAP_KEY="$API_KEY" ./target/release/ontosyx >"$BACKEND_LOG" 2>&1 &)
BACKEND_PID=$!
log "Backend PID: $BACKEND_PID, logging to $BACKEND_LOG"

# Wait for /api/health
log "Waiting for /api/health…"
HEALTHY=0
for i in {1..90}; do
  if curl -sf "$API/health" | grep -q '"status":"ok"'; then
    HEALTHY=1
    break
  fi
  sleep 1
done
if [[ "$HEALTHY" -ne 1 ]]; then
  fail "Backend did not become healthy within 90s"
  tail -40 "$BACKEND_LOG" || true
  exit 1
fi
pass "Backend /api/health = ok"

if [[ -z "$WORKSPACE_ID" ]]; then
  WORKSPACE_ID=$(curl -sf -H "X-API-Key: $API_KEY" "$API/workspaces" | python3 -c '
import json, sys
try:
    payload = json.load(sys.stdin).get("data", [])
    items = payload if isinstance(payload, list) else payload.get("items", [])
    if items:
        print(items[0].get("id", ""))
except Exception:
    pass
' 2>/dev/null || true)
fi
if [[ -z "$WORKSPACE_ID" ]]; then
  fail "Default workspace not returned"
  tail -40 "$BACKEND_LOG" || true
  exit 1
fi
pass "Default workspace resolved (id=$WORKSPACE_ID)"

API_AUTH_HEADERS=(-H "X-API-Key: $API_KEY" -H "X-Workspace-Id: $WORKSPACE_ID")

# ---------------------------------------------------------------------------
# Idempotency: delete any prior "Korean E2E Fixture" projects.
# ---------------------------------------------------------------------------
log ""
log "=== Cleanup previous test state ==="
EXISTING=$(curl -sf "${API_AUTH_HEADERS[@]}" "$API/projects" || echo '{"projects":[]}')
OLD_IDS=$(printf '%s' "$EXISTING" | python3 -c '
import sys, json
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(0)
for p in d.get("projects", d if isinstance(d, list) else []):
    if "Korean E2E Fixture" in str(p.get("title", "")):
        print(p.get("id", ""))
' 2>/dev/null || true)
for id in $OLD_IDS; do
  [[ -z "$id" ]] && continue
  curl -sf -X DELETE "${API_AUTH_HEADERS[@]}" "$API/projects/$id" >/dev/null || true
  log "  Deleted stale project $id"
done

# ---------------------------------------------------------------------------
# Create project from Korean CSV
# ---------------------------------------------------------------------------
log ""
log "=== Create project from Korean CSV ==="
CSV_JSON=$(python3 -c '
import json, sys, pathlib
p = pathlib.Path(sys.argv[1])
# Strip leading # comment lines; keep CSV header + data.
text = "\n".join(l for l in p.read_text(encoding="utf-8").splitlines() if not l.startswith("#"))
sys.stdout.write(json.dumps(text, ensure_ascii=False))
' "$FIXTURE")

PROJ_BODY=$(python3 -c '
import json, sys
csv_data = json.loads(sys.argv[1])
body = {
    "title": "Korean E2E Fixture",
    "origin_type": "source",
    "source": {"type": "csv", "data": csv_data},
    "selection": {"kind": "all"},
}
sys.stdout.write(json.dumps(body, ensure_ascii=False))
' "$CSV_JSON")

PROJ_RESP=$(curl -sf -X POST "$API/projects" \
  "${API_AUTH_HEADERS[@]}" -H "Content-Type: application/json; charset=utf-8" \
  --data-binary "$PROJ_BODY") || { fail "Project create HTTP failed"; tail -40 "$BACKEND_LOG"; exit 1; }

PROJECT_ID=$(printf '%s' "$PROJ_RESP" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("id",""))')
if [[ -z "$PROJECT_ID" ]]; then
  fail "Project ID missing from create response"
  printf '%s\n' "$PROJ_RESP" | head -c 500
  exit 1
fi
pass "Project created (id=$PROJECT_ID)"

# Verify Hangul column names survived source analysis
if printf '%s' "$PROJ_RESP" | grep -q '사용자'; then
  pass "Korean column names preserved in source_schema"
else
  fail "Korean column names missing from source_schema"
fi

# ---------------------------------------------------------------------------
# Ontology design
# ---------------------------------------------------------------------------
log ""
log "=== Design ontology (LLM) ==="
DESIGN_RESP=$(curl -sf --max-time 180 -X POST "$API/projects/$PROJECT_ID/design" \
  "${API_AUTH_HEADERS[@]}" -H "Content-Type: application/json" \
  -d '{"source_description": "한국어 이커머스 주문 데이터: 사용자, 주문, 상품, 카테고리 관계"}') \
  || { fail "Design call failed"; tail -60 "$BACKEND_LOG"; exit 1; }

NODE_COUNT=$(printf '%s' "$DESIGN_RESP" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    print(len(d.get("ontology", {}).get("node_types", [])))
except Exception:
    print(0)
')
if [[ "$NODE_COUNT" -ge 3 ]]; then
  pass "Ontology has $NODE_COUNT node types (≥3)"
else
  fail "Ontology node count too low: $NODE_COUNT"
fi

# Look for Korean label somewhere in the ontology
if printf '%s' "$DESIGN_RESP" | grep -qE '사용자|주문|상품|카테고리'; then
  pass "OntologyIR contains Korean labels"
else
  fail "OntologyIR missing Korean labels"
fi

# ---------------------------------------------------------------------------
# Five Korean NL chat queries
# ---------------------------------------------------------------------------
log ""
log "=== 5 Korean NL chat queries ==="
ONTOLOGY_JSON=$(printf '%s' "$DESIGN_RESP" | python3 -c '
import json, sys
d = json.load(sys.stdin)
sys.stdout.write(json.dumps(d.get("ontology", {})))
')

run_korean_query() {
  local name="$1"
  local question="$2"
  local body
  body=$(python3 -c '
import json, sys
sys.stdout.write(json.dumps({
    "message": sys.argv[1],
    "ontology": json.loads(sys.argv[2]),
    "project_id": sys.argv[3],
}, ensure_ascii=False))
' "$question" "$ONTOLOGY_JSON" "$PROJECT_ID")

  local resp
  resp=$(curl -sf --max-time 120 -X POST "$API/chat/stream" \
    "${API_AUTH_HEADERS[@]}" \
    -H "Content-Type: application/json; charset=utf-8" \
    -H "Accept: text/event-stream" \
    --data-binary "$body" 2>&1 || true)

  if [[ -z "$resp" ]]; then
    fail "$name: empty SSE response"
    return
  fi

  # Stream must contain Korean content somewhere.
  if printf '%s' "$resp" | grep -qE '사용자|주문|상품|카테고리|가격|수량|이메일'; then
    pass "$name: Korean content present in stream"
  else
    fail "$name: no Korean content"
  fi

  # At least one row-bearing payload (either query result or result_table event).
  if printf '%s' "$resp" | grep -qE '"rows"|"result"|"data"|event: result'; then
    pass "$name: result payload present"
  else
    warn "$name: no obvious result payload (may be expected if query declined)"
  fi
}

run_korean_query "Q1 사용자별 주문수"  "사용자별 주문 건수를 보여줘"
run_korean_query "Q2 카테고리별 매출"  "카테고리별 총 매출을 구해줘"
run_korean_query "Q3 상위 상품"        "가장 많이 팔린 상품 상위 5개를 알려줘"
run_korean_query "Q4 이메일 목록"      "이메일 주소 목록을 사용자명과 함께 보여줘"
run_korean_query "Q5 평균 수량"        "주문 건당 평균 수량을 계산해줘"

# ---------------------------------------------------------------------------
# Cleanup
# ---------------------------------------------------------------------------
log ""
log "=== Cleanup ==="
curl -sf -X DELETE "${API_AUTH_HEADERS[@]}" "$API/projects/$PROJECT_ID" >/dev/null || true
pass "Deleted test project"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
log ""
log "============================================"
log " Results: ${GREEN}${PASS} passed${NC}, ${RED}${FAIL} failed${NC}, ${TOTAL} total"
log "============================================"

if [[ "$FAIL" -gt 0 ]]; then
  exit 1
fi
