#!/usr/bin/env bash
# ============================================================
# Reset the local dev environment to a clean state
#
# Clears:  ecommerce source DB, Neo4j graph data
# Checks:  Docker services running, API healthy
# Safe to re-run multiple times (idempotent).
#
# Usage: ./scripts/reset-dev.sh
# ============================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Colours ──────────────────────────────────────────────────
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

ok()   { echo -e "  ${GREEN}OK${NC}   $1"; }
fail() { echo -e "  ${RED}FAIL${NC} $1"; }
info() { echo -e "  ${CYAN}INFO${NC} $1"; }
step() { echo -e "\n${YELLOW}=== $1 ===${NC}"; }

# ── Configuration ────────────────────────────────────────────
NEO4J_HOST="${NEO4J_HOST:-localhost}"
NEO4J_BOLT_PORT="${NEO4J_BOLT_PORT:-7687}"
NEO4J_HTTP_PORT="${NEO4J_HTTP_PORT:-7474}"
NEO4J_USER="${NEO4J_USER:-neo4j}"
NEO4J_PASS="${NEO4J_PASS:-ontosyx-dev}"

API_URL="${ONTOSYX_API_URL:-http://localhost:3001/api}"

echo "============================================"
echo " Ontosyx Dev Environment Reset"
echo "============================================"
echo " Neo4j:  bolt://${NEO4J_HOST}:${NEO4J_BOLT_PORT}"
echo " API:    ${API_URL}"
echo ""

# ─────────────────────────────────────────────────────────────
# Step 1: Check Docker services are running
# ─────────────────────────────────────────────────────────────
step "Step 1: Check Docker services"

SERVICES_OK=true

check_container() {
    local name="$1"
    if docker compose ps --status running 2>/dev/null | grep -q "$name"; then
        ok "$name is running"
    elif docker ps --format '{{.Names}}' 2>/dev/null | grep -q "$name"; then
        ok "$name is running"
    else
        fail "$name is NOT running"
        SERVICES_OK=false
    fi
}

check_container "postgres-ecommerce"
check_container "neo4j"
check_container "postgres"

if [ "$SERVICES_OK" = false ]; then
    echo ""
    info "Start services with: docker compose up -d"
    info "Continuing anyway (some steps may fail)..."
fi

# ─────────────────────────────────────────────────────────────
# Step 2: Reset ecommerce source database
# ─────────────────────────────────────────────────────────────
step "Step 2: Reset ecommerce source database"

ECOMMERCE_RESET="$ROOT_DIR/data/ecommerce/reset.sh"
if [ -x "$ECOMMERCE_RESET" ]; then
    if bash "$ECOMMERCE_RESET"; then
        ok "Ecommerce DB reset complete"
    else
        fail "Ecommerce DB reset failed (exit $?)"
    fi
else
    fail "reset script not found or not executable: $ECOMMERCE_RESET"
fi

# ─────────────────────────────────────────────────────────────
# Step 3: Clear Neo4j graph data
# ─────────────────────────────────────────────────────────────
step "Step 3: Clear Neo4j graph data"

neo4j_query() {
    local query="$1"
    curl -sf -X POST "http://${NEO4J_HOST}:${NEO4J_HTTP_PORT}/db/neo4j/tx/commit" \
        -u "${NEO4J_USER}:${NEO4J_PASS}" \
        -H "Content-Type: application/json" \
        -d "{\"statements\": [{\"statement\": \"$query\"}]}"
}

# Delete all relationships first, then all nodes (batched to avoid OOM on large graphs)
info "Deleting relationships (batched)..."
for _ in $(seq 1 20); do
    RESULT=$(neo4j_query "MATCH ()-[r]->() WITH r LIMIT 10000 DELETE r RETURN count(*) AS deleted" 2>/dev/null) || break
    DELETED=$(echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['results'][0]['data'][0]['row'][0])" 2>/dev/null || echo "0")
    if [ "$DELETED" = "0" ]; then
        break
    fi
    info "  deleted $DELETED relationships"
