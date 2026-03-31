#!/usr/bin/env bash
# ============================================================
# Olive Young Knowledge Graph — Verification Suite
# 5대 핵심 그래프 시나리오 검증 (RDS 불가능 패턴)
# ============================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Docker — OrbStack 또는 시스템 docker
if command -v docker &>/dev/null; then
  DOCKER="docker"
elif [ -x "${HOME}/.orbstack/bin/docker" ]; then
  DOCKER="${HOME}/.orbstack/bin/docker"
else
  echo "ERROR: docker not found. Install Docker or OrbStack." >&2
  exit 1
fi

NEO4J_USER="neo4j"
NEO4J_PASS="ontosyx-dev"

# 색상
GREEN='\033[0;32m'; RED='\033[0;31m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

PASS=0; FAIL=0; TOTAL=0

# ============================================================
# Helper Functions
# ============================================================

find_neo4j_container() {
  $DOCKER compose -f "$SCRIPT_DIR/../../docker-compose.yml" ps --format '{{.Name}}' 2>/dev/null \
    | grep -i neo4j | head -1 || \
  $DOCKER ps --format '{{.Names}}' 2>/dev/null \
    | grep -i neo4j | head -1 || \
  echo ""
}

run_cypher() {
  local query="$1"
  local container
  container=$(find_neo4j_container)
  if [ -z "$container" ]; then
    echo "ERROR: Neo4j container not found" >&2
    return 1
  fi
  $DOCKER exec -i "$container" cypher-shell \
    -u "$NEO4J_USER" -p "$NEO4J_PASS" \
    --format plain \
    "$query" 2>/dev/null
}

run_cypher_value() {
  local query="$1"
  run_cypher "$query" | tail -1 | tr -d ' "' | tr -d "'"
}

assert_eq() {
  local name="$1" expected="$2" actual="$3"
  TOTAL=$((TOTAL + 1))
  if [ "$actual" = "$expected" ]; then
    echo -e "  ${GREEN}PASS${NC} $name (=$expected)"
    PASS=$((PASS + 1))
  else
    echo -e "  ${RED}FAIL${NC} $name (expected=$expected, got=$actual)"
    FAIL=$((FAIL + 1))
  fi
}

assert_gte() {
  local name="$1" expected="$2" actual="$3"
  TOTAL=$((TOTAL + 1))
  if [ "$actual" -ge "$expected" ] 2>/dev/null; then
    echo -e "  ${GREEN}PASS${NC} $name (>=$expected, got=$actual)"
    PASS=$((PASS + 1))
  else
    echo -e "  ${RED}FAIL${NC} $name (expected>=$expected, got=$actual)"
    FAIL=$((FAIL + 1))
  fi
}

assert_gt() {
  local name="$1" expected="$2" actual="$3"
  TOTAL=$((TOTAL + 1))
  if [ "$actual" -gt "$expected" ] 2>/dev/null; then
    echo -e "  ${GREEN}PASS${NC} $name (>$expected, got=$actual)"
    PASS=$((PASS + 1))
  else
    echo -e "  ${RED}FAIL${NC} $name (expected>$expected, got=$actual)"
    FAIL=$((FAIL + 1))
  fi
}

# ============================================================
# Phase 0: Infrastructure
# ============================================================

ensure_containers() {
  echo -e "${BOLD}Phase 0: Infrastructure${NC}"
  local container
  container=$(find_neo4j_container)
  if [ -z "$container" ]; then
    echo -e "  ${YELLOW}Starting Neo4j container...${NC}"
    $DOCKER compose -f "$SCRIPT_DIR/../../docker-compose.yml" up -d neo4j
    echo -n "  Waiting for Neo4j"
    for i in $(seq 1 30); do
      if run_cypher "RETURN 1" &>/dev/null; then
        echo -e " ${GREEN}ready${NC}"
        return 0
      fi
      echo -n "."
      sleep 2
    done
    echo -e " ${RED}timeout${NC}"
    exit 1
  else
    echo -e "  Neo4j container: ${GREEN}$container${NC}"
    # Wait for readiness
    if ! run_cypher "RETURN 1" &>/dev/null; then
      echo -n "  Waiting for Neo4j readiness"
      for i in $(seq 1 15); do
        if run_cypher "RETURN 1" &>/dev/null; then
          echo -e " ${GREEN}ready${NC}"
          return 0
        fi
        echo -n "."
        sleep 2
      done
      echo -e " ${RED}timeout${NC}"
      exit 1
    fi
  fi
}

