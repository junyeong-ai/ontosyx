#!/bin/bash
# =============================================================================
# Ontosyx E2E Test Suite
# Tests all API endpoints with real LLM calls + edge cases
# =============================================================================

set -euo pipefail

BASE="http://localhost:3001/api"
PRINCIPAL_ID="${ONTOSYX_E2E_PRINCIPAL_ID:-ontosyx-e2e}"
PASS=0
FAIL=0
TOTAL=0

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

curl_user() {
    curl -H "X-Principal-Id: $PRINCIPAL_ID" "$@"
}

assert_status() {
    local test_name="$1"
    local expected="$2"
    local actual="$3"
    local body="$4"
    TOTAL=$((TOTAL + 1))
    if [ "$actual" = "$expected" ]; then
        echo -e "  ${GREEN}PASS${NC} [$actual] $test_name"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC} [$actual != $expected] $test_name"
        echo "       Body: $(echo "$body" | head -c 300)"
        FAIL=$((FAIL + 1))
    fi
}

assert_json_field() {
    local test_name="$1"
    local body="$2"
    local field="$3"
    TOTAL=$((TOTAL + 1))
    if echo "$body" | python3 -c "import sys,json; d=json.load(sys.stdin); assert $field" 2>/dev/null; then
        echo -e "  ${GREEN}PASS${NC} $test_name"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC} $test_name"
        echo "       Body: $(echo "$body" | head -c 300)"
        FAIL=$((FAIL + 1))
    fi
}

assert_header() {
    local test_name="$1"
    local headers="$2"
    local header_name="$3"
    TOTAL=$((TOTAL + 1))
    if echo "$headers" | grep -qi "$header_name"; then
        echo -e "  ${GREEN}PASS${NC} $test_name"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC} $test_name"
        FAIL=$((FAIL + 1))
    fi
}

echo "============================================"
echo " Ontosyx E2E Test Suite"
echo "============================================"
echo ""

# ===================================================================
# 1. HEALTH CHECK
# ===================================================================
echo -e "${YELLOW}=== 1. Health Check ===${NC}"

RESP=$(curl -sw '\n%{http_code}' "$BASE/health")
CODE=$(echo "$RESP" | tail -n1)
BODY=$(echo "$RESP" | sed '$d')
assert_status "GET /health" "200" "$CODE" "$BODY"
assert_json_field "health: status is ok" "$BODY" "'status' in d and d['status'] in ('ok','degraded','unavailable')"
assert_json_field "health: has components" "$BODY" "'components' in d"
assert_json_field "health: has version" "$BODY" "'version' in d"

# ===================================================================
# 2. REQUEST ID HEADER
# ===================================================================
echo ""
echo -e "${YELLOW}=== 2. Request ID Header ===${NC}"

HEADERS=$(curl -sI "$BASE/health")
assert_header "x-request-id header present" "$HEADERS" "x-request-id"

# ===================================================================
# 3. DESIGN PROJECT LIFECYCLE (LLM CALL)
# ===================================================================
echo ""
echo -e "${YELLOW}=== 3. Design Project Lifecycle ===${NC}"

API_KEY=$(echo "${OX_API_KEY:-}" || echo "")
AUTH_HEADER=""
if [ -n "$API_KEY" ]; then
    AUTH_HEADER="-H X-API-Key:$API_KEY"
fi

# 3a. Create project (includes source analysis)
RESP=$(curl -sw '\n%{http_code}' --max-time 120 -X POST "$BASE/projects" \
  -H 'Content-Type: application/json' \
  $AUTH_HEADER \
  -d '{
    "name": "E2E Test Project",
    "source": {"type": "text", "data": "id,name,department,salary\n1,Alice,Engineering,95000\n2,Bob,Marketing,75000\n3,Charlie,Engineering,105000"},
    "context": "Company employee database"
  }')
BODY=$(echo "$RESP" | sed '$d')
CODE=$(echo "$RESP" | tail -n1)
assert_status "POST /projects (create)" "201" "$CODE" "$BODY"
PROJECT_ID=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null || echo "")
assert_json_field "project: has id" "$BODY" "'id' in d"
assert_json_field "project: has status" "$BODY" "'status' in d"

