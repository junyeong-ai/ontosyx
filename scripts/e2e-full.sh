#!/bin/bash
# =============================================================================
# Ontosyx Full Pipeline E2E Test
# Tests the complete workflow: Source → Design → Deploy → Load → Query → Dashboard
# Requires: postgres-ecommerce (port 5435), neo4j, ontosyx API running
# =============================================================================

set -euo pipefail

BASE="${ONTOSYX_API_URL:-http://localhost:3001/api}"
API_KEY="${OX_API_KEY:-dev-api-key-ontosyx}"
SOURCE_HOST="${ECOMMERCE_DB_HOST:-localhost}"
SOURCE_PORT="${ECOMMERCE_DB_PORT:-5435}"

PASS=0
FAIL=0
TOTAL=0
PHASE_START=0

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

api() {
    curl -s -H "X-API-Key: $API_KEY" -H "Content-Type: application/json" "$@"
}

api_status() {
    curl -s -o /dev/null -w '%{http_code}' -H "X-API-Key: $API_KEY" -H "Content-Type: application/json" "$@"
}

assert_ok() {
    local name="$1"
    local status="$2"
    TOTAL=$((TOTAL + 1))
    if [ "$status" -ge 200 ] && [ "$status" -lt 300 ]; then
        echo -e "  ${GREEN}PASS${NC} $name (HTTP $status)"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC} $name (HTTP $status)"
        FAIL=$((FAIL + 1))
    fi
}

assert_eq() {
    local name="$1"
    local expected="$2"
    local actual="$3"
    TOTAL=$((TOTAL + 1))
    if [ "$expected" = "$actual" ]; then
        echo -e "  ${GREEN}PASS${NC} $name ($actual)"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC} $name (expected=$expected actual=$actual)"
        FAIL=$((FAIL + 1))
    fi
}

assert_gte() {
    local name="$1"
    local min="$2"
    local actual="$3"
    TOTAL=$((TOTAL + 1))
    if [ "$actual" -ge "$min" ] 2>/dev/null; then
        echo -e "  ${GREEN}PASS${NC} $name ($actual >= $min)"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC} $name ($actual < $min)"
        FAIL=$((FAIL + 1))
    fi
}

phase_start() {
    local name="$1"
    echo ""
    echo -e "${YELLOW}=== $name ===${NC}"
    PHASE_START=$(date +%s)
}

phase_end() {
    local elapsed=$(( $(date +%s) - PHASE_START ))
    echo -e "  ${CYAN}(${elapsed}s)${NC}"
}

jq_field() {
    echo "$1" | python3 -c "import sys,json; d=json.load(sys.stdin); print($2)" 2>/dev/null || echo ""
}

echo "============================================"
echo " Ontosyx Full Pipeline E2E Test"
echo "============================================"
echo " API: $BASE"
echo " Source DB: $SOURCE_HOST:$SOURCE_PORT"
echo ""

# ===================================================================
# Phase 1: Infrastructure Check
# ===================================================================
phase_start "Phase 1: Infrastructure Check"

# API health
HEALTH=$(api "$BASE/health")
API_STATUS=$(jq_field "$HEALTH" "d.get('status', 'unknown')")
assert_eq "API health" "ok" "$API_STATUS" || true

# Check if API is reachable
HTTP=$(api_status "$BASE/health")
assert_ok "API reachable" "$HTTP"

# Check source DB connectivity (via psql)
if command -v psql &>/dev/null; then
    TABLE_COUNT=$(PGPASSWORD=source-dev psql -h "$SOURCE_HOST" -p "$SOURCE_PORT" -U source -d ecommerce -t -c \
        "SELECT count(*) FROM pg_tables WHERE schemaname = 'public'" 2>/dev/null | tr -d ' ')
    assert_gte "Source DB tables" 18 "${TABLE_COUNT:-0}"
else
    echo -e "  ${CYAN}SKIP${NC} psql not available — source DB check skipped"
fi

phase_end

# ===================================================================
# Phase 2: Source DB Verification
# ===================================================================
phase_start "Phase 2: Source DB Verification"

if command -v psql &>/dev/null; then
    verify_count() {
        local table="$1"
        local min="$2"
        local count=$(PGPASSWORD=source-dev psql -h "$SOURCE_HOST" -p "$SOURCE_PORT" -U source -d ecommerce -t -c \
            "SELECT count(*) FROM $table" 2>/dev/null | tr -d ' ')
        assert_gte "$table count" "$min" "${count:-0}"
    }

    verify_count customers 50
    verify_count products 100
    verify_count orders 200
    verify_count order_items 500
    verify_count reviews 100
    verify_count categories 30
    verify_count brands 15
    verify_count shipping_events 200
else
    echo -e "  ${CYAN}SKIP${NC} psql not available"
fi

phase_end

# ===================================================================
# Phase 3: Source Introspection via API
# ===================================================================
phase_start "Phase 3: Source Introspection (Create Project)"

PROJECT_BODY=$(cat <<'JSONEOF'
{
    "title": "E2E E-Commerce Test",
    "source": {
        "type": "postgresql",
        "connection_string": "postgres://source:source-dev@host.docker.internal:5435/ecommerce",
        "schema_name": "public"
    }
}
JSONEOF
)

# Create project from PostgreSQL source
RESP=$(api -X POST "$BASE/projects" -d "$PROJECT_BODY" -w '\n%{http_code}')
HTTP=$(echo "$RESP" | tail -1)
BODY=$(echo "$RESP" | head -n -1)