# ============================================================
# Phase 0.5: PostgreSQL Source Verification
# ============================================================

verify_postgres_source() {
  echo -e "\n${BOLD}Phase 0.5: PostgreSQL Source Verification${NC}"

  local pg_container
  pg_container=$($DOCKER compose -f "$SCRIPT_DIR/../../docker-compose.yml" ps --format '{{.Name}}' 2>/dev/null | grep -i "postgres-source" | head -1 || echo "")
  if [ -z "$pg_container" ]; then
    echo -e "  ${YELLOW}postgres-source not running — skipping${NC}"
    return 0
  fi

  local val

  val=$($DOCKER exec "$pg_container" psql -U source -d olive_young -t -c "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE'" 2>/dev/null | tr -d ' ')
  assert_eq "PG: Table count" "21" "$val"

  val=$($DOCKER exec "$pg_container" psql -U source -d olive_young -t -c "SELECT count(*) FROM products" 2>/dev/null | tr -d ' ')
  assert_eq "PG: Products" "100" "$val"

  val=$($DOCKER exec "$pg_container" psql -U source -d olive_young -t -c "SELECT count(*) FROM customers" 2>/dev/null | tr -d ' ')
  assert_eq "PG: Customers" "50" "$val"

  val=$($DOCKER exec "$pg_container" psql -U source -d olive_young -t -c "SELECT count(*) FROM ingredients" 2>/dev/null | tr -d ' ')
  assert_eq "PG: Ingredients" "25" "$val"

  val=$($DOCKER exec "$pg_container" psql -U source -d olive_young -t -c "SELECT count(*) FROM brands" 2>/dev/null | tr -d ' ')
  assert_eq "PG: Brands" "31" "$val"

  val=$($DOCKER exec "$pg_container" psql -U source -d olive_young -t -c "SELECT count(*) FROM stores" 2>/dev/null | tr -d ' ')
  assert_eq "PG: Stores" "20" "$val"

  val=$($DOCKER exec "$pg_container" psql -U source -d olive_young -t -c "SELECT count(*) FROM transactions" 2>/dev/null | tr -d ' ')
  assert_gte "PG: Transactions" "165" "$val"

  val=$($DOCKER exec "$pg_container" psql -U source -d olive_young -t -c "SELECT count(*) FROM product_ingredients" 2>/dev/null | tr -d ' ')
  assert_gte "PG: Product-Ingredient mappings" "160" "$val"
}

# ============================================================
# Phase 1: Data Loading (idempotent)
# ============================================================

load_data() {
  echo -e "\n${BOLD}Phase 1: Data Loading${NC}"
  local container
  container=$(find_neo4j_container)

  echo -e "  Loading seed.cypher..."
  $DOCKER exec -i "$container" cypher-shell \
    -u "$NEO4J_USER" -p "$NEO4J_PASS" \
    < "$SCRIPT_DIR/seed.cypher" 2>/dev/null && \
    echo -e "  ${GREEN}Base seed loaded${NC}" || \
    echo -e "  ${YELLOW}Base seed already loaded (constraints exist)${NC}"

  echo -e "  Loading seed-enrich.cypher..."
  $DOCKER exec -i "$container" cypher-shell \
    -u "$NEO4J_USER" -p "$NEO4J_PASS" \
    < "$SCRIPT_DIR/seed-enrich.cypher" 2>/dev/null && \
    echo -e "  ${GREEN}Enrichment data loaded${NC}" || \
    echo -e "  ${RED}Enrichment load failed${NC}"

  # Quick summary
  echo -e "  ${CYAN}--- Data Summary ---${NC}"
  run_cypher "MATCH (n) RETURN labels(n)[0] AS label, count(*) AS cnt ORDER BY cnt DESC"
  echo ""
  run_cypher "MATCH ()-[r]->() RETURN type(r) AS rel, count(*) AS cnt ORDER BY cnt DESC"
}