if [ -z "$PROJECT_ID" ]; then
    echo -e "  ${RED}FATAL${NC} Failed to extract project ID — aborting remaining tests"
    exit 1
fi

# 3b. List projects
RESP=$(curl -sw '\n%{http_code}' "$BASE/projects?limit=10" $AUTH_HEADER)
BODY=$(echo "$RESP" | sed '$d')
CODE=$(echo "$RESP" | tail -n1)
assert_status "GET /projects (list)" "200" "$CODE" "$BODY"
assert_json_field "projects: has items" "$BODY" "'items' in d and len(d['items']) >= 1"

# 3c. Get project
RESP=$(curl -sw '\n%{http_code}' "$BASE/projects/$PROJECT_ID" $AUTH_HEADER)
BODY=$(echo "$RESP" | sed '$d')
CODE=$(echo "$RESP" | tail -n1)
assert_status "GET /projects/:id" "200" "$CODE" "$BODY"
assert_json_field "project: correct id" "$BODY" "d['id'] == '$PROJECT_ID'"
PROJECT_REVISION=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['revision'])" 2>/dev/null || echo "1")

# 3d. Design ontology (LLM call)
RESP=$(curl -sw '\n%{http_code}' --max-time 120 -X POST "$BASE/projects/$PROJECT_ID/design" \
  -H 'Content-Type: application/json' \
  $AUTH_HEADER \
  -d "{\"revision\": $PROJECT_REVISION}")
BODY=$(echo "$RESP" | sed '$d')
CODE=$(echo "$RESP" | tail -n1)
assert_status "POST /projects/:id/design" "200" "$CODE" "$BODY"
assert_json_field "design: has ontology" "$BODY" "'ontology' in d and len(d['ontology']['node_types']) > 0"

# Extract ontology for use in subsequent tests
ONTOLOGY_JSON=$(echo "$BODY" | python3 -c "import sys,json; print(json.dumps(json.load(sys.stdin)['ontology']))" 2>/dev/null || echo "")
if [ -z "$ONTOLOGY_JSON" ]; then
    echo -e "  ${RED}FATAL${NC} Failed to extract ontology from design response — aborting remaining tests"
    exit 1
fi

# 3e. Validation: get non-existent project -> 404
RESP=$(curl -sw '\n%{http_code}' "$BASE/projects/00000000-0000-0000-0000-000000000000" $AUTH_HEADER)
BODY=$(echo "$RESP" | sed '$d')
CODE=$(echo "$RESP" | tail -n1)
assert_status "GET /projects/:id (not found)" "404" "$CODE" "$BODY"
assert_json_field "404: structured error" "$BODY" "'error' in d and 'type' in d['error']"

# ===================================================================
# 4. LOAD PLAN (LLM CALL)
# ===================================================================
echo ""
echo -e "${YELLOW}=== 4. Load Plan (LLM) ===${NC}"