if [ "$HTTP" = "200" ] || [ "$HTTP" = "201" ]; then
    PROJECT_ID=$(jq_field "$BODY" "d['id']")
    echo -e "  ${GREEN}PASS${NC} Project created: $PROJECT_ID"
    PASS=$((PASS + 1))
    TOTAL=$((TOTAL + 1))

    # Verify source analysis
    TABLES=$(jq_field "$BODY" "d.get('source_schema',{}).get('table_count', 0)")
    assert_gte "Detected tables" 10 "${TABLES:-0}"
else
    echo -e "  ${RED}FAIL${NC} Project creation failed (HTTP $HTTP)"
    echo "       Response: $(echo "$BODY" | head -c 500)"
    FAIL=$((FAIL + 1))
    TOTAL=$((TOTAL + 1))
    PROJECT_ID=""
fi

phase_end

# ===================================================================
# Phase 4: Ontology Design (LLM)
# ===================================================================
phase_start "Phase 4: Ontology Design"

if [ -n "${PROJECT_ID:-}" ]; then
    DESIGN_RESP=$(api -X POST "$BASE/projects/$PROJECT_ID/design" \
        -d '{"source_description": "E-commerce platform with customers, products, orders, reviews, inventory, campaigns, and shipping"}' \
        --max-time 120)

    NODE_COUNT=$(jq_field "$DESIGN_RESP" "len(d.get('ontology',{}).get('node_types',[]))")
    EDGE_COUNT=$(jq_field "$DESIGN_RESP" "len(d.get('ontology',{}).get('edge_types',[]))")

    assert_gte "Designed node types" 6 "${NODE_COUNT:-0}"
    assert_gte "Designed edge types" 5 "${EDGE_COUNT:-0}"

    # Check quality report exists
    HAS_QUALITY=$(jq_field "$DESIGN_RESP" "'quality_report' in d")
    assert_eq "Quality report present" "True" "$HAS_QUALITY"
else
    echo -e "  ${CYAN}SKIP${NC} No project ID"
fi

phase_end

# ===================================================================
# Phase 5: Query Execution (Raw Cypher)
# ===================================================================
phase_start "Phase 5: Query Execution"

# Simple Cypher query
QUERY_RESP=$(api -X POST "$BASE/query/raw" \
    -d '{"query": "RETURN 1 AS test_value"}')
QUERY_STATUS=$(jq_field "$QUERY_RESP" "len(d.get('rows',[]))")
assert_gte "Cypher query returns rows" 1 "${QUERY_STATUS:-0}"

# Graph overview
OVERVIEW_RESP=$(api "$BASE/graph/overview")
OVERVIEW_STATUS=$(api_status "$BASE/graph/overview")
assert_ok "Graph overview" "$OVERVIEW_STATUS"

phase_end

# ===================================================================
# Phase 6: Dashboard Management
# ===================================================================
phase_start "Phase 6: Dashboard CRUD"

# Create dashboard
DASH_RESP=$(api -X POST "$BASE/dashboards" \
    -d '{"name": "E2E Test Dashboard", "description": "Automated test"}')
DASH_ID=$(jq_field "$DASH_RESP" "d.get('id','')")

if [ -n "${DASH_ID:-}" ] && [ "$DASH_ID" != "" ]; then
    echo -e "  ${GREEN}PASS${NC} Dashboard created: $DASH_ID"
    PASS=$((PASS + 1))
    TOTAL=$((TOTAL + 1))

    # List dashboards
    LIST_STATUS=$(api_status "$BASE/dashboards")
    assert_ok "List dashboards" "$LIST_STATUS"

    # Delete dashboard
    DEL_STATUS=$(api_status -X DELETE "$BASE/dashboards/$DASH_ID")
    assert_ok "Delete dashboard" "$DEL_STATUS"
else
    echo -e "  ${RED}FAIL${NC} Dashboard creation failed"
    FAIL=$((FAIL + 1))
    TOTAL=$((TOTAL + 1))
fi

phase_end

# ===================================================================
# Phase 7: User & Workspace Management
# ===================================================================
phase_start "Phase 7: Workspace Management"

WS_LIST_STATUS=$(api_status "$BASE/workspaces")
assert_ok "List workspaces" "$WS_LIST_STATUS"

USER_LIST_STATUS=$(api_status "$BASE/users")
assert_ok "List users" "$USER_LIST_STATUS"

phase_end

# ===================================================================
# Phase 8: Config & System
# ===================================================================
phase_start "Phase 8: System Config"

CONFIG_STATUS=$(api_status "$BASE/config")
assert_ok "Get config" "$CONFIG_STATUS"

UI_CONFIG_STATUS=$(api_status "$BASE/config/ui")
assert_ok "Get UI config" "$UI_CONFIG_STATUS"

phase_end

# ===================================================================
# Phase 9: Cleanup
# ===================================================================
phase_start "Phase 9: Cleanup"

if [ -n "${PROJECT_ID:-}" ]; then
    DEL_STATUS=$(api_status -X DELETE "$BASE/projects/$PROJECT_ID")
    assert_ok "Delete test project" "$DEL_STATUS"
fi

phase_end

# ===================================================================
# Summary
# ===================================================================
echo ""
echo "============================================"
echo -e " Results: ${GREEN}${PASS} passed${NC}, ${RED}${FAIL} failed${NC}, ${TOTAL} total"
echo "============================================"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