done

info "Deleting nodes (batched)..."
for _ in $(seq 1 20); do
    RESULT=$(neo4j_query "MATCH (n) WITH n LIMIT 10000 DETACH DELETE n RETURN count(*) AS deleted" 2>/dev/null) || break
    DELETED=$(echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['results'][0]['data'][0]['row'][0])" 2>/dev/null || echo "0")
    if [ "$DELETED" = "0" ]; then
        break
    fi
    info "  deleted $DELETED nodes"
done

# Drop all indexes and constraints (idempotent — no error if none exist)
info "Dropping indexes and constraints..."
INDEXES=$(neo4j_query "SHOW INDEXES YIELD name RETURN collect(name) AS names" 2>/dev/null) || true
INDEX_NAMES=$(echo "$INDEXES" | python3 -c "
import sys, json
d = json.load(sys.stdin)
names = d['results'][0]['data'][0]['row'][0] if d.get('results') else []
# Skip built-in lookup indexes
for n in names:
    if not n.startswith('__'):
        print(n)
" 2>/dev/null || true)

while IFS= read -r idx; do
    [ -z "$idx" ] && continue
    neo4j_query "DROP INDEX \`$idx\` IF EXISTS" >/dev/null 2>&1 || true
    info "  dropped index: $idx"
done <<< "$INDEX_NAMES"

CONSTRAINTS=$(neo4j_query "SHOW CONSTRAINTS YIELD name RETURN collect(name) AS names" 2>/dev/null) || true
CONSTRAINT_NAMES=$(echo "$CONSTRAINTS" | python3 -c "
import sys, json
d = json.load(sys.stdin)
names = d['results'][0]['data'][0]['row'][0] if d.get('results') else []
for n in names:
    print(n)
" 2>/dev/null || true)

while IFS= read -r cst; do
    [ -z "$cst" ] && continue
    neo4j_query "DROP CONSTRAINT \`$cst\` IF EXISTS" >/dev/null 2>&1 || true
    info "  dropped constraint: $cst"
done <<< "$CONSTRAINT_NAMES"

# Verify empty
VERIFY=$(neo4j_query "MATCH (n) RETURN count(n) AS cnt" 2>/dev/null) || true
NODE_CNT=$(echo "$VERIFY" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['results'][0]['data'][0]['row'][0])" 2>/dev/null || echo "?")
if [ "$NODE_CNT" = "0" ]; then
    ok "Neo4j graph is empty ($NODE_CNT nodes)"
else
    fail "Neo4j still has $NODE_CNT nodes remaining"
fi

# ─────────────────────────────────────────────────────────────
# Step 4: Wait for API health
# ─────────────────────────────────────────────────────────────
step "Step 4: API health check"

API_HEALTHY=false
for i in $(seq 1 10); do
    HTTP_CODE=$(curl -sf -o /dev/null -w '%{http_code}' "$API_URL/health" 2>/dev/null || echo "000")
    if [ "$HTTP_CODE" = "200" ]; then
        API_HEALTHY=true
        break
    fi
    info "Waiting for API... (attempt $i/10, got HTTP $HTTP_CODE)"
    sleep 2
done

if [ "$API_HEALTHY" = true ]; then
    ok "API is healthy"
else
    fail "API did not respond (is it running?)"
    info "Start with: cargo run -p ox-api"
fi

# ─────────────────────────────────────────────────────────────
# Step 5: Summary
# ─────────────────────────────────────────────────────────────
step "Summary"

echo "  Ecommerce DB:  reset and re-seeded"
echo "  Neo4j:         all nodes, relationships, indexes, constraints cleared"
echo "  API:           $([ "$API_HEALTHY" = true ] && echo "healthy" || echo "NOT reachable")"
echo ""
echo -e "${GREEN}Dev environment reset complete.${NC}"
echo "Run the E2E test: ./scripts/e2e-full.sh"