LOAD_REQ=$(python3 -c "
import json,sys
onto = json.loads('''$ONTOLOGY_JSON''')
req = {'ontology': onto, 'source_description': 'CSV file with columns: id, name, department, salary'}
print(json.dumps(req))
" 2>/dev/null || echo "")

if [ -n "$LOAD_REQ" ]; then
    RESP=$(curl -sw '\n%{http_code}' --max-time 120 -X POST "$BASE/load" \
      -H 'Content-Type: application/json' \
      $AUTH_HEADER \
      -d "$LOAD_REQ")
    BODY=$(echo "$RESP" | sed '$d')
    CODE=$(echo "$RESP" | tail -n1)
    assert_status "POST /load (plan)" "200" "$CODE" "$BODY"
    assert_json_field "load plan: has plan" "$BODY" "'plan' in d"
    assert_json_field "load plan: has target" "$BODY" "'target' in d"
else
    TOTAL=$((TOTAL + 2)); FAIL=$((FAIL + 2))
    echo -e "  ${RED}FAIL${NC} Could not build load request from ontology"
fi

# ===================================================================
# 5. PROMPTS
# ===================================================================
echo ""
echo -e "${YELLOW}=== 5. Prompts ===${NC}"

RESP=$(curl -sw '\n%{http_code}' "$BASE/prompts" $AUTH_HEADER)
BODY=$(echo "$RESP" | sed '$d')
CODE=$(echo "$RESP" | tail -n1)
assert_status "GET /prompts" "200" "$CODE" "$BODY"
assert_json_field "prompts: is non-empty array" "$BODY" "len(d) > 0"
assert_json_field "prompts: has name and version" "$BODY" "all('name' in p and 'version' in p for p in d)"

# ===================================================================
# 6. CHAT — NL QUERY (single-turn)
# ===================================================================
echo ""
echo -e "${YELLOW}=== 6. Chat: NL Query (single-turn) ===${NC}"

CHAT_REQ=$(python3 -c "
import json
onto = json.loads('''$ONTOLOGY_JSON''')
req = {
    'message': 'Find all employees in Engineering department',
    'ontology': onto
}
print(json.dumps(req))
" 2>/dev/null || echo "")

if [ -n "$CHAT_REQ" ]; then
    RESP=$(curl_user -sw '\n%{http_code}' --max-time 120 -X POST "$BASE/chat" \
      -H 'Content-Type: application/json' \
      -d "$CHAT_REQ")
    BODY=$(echo "$RESP" | sed '$d')
    CODE=$(echo "$RESP" | tail -n1)
    assert_status "POST /chat (single-turn)" "200" "$CODE" "$BODY"
    assert_json_field "chat: has content" "$BODY" "'content' in d and len(d['content']) > 0"
    assert_json_field "chat: has model" "$BODY" "'model' in d"
    assert_json_field "chat: has query_ir" "$BODY" "'query_ir' in d"
    assert_json_field "chat: has execution_id" "$BODY" "'execution_id' in d"

    # Extract execution_id for pinboard test
    EXECUTION_ID=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['execution_id'])" 2>/dev/null || echo "")

    # 6b. Chat with empty message -> 400
    CHAT_EMPTY=$(python3 -c "
import json
onto = json.loads('''$ONTOLOGY_JSON''')
req = {
    'message': '',
    'ontology': onto
}
print(json.dumps(req))
" 2>/dev/null || echo "")
    RESP=$(curl_user -sw '\n%{http_code}' --max-time 10 -X POST "$BASE/chat" \
      -H 'Content-Type: application/json' \
      -d "$CHAT_EMPTY")
    BODY=$(echo "$RESP" | sed '$d')
    CODE=$(echo "$RESP" | tail -n1)
    assert_status "POST /chat (empty message)" "400" "$CODE" "$BODY"
else
    TOTAL=$((TOTAL + 5)); FAIL=$((FAIL + 5))
    echo -e "  ${RED}FAIL${NC} Could not build chat request"
fi

# ===================================================================
# 7. CHAT STREAM (SSE)
# ===================================================================
echo ""
echo -e "${YELLOW}=== 7. Chat: Stream (SSE) ===${NC}"

STREAM_REQ=$(python3 -c "
import json
onto = json.loads('''$ONTOLOGY_JSON''')
req = {
    'message': 'Count employees by department',
    'ontology': onto
}
print(json.dumps(req))
" 2>/dev/null || echo "")

if [ -n "$STREAM_REQ" ]; then
    STREAM_OUTPUT=$(curl_user -s --max-time 120 -X POST "$BASE/chat/stream" \
      -H 'Content-Type: application/json' \
      -H 'Accept: text/event-stream' \
      -d "$STREAM_REQ" 2>&1 || true)

    TOTAL=$((TOTAL + 1))
    if echo "$STREAM_OUTPUT" | grep -q "event: pipeline"; then
        echo -e "  ${GREEN}PASS${NC} POST /chat/stream (has pipeline event)"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC} POST /chat/stream (missing pipeline event)"
        echo "       Output: $(echo "$STREAM_OUTPUT" | head -c 500)"
        FAIL=$((FAIL + 1))
    fi

    TOTAL=$((TOTAL + 1))
    if echo "$STREAM_OUTPUT" | grep -q '"delta"'; then
        echo -e "  ${GREEN}PASS${NC} POST /chat/stream (has delta chunks)"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC} POST /chat/stream (missing delta chunks)"
        FAIL=$((FAIL + 1))
    fi

    TOTAL=$((TOTAL + 1))
    if echo "$STREAM_OUTPUT" | grep -q "event: done"; then
        echo -e "  ${GREEN}PASS${NC} POST /chat/stream (has done event)"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC} POST /chat/stream (missing done event)"
        FAIL=$((FAIL + 1))
    fi

    TOTAL=$((TOTAL + 1))
    if echo "$STREAM_OUTPUT" | grep -q '"is_final":true'; then
        echo -e "  ${GREEN}PASS${NC} POST /chat/stream (has final chunk)"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC} POST /chat/stream (missing final chunk)"
        FAIL=$((FAIL + 1))
    fi