# ============================================================
# Phase 2: Structural Verification (11 checks)
# ============================================================

verify_structure() {
  echo -e "\n${BOLD}Phase 2: Structural Verification${NC}"

  local val

  val=$(run_cypher_value "MATCH (p:Product) RETURN count(p)")
  assert_eq "Product count" "100" "$val"

  val=$(run_cypher_value "MATCH (c:Customer) RETURN count(c)")
  assert_eq "Customer count" "50" "$val"

  val=$(run_cypher_value "MATCH (i:Ingredient) RETURN count(i)")
  assert_eq "Ingredient count" "25" "$val"

  # 모든 제품에 성분 연결
  val=$(run_cypher_value "MATCH (p:Product) WHERE NOT (p)-[:HAS_INGREDIENT]->() RETURN count(p)")
  assert_eq "Products without ingredients" "0" "$val"

  # 고아 고객 없음 (추천 네트워크 전원 편입)
  val=$(run_cypher_value "MATCH (c:Customer) WHERE NOT (c)-[:REFERRED]-() RETURN count(c)")
  assert_eq "Orphaned customers" "0" "$val"

  # 추천 체인 최대 깊이 >= 5
  val=$(run_cypher_value "MATCH path=(root:Customer)-[:REFERRED*]->(leaf:Customer) WHERE NOT ()-[:REFERRED]->(root) RETURN max(length(path))")
  assert_gte "Max referral chain depth" "5" "$val"

  # 다중 상품 장바구니 >= 20
  val=$(run_cypher_value "MATCH (t:Transaction)-[:CONTAINS]->(p:Product) WITH t, count(p) AS cnt WHERE cnt >= 2 RETURN count(t)")
  assert_gte "Multi-product transactions" "20" "$val"

  # 거래 수 >= 210
  val=$(run_cypher_value "MATCH (t:Transaction) RETURN count(t)")
  assert_gte "Transaction count" "210" "$val"

  # 시너지 >= 12쌍
  val=$(run_cypher_value "MATCH ()-[r:SYNERGIZES_WITH]->() RETURN count(r)")
  assert_gte "Synergy pairs" "12" "$val"

  # 충돌 >= 7쌍
  val=$(run_cypher_value "MATCH ()-[r:CONFLICTS_WITH]->() RETURN count(r)")
  assert_gte "Conflict pairs" "7" "$val"

  # 3+ 카테고리 관통 성분 >= 5종
  val=$(run_cypher_value "MATCH (p:Product)-[:IN_CATEGORY]->(c:Category), (p)-[:HAS_INGREDIENT]->(i:Ingredient) WITH i, collect(DISTINCT c.name) AS cats WHERE size(cats) >= 3 RETURN count(i)")
  assert_gte "Ingredients spanning 3+ categories" "5" "$val"
}

# ============================================================
# Phase 3: Scenario Verification (5 scenarios)
# ============================================================

