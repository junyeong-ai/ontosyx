#!/usr/bin/env bash
# Load Olive Young seed data into Neo4j
# Usage: ./load.sh [NEO4J_URL] [USER] [PASSWORD]

set -euo pipefail

NEO4J_URL="${1:-bolt://localhost:7687}"
NEO4J_USER="${2:-neo4j}"
NEO4J_PASS="${3:-ontosyx-dev}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SEED_FILE="$SCRIPT_DIR/seed.cypher"

if ! command -v cypher-shell &>/dev/null; then
  echo "cypher-shell not found. Trying via Docker..."
  docker exec -i ontosyx-neo4j cypher-shell \
    -u "$NEO4J_USER" -p "$NEO4J_PASS" \
    -a "$NEO4J_URL" \
    < "$SEED_FILE"
else
  cypher-shell \
    -u "$NEO4J_USER" -p "$NEO4J_PASS" \
    -a "$NEO4J_URL" \
    < "$SEED_FILE"
fi

echo ""
echo "✅ Olive Young seed data loaded successfully!"
echo ""
echo "Quick verification queries:"
echo "  MATCH (n) RETURN labels(n)[0] AS label, count(*) AS cnt ORDER BY cnt DESC"
echo "  MATCH ()-[r]->() RETURN type(r) AS rel, count(*) AS cnt ORDER BY cnt DESC"