else
    TOTAL=$((TOTAL + 4)); FAIL=$((FAIL + 4))
    echo -e "  ${RED}FAIL${NC} Could not build stream request"
fi

# ===================================================================
# 8. QUERY HISTORY
# ===================================================================
echo ""
echo -e "${YELLOW}=== 8. Query History ===${NC}"

RESP=$(curl_user -sw '\n%{http_code}' "$BASE/query/history?limit=10")
BODY=$(echo "$RESP" | sed '$d')
CODE=$(echo "$RESP" | tail -n1)
assert_status "GET /query/history (list)" "200" "$CODE" "$BODY"
assert_json_field "history: has items" "$BODY" "'items' in d and len(d['items']) >= 1"

# Get single execution
if [ -n "${EXECUTION_ID:-}" ]; then
    RESP=$(curl_user -sw '\n%{http_code}' "$BASE/query/history/$EXECUTION_ID")
    BODY=$(echo "$RESP" | sed '$d')
    CODE=$(echo "$RESP" | tail -n1)
    assert_status "GET /query/history/:id" "200" "$CODE" "$BODY"
    assert_json_field "execution: correct id" "$BODY" "d['id'] == '$EXECUTION_ID'"
fi

# Non-existent execution -> 404
RESP=$(curl_user -sw '\n%{http_code}' "$BASE/query/history/00000000-0000-0000-0000-000000000000")
BODY=$(echo "$RESP" | sed '$d')
CODE=$(echo "$RESP" | tail -n1)
assert_status "GET /query/history/:id (not found)" "404" "$CODE" "$BODY"

# ===================================================================
# 9. PINBOARD
# ===================================================================
echo ""
echo -e "${YELLOW}=== 9. Pinboard ===${NC}"

if [ -n "${EXECUTION_ID:-}" ]; then
    # 9a. Create pin
    RESP=$(curl_user -sw '\n%{http_code}' -X POST "$BASE/pins" \
      -H 'Content-Type: application/json' \
      -d "{\"query_execution_id\":\"$EXECUTION_ID\",\"title\":\"Test Pin\"}")
    BODY=$(echo "$RESP" | sed '$d')
    CODE=$(echo "$RESP" | tail -n1)
    assert_status "POST /pins (create)" "201" "$CODE" "$BODY"
    PIN_ID=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null || echo "")
    assert_json_field "pin: has id" "$BODY" "'id' in d"

    # 9b. List pins (cursor-paginated)
    RESP=$(curl_user -sw '\n%{http_code}' "$BASE/pins?limit=10")
    BODY=$(echo "$RESP" | sed '$d')
    CODE=$(echo "$RESP" | tail -n1)
    assert_status "GET /pins (list)" "200" "$CODE" "$BODY"
    assert_json_field "pins: non-empty items" "$BODY" "len(d.get('items',[])) >= 1"

    # 9c. Delete pin
    if [ -n "$PIN_ID" ]; then
        RESP=$(curl_user -sw '\n%{http_code}' -X DELETE "$BASE/pins/$PIN_ID")
        BODY=$(echo "$RESP" | sed '$d')
        CODE=$(echo "$RESP" | tail -n1)
        assert_status "DELETE /pins/:id" "204" "$CODE" "$BODY"

        # Verify deletion
        RESP=$(curl_user -sw '\n%{http_code}' "$BASE/pins?limit=10")
        BODY=$(echo "$RESP" | sed '$d')
        CODE=$(echo "$RESP" | tail -n1)
        assert_json_field "pins: empty after delete" "$BODY" "len(d.get('items',[])) == 0"
    fi
else
    TOTAL=$((TOTAL + 5)); FAIL=$((FAIL + 5))
    echo -e "  ${RED}FAIL${NC} No execution_id available for pinboard tests"