verify_scenarios() {
  echo -e "\n${BOLD}Phase 3: Scenario Verification${NC}"

  local val

  # --- S1: 성분 충돌 탐지 ---
  echo -e "\n  ${CYAN}S1. 실시간 성분 충돌 안전 경고${NC}"
  val=$(run_cypher_value "MATCH (c:Customer)-[:PURCHASED]->(t1:Transaction)-[:CONTAINS]->(p1:Product)-[:HAS_INGREDIENT]->(i1:Ingredient)-[:CONFLICTS_WITH]-(i2:Ingredient)<-[:HAS_INGREDIENT]-(p2:Product)<-[:CONTAINS]-(t2:Transaction)<-[:PURCHASED]-(c) WHERE p1 <> p2 RETURN count(DISTINCT c)")
  assert_gte "S1: Customers with ingredient conflicts" "3" "$val"

  # --- S2: 시너지 크로스셀링 ---
  echo -e "\n  ${CYAN}S2. 시너지 기반 크로스셀링${NC}"
  val=$(run_cypher_value "MATCH (c:Customer)-[:PURCHASED]->(:Transaction)-[:CONTAINS]->(bought:Product)-[:HAS_INGREDIENT]->(i1:Ingredient)-[:SYNERGIZES_WITH]-(i2:Ingredient)<-[:HAS_INGREDIENT]-(rec:Product) WHERE rec <> bought AND NOT EXISTS { MATCH (c)-[:PURCHASED]->(:Transaction)-[:CONTAINS]->(rec) } RETURN count(DISTINCT rec)")
  assert_gte "S2: Cross-sell candidate products" "5" "$val"

  # --- S3: 규제 캐스케이드 ---
  echo -e "\n  ${CYAN}S3. 규제 변경 비즈니스 임팩트${NC}"
  val=$(run_cypher_value "MATCH (reg:Regulation)<-[:REGULATED_BY]-(i:Ingredient)<-[:HAS_INGREDIENT]-(p:Product)<-[:CONTAINS]-(t:Transaction) RETURN count(DISTINCT t)")
  assert_gte "S3: Transactions with regulated products" "10" "$val"

  # --- S4: 안전 루틴 설계 ---
  echo -e "\n  ${CYAN}S4. 안전 루틴 설계 엔진${NC}"
  val=$(run_cypher_value "MATCH (toner:Product)-[:IN_CATEGORY]->(:Category {id: 'cat-toner'}), (serum:Product)-[:IN_CATEGORY]->(:Category {id: 'cat-serum'}), (cream:Product)-[:IN_CATEGORY]->(:Category {id: 'cat-cream'}) MATCH (toner)-[:HAS_INGREDIENT]->(ti:Ingredient)-[:TREATS]->(:SkinConcern {id: 'sc-dryness'}) MATCH (serum)-[:HAS_INGREDIENT]->(si:Ingredient)-[:TREATS]->(sc:SkinConcern) WHERE sc.id IN ['sc-aging','sc-brightening'] MATCH (cream)-[:HAS_INGREDIENT]->(ci:Ingredient)-[:TREATS]->(:SkinConcern {id: 'sc-dryness'}) WHERE NOT EXISTS { MATCH (toner)-[:HAS_INGREDIENT]->(a:Ingredient)-[:CONFLICTS_WITH]-(b:Ingredient)<-[:HAS_INGREDIENT]-(serum) } AND NOT EXISTS { MATCH (serum)-[:HAS_INGREDIENT]->(a:Ingredient)-[:CONFLICTS_WITH]-(b:Ingredient)<-[:HAS_INGREDIENT]-(cream) } AND NOT EXISTS { MATCH (toner)-[:HAS_INGREDIENT]->(a:Ingredient)-[:AGGRAVATES]->(:SkinConcern {id: 'sc-sensitivity'}) } AND NOT EXISTS { MATCH (serum)-[:HAS_INGREDIENT]->(a:Ingredient)-[:AGGRAVATES]->(:SkinConcern {id: 'sc-sensitivity'}) } RETURN count(*) > 0")
  # cypher-shell returns TRUE/true depending on version — normalize
  val=$(echo "$val" | tr '[:upper:]' '[:lower:]')
  assert_eq "S4: Safe routine exists" "true" "$val"

  # --- S5: 인플루언서 ROI ---
  echo -e "\n  ${CYAN}S5. 인플루언서 네트워크 ROI${NC}"
  val=$(run_cypher_value "MATCH (root:Customer) WHERE NOT ()-[:REFERRED]->(root) AND (root)-[:REFERRED]->() MATCH (root)-[:REFERRED*1..6]->(referred:Customer)-[:PURCHASED]->(t:Transaction) RETURN count(DISTINCT root)")
  assert_gte "S5: Root influencers with downstream revenue" "4" "$val"
}

# ============================================================
# Phase 4: Demo Scenarios (human-readable output)
# ============================================================

