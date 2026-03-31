#!/usr/bin/env bash
# ============================================================
# Olive Young — Reset & Seed Script
# 용법: ./reset-and-seed.sh [--neo4j-only] [--pg-only] [--verify]
# ============================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
COMPOSE_FILE="$SCRIPT_DIR/../../docker-compose.yml"

# Docker
if command -v docker &>/dev/null; then
  DOCKER="docker"
elif [ -x "${HOME}/.orbstack/bin/docker" ]; then
  DOCKER="${HOME}/.orbstack/bin/docker"
else
  echo "ERROR: docker not found" >&2; exit 1
fi

GREEN='\033[0;32m'; RED='\033[0;31m'; YELLOW='\033[1;33m'; NC='\033[0m'

NEO4J_ONLY=false; PG_ONLY=false; VERIFY=false
for arg in "$@"; do
  case "$arg" in
    --neo4j-only) NEO4J_ONLY=true ;;
    --pg-only)    PG_ONLY=true ;;
    --verify)     VERIFY=true ;;
  esac
done

find_container() {
  local service="$1"
  $DOCKER compose -f "$COMPOSE_FILE" ps --format '{{.Name}}' 2>/dev/null | grep -i "$service" | head -1
}

ensure_running() {
  local service="$1"
  local container
  container=$(find_container "$service")
  if [ -z "$container" ]; then
    echo -e "${YELLOW}Starting $service...${NC}"
    $DOCKER compose -f "$COMPOSE_FILE" up -d "$service"
    sleep 5
    container=$(find_container "$service")
  fi
  echo "$container"
}

# --- Neo4j Reset ---
reset_neo4j() {
  echo -e "\n${YELLOW}=== Neo4j Reset ===${NC}"
  local container
  container=$(ensure_running "neo4j")

  # Wait for readiness
  for i in $(seq 1 20); do
    if $DOCKER exec "$container" cypher-shell -u neo4j -p ontosyx-dev "RETURN 1" &>/dev/null; then
      break
    fi
    sleep 2
  done

  echo "  Clearing all data..."
  $DOCKER exec "$container" cypher-shell -u neo4j -p ontosyx-dev \
    "MATCH (n) DETACH DELETE n" 2>/dev/null

  # Drop constraints (they prevent CREATE on reload)
  $DOCKER exec "$container" cypher-shell -u neo4j -p ontosyx-dev --format plain \
    "SHOW CONSTRAINTS YIELD name RETURN name" 2>/dev/null | tail -n +2 | while read -r name; do
    name=$(echo "$name" | tr -d ' "')
    [ -n "$name" ] && $DOCKER exec "$container" cypher-shell -u neo4j -p ontosyx-dev \
      "DROP CONSTRAINT $name" 2>/dev/null || true
  done

  echo "  Loading seed.cypher..."
  $DOCKER exec -i "$container" cypher-shell -u neo4j -p ontosyx-dev \
    < "$SCRIPT_DIR/seed.cypher" 2>/dev/null
  echo -e "  ${GREEN}Base seed loaded${NC}"

  echo "  Loading seed-enrich.cypher..."
  $DOCKER exec -i "$container" cypher-shell -u neo4j -p ontosyx-dev \
    < "$SCRIPT_DIR/seed-enrich.cypher" 2>/dev/null
  echo -e "  ${GREEN}Enrichment loaded${NC}"

  # Summary
  $DOCKER exec "$container" cypher-shell -u neo4j -p ontosyx-dev --format plain \
    "MATCH (n) RETURN count(n) AS nodes" 2>/dev/null | tail -1 | xargs -I {} echo -e "  Nodes: {}"
  $DOCKER exec "$container" cypher-shell -u neo4j -p ontosyx-dev --format plain \
    "MATCH ()-[r]->() RETURN count(r) AS edges" 2>/dev/null | tail -1 | xargs -I {} echo -e "  Edges: {}"
}

# --- PostgreSQL Reset ---
reset_pg() {
  echo -e "\n${YELLOW}=== PostgreSQL Source Reset ===${NC}"
  local container
  container=$(ensure_running "postgres-source")

  # Wait for readiness
  for i in $(seq 1 10); do
    if $DOCKER exec "$container" pg_isready -U source &>/dev/null; then break; fi
    sleep 2
  done

  echo "  Dropping schema..."
  $DOCKER exec "$container" psql -U source -d olive_young \
    -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;" 2>/dev/null

  echo "  Loading schema.sql..."
  $DOCKER exec -i "$container" psql -U source -d olive_young \
    < "$SCRIPT_DIR/schema.sql" 2>/dev/null

  echo "  Loading seed-data.sql..."
  $DOCKER exec -i "$container" psql -U source -d olive_young \
    < "$SCRIPT_DIR/seed-data.sql" 2>/dev/null

  local count
  count=$($DOCKER exec "$container" psql -U source -d olive_young -t \
    -c "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE'" 2>/dev/null | tr -d ' ')
  echo -e "  ${GREEN}Tables: $count${NC}"
}

# --- Main ---
echo "============================================"
echo " Olive Young — Reset & Seed"
echo "============================================"

if $PG_ONLY; then
  reset_pg
elif $NEO4J_ONLY; then
  reset_neo4j
else
  reset_pg
  reset_neo4j
fi

if $VERIFY; then
  echo ""
  bash "$SCRIPT_DIR/verify.sh"
fi

echo -e "\n${GREEN}Done.${NC}"