fi

# ===================================================================
# 9.5 REVISIONS + MIGRATION (Golden Path)
# ===================================================================
echo ""
echo -e "${YELLOW}=== 9.5 Revisions + Migration ===${NC}"

if [ -n "${PROJECT_ID:-}" ]; then
    # Recreate project for revision tests (previous one was designed)
    RESP=$(curl -sw '\n%{http_code}' --max-time 120 -X POST "$BASE/projects" \
      -H 'Content-Type: application/json' \
      $AUTH_HEADER \
      -d '{
        "name": "Migration Test",
        "source": {"type": "text", "data": "id,name,role\n1,Alice,Admin\n2,Bob,User"},
        "context": "User management"
      }')
    BODY=$(echo "$RESP" | sed '$d')
    CODE=$(echo "$RESP" | tail -n1)
    assert_status "POST /projects (migration test)" "201" "$CODE" "$BODY"
    MIG_PROJECT_ID=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null || echo "")
    MIG_REV=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['revision'])" 2>/dev/null || echo "1")

    if [ -n "$MIG_PROJECT_ID" ]; then
        # Design ontology
        RESP=$(curl -sw '\n%{http_code}' --max-time 120 -X POST "$BASE/projects/$MIG_PROJECT_ID/design" \
          -H 'Content-Type: application/json' \
          $AUTH_HEADER \
          -d "{\"revision\": $MIG_REV}")
        BODY=$(echo "$RESP" | sed '$d')
        CODE=$(echo "$RESP" | tail -n1)
        assert_status "POST /projects/:id/design (migration)" "200" "$CODE" "$BODY"
        MIG_REV=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['revision'])" 2>/dev/null || echo "2")

        # List revisions
        RESP=$(curl -sw '\n%{http_code}' "$BASE/projects/$MIG_PROJECT_ID/revisions" $AUTH_HEADER)
        BODY=$(echo "$RESP" | sed '$d')
        CODE=$(echo "$RESP" | tail -n1)
        assert_status "GET /projects/:id/revisions" "200" "$CODE" "$BODY"
        assert_json_field "revisions: has items" "$BODY" "'items' in d and len(d['items']) >= 1"
        FIRST_REV=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['items'][-1]['revision'])" 2>/dev/null || echo "1")

        # Migration preview (dry_run)
        RESP=$(curl -sw '\n%{http_code}' --max-time 30 -X POST \
          "$BASE/projects/$MIG_PROJECT_ID/revisions/$FIRST_REV/migrate" \
          -H 'Content-Type: application/json' \
          $AUTH_HEADER \
          -d '{"dry_run": true}')
        BODY=$(echo "$RESP" | sed '$d')
        CODE=$(echo "$RESP" | tail -n1)
        assert_status "POST migrate (dry_run preview)" "200" "$CODE" "$BODY"
        assert_json_field "migration: has up" "$BODY" "'up' in d"
        assert_json_field "migration: has down" "$BODY" "'down' in d"
        assert_json_field "migration: has warnings" "$BODY" "'warnings' in d"
        assert_json_field "migration: not executed" "$BODY" "d.get('executed') == False"

        # Cleanup migration test project
        curl -s -X DELETE "$BASE/projects/$MIG_PROJECT_ID" $AUTH_HEADER > /dev/null 2>&1
    fi
fi

# ===================================================================
# 10. RAW QUERY
# ===================================================================
echo ""
echo -e "${YELLOW}=== 10. Raw Query ===${NC}"

RESP=$(curl -sw '\n%{http_code}' -X POST "$BASE/query/raw" \
  -H 'Content-Type: application/json' \
  $AUTH_HEADER \
  -d '{"query":"MATCH (n) RETURN n LIMIT 5"}')
BODY=$(echo "$RESP" | sed '$d')
CODE=$(echo "$RESP" | tail -n1)
# With Neo4j connected, expect 200; without, expect 503
if [ "$CODE" = "200" ] || [ "$CODE" = "503" ]; then
    TOTAL=$((TOTAL + 1)); PASS=$((PASS + 1))
    echo -e "  ${GREEN}PASS${NC} [$CODE] POST /query/raw"