run_demos() {
  echo -e "\n${BOLD}Phase 4: Demo Scenario Results${NC}"

  echo -e "\n  ${CYAN}--- S1: 성분 충돌 경고 Top 5 ---${NC}"
  run_cypher "MATCH (c:Customer)-[:PURCHASED]->(t1:Transaction)-[:CONTAINS]->(p1:Product)-[:HAS_INGREDIENT]->(i1:Ingredient)-[conf:CONFLICTS_WITH]-(i2:Ingredient)<-[:HAS_INGREDIENT]-(p2:Product)<-[:CONTAINS]-(t2:Transaction)<-[:PURCHASED]-(c) WHERE p1 <> p2 RETURN DISTINCT c.name AS customer, conf.risk_level AS risk, i1.name AS ing1, i2.name AS ing2, p1.name AS product1, p2.name AS product2 ORDER BY CASE conf.risk_level WHEN 'high' THEN 1 WHEN 'medium' THEN 2 ELSE 3 END LIMIT 5"

  echo -e "\n  ${CYAN}--- S2: 시너지 크로스셀 추천 Top 5 ---${NC}"
  run_cypher "MATCH (c:Customer)-[:PURCHASED]->(:Transaction)-[:CONTAINS]->(bought:Product)-[:HAS_INGREDIENT]->(i1:Ingredient)-[syn:SYNERGIZES_WITH]-(i2:Ingredient)<-[:HAS_INGREDIENT]-(rec:Product) WHERE rec <> bought AND NOT EXISTS { MATCH (c)-[:PURCHASED]->(:Transaction)-[:CONTAINS]->(rec) } RETURN DISTINCT c.name AS customer, bought.name AS owned, i1.name AS has_ing, syn.boost_pct AS boost, i2.name AS synergy_ing, rec.name AS recommend ORDER BY syn.boost_pct DESC LIMIT 5"

  echo -e "\n  ${CYAN}--- S3: 규제 캐스케이드 영향 ---${NC}"
  run_cypher "MATCH (reg:Regulation)<-[rb:REGULATED_BY]-(i:Ingredient)<-[:HAS_INGREDIENT]-(p:Product)-[:MADE_BY]->(b:Brand) OPTIONAL MATCH (p)<-[:CONTAINS]-(tx:Transaction) WITH reg.name AS regulation, reg.authority AS authority, i.name AS ingredient, b.name AS brand, count(DISTINCT p) AS products, count(DISTINCT tx) AS transactions RETURN regulation, authority, ingredient, brand, products, transactions ORDER BY transactions DESC LIMIT 10"

  echo -e "\n  ${CYAN}--- S5: 인플루언서 ROI ---${NC}"
  run_cypher "MATCH (root:Customer) WHERE NOT ()-[:REFERRED]->(root) AND (root)-[:REFERRED]->() MATCH path=(root)-[:REFERRED*1..6]->(ref:Customer) OPTIONAL MATCH (ref)-[:PURCHASED]->(tx:Transaction) WITH root, count(DISTINCT ref) AS referrals, max(length(path)) AS depth, sum(COALESCE(tx.total_amount, 0)) AS revenue RETURN root.name AS influencer, root.membership_tier AS tier, referrals, depth AS max_depth, revenue ORDER BY revenue DESC"
}

# ============================================================
# Summary
# ============================================================

print_summary() {
  echo -e "\n${BOLD}============================================${NC}"
  echo -e "${BOLD} Verification Summary${NC}"
  echo -e "${BOLD}============================================${NC}"
  echo -e "  Total:  $TOTAL"
  echo -e "  ${GREEN}Passed: $PASS${NC}"
  if [ "$FAIL" -gt 0 ]; then
    echo -e "  ${RED}Failed: $FAIL${NC}"
  else
    echo -e "  Failed: 0"
  fi
  echo ""
  if [ "$FAIL" -eq 0 ]; then
    echo -e "  ${GREEN}${BOLD}ALL CHECKS PASSED${NC}"
  else
    echo -e "  ${RED}${BOLD}$FAIL CHECK(S) FAILED${NC}"
  fi
  echo ""
  return "$FAIL"
}

# ============================================================
# Main
# ============================================================

main() {
  echo "============================================"
  echo " Olive Young KG — Verification Suite"
  echo " 5 High-Value Graph Scenarios"
  echo "============================================"
  echo ""

  ensure_containers
  verify_postgres_source
  load_data
  verify_structure
  verify_scenarios
  run_demos
  print_summary
}

main "$@"