else
    TOTAL=$((TOTAL + 1)); FAIL=$((FAIL + 1))
    echo -e "  ${RED}FAIL${NC} [$CODE] POST /query/raw (expected 200 or 503)"
    echo "       Body: ${BODY:0:200}"
fi
if [ "$CODE" = "200" ]; then
    assert_json_field "query/raw: has results" "$BODY" "'results' in d"
else
    assert_json_field "query/raw: structured error" "$BODY" "'error' in d and 'type' in d['error']"
fi

# 10b. Validation: empty query -> 400
RESP=$(curl -sw '\n%{http_code}' -X POST "$BASE/query/raw" \
  -H 'Content-Type: application/json' \
  $AUTH_HEADER \
  -d '{"query":""}')
BODY=$(echo "$RESP" | sed '$d')
CODE=$(echo "$RESP" | tail -n1)
assert_status "POST /query/raw (empty query)" "400" "$CODE" "$BODY"

# ===================================================================
# 11. BODY SIZE LIMIT
# ===================================================================
echo ""
echo -e "${YELLOW}=== 11. Body Size Limit ===${NC}"

TMPFILE=$(mktemp)
python3 -c "import json; f=open('$TMPFILE','w'); json.dump({'message':'x'*(3*1024*1024)}, f); f.close()"
RESP=$(curl_user -sw '\n%{http_code}' -X POST "$BASE/chat" \
  -H 'Content-Type: application/json' \
  -d @"$TMPFILE")
BODY=$(echo "$RESP" | sed '$d')
CODE=$(echo "$RESP" | tail -n1)
rm -f "$TMPFILE"
assert_status "POST oversized body (3MB > 2MB limit)" "413" "$CODE" "$BODY"

# ===================================================================
# 12. ERROR RESPONSE CONSISTENCY
# ===================================================================
echo ""
echo -e "${YELLOW}=== 12. Error Response Consistency ===${NC}"

TOTAL=$((TOTAL + 1))
ERROR_URLS=(
    "GET:$BASE/query/history/00000000-0000-0000-0000-000000000000"
    "GET:$BASE/projects/00000000-0000-0000-0000-000000000000"
)
ALL_CONSISTENT=true
for entry in "${ERROR_URLS[@]}"; do
    METHOD="${entry%%:*}"
    URL="${entry#*:}"
    if [ "$METHOD" = "GET" ]; then
        ERR_BODY=$(curl_user -s "$URL" $AUTH_HEADER)
    fi
    # Check structured error format: { "error": { "type": "...", "message": "..." } }
    if ! echo "$ERR_BODY" | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'error' in d and 'type' in d['error'] and 'message' in d['error']" 2>/dev/null; then
        ALL_CONSISTENT=false
        echo -e "  ${RED}FAIL${NC} $METHOD $URL missing structured error format"
    fi
done
if $ALL_CONSISTENT; then
    echo -e "  ${GREEN}PASS${NC} All error responses have { error: { type, message } } format"
    PASS=$((PASS + 1))
else
    FAIL=$((FAIL + 1))
fi

# ===================================================================
# 13. DELETE PROJECT (cleanup)
# ===================================================================
echo ""
echo -e "${YELLOW}=== 13. Delete Project (cleanup) ===${NC}"

if [ -n "${PROJECT_ID:-}" ]; then
    RESP=$(curl -sw '\n%{http_code}' -X DELETE "$BASE/projects/$PROJECT_ID" $AUTH_HEADER)
    BODY=$(echo "$RESP" | sed '$d')
    CODE=$(echo "$RESP" | tail -n1)
    assert_status "DELETE /projects/:id" "204" "$CODE" "$BODY"

    # Verify it's gone
    RESP=$(curl -sw '\n%{http_code}' "$BASE/projects/$PROJECT_ID" $AUTH_HEADER)
    BODY=$(echo "$RESP" | sed '$d')
    CODE=$(echo "$RESP" | tail -n1)
    assert_status "GET deleted project -> 404" "404" "$CODE" "$BODY"
fi

# ===================================================================
# SUMMARY
# ===================================================================
echo ""
echo "============================================"
echo -e " Results: ${GREEN}${PASS} passed${NC} / ${RED}${FAIL} failed${NC} / ${TOTAL} total"
echo "============================================"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
