// ============================================================
// S3. 규제 변경 비즈니스 임팩트 캐스케이드 시뮬레이션
// ============================================================
// 비즈니스 가치: 규제 리스크 즉시 파악 → 선제적 포뮬레이션 변경
// RDS 불가능: 규제→성분→제품→브랜드→매출 5-hop + 대체 성분 양방향 탐색
//
// 경로 (5-hop + 대체 성분 탐색):
//   (Regulation)<-[:REGULATED_BY]-(Ingredient)<-[:HAS_INGREDIENT]-(Product)
//   -[:MADE_BY]->(Brand)
//   + (Product)<-[:CONTAINS]-(Tx) → 매출 집계
//   + (Ingredient)-[:TREATS]->(SkinConcern)<-[:TREATS]-(AltIngredient) → 대체
// ============================================================

// Part 1: 규제별 영향 분석 (제품, 브랜드, 매출)
MATCH (reg:Regulation)<-[rb:REGULATED_BY]-(i:Ingredient)
        <-[:HAS_INGREDIENT]-(p:Product)-[:MADE_BY]->(b:Brand)
OPTIONAL MATCH (p)<-[cont:CONTAINS]-(tx:Transaction)
WITH reg, i, b, rb.max_concentration_pct AS 최대허용농도,
     collect(DISTINCT p.name) AS 영향제품,
     count(DISTINCT p) AS 영향제품수,
     count(DISTINCT tx) AS 관련거래수,
     sum(CASE WHEN tx IS NOT NULL THEN cont.quantity * cont.unit_price ELSE 0 END) AS 관련매출
ORDER BY 관련매출 DESC
RETURN
  reg.name AS 규제명,
  reg.authority AS 기관,
  reg.status AS 상태,
  i.name AS 규제_성분,
  i.name_inci AS 성분_INCI,
  최대허용농도 AS 최대허용농도_pct,
  b.name AS 영향_브랜드,
  영향제품수,
  영향제품,
  관련거래수,
  관련매출 AS 관련매출_원;

// Part 2: 대체 성분 제안 (동일 효능, 충돌 없음)
// MATCH (reg:Regulation)<-[:REGULATED_BY]-(regulated:Ingredient)-[:TREATS]->(sc:SkinConcern)<-[t2:TREATS]-(alt:Ingredient)
// WHERE alt <> regulated
//   AND NOT (alt)-[:REGULATED_BY]->(:Regulation {status: 'banned'})
//   AND NOT (alt)-[:CONFLICTS_WITH]-(regulated)
// WITH regulated, sc, alt, t2.efficacy_level AS 대체효능
// WHERE 대체효능 IN ['high', 'medium']
// RETURN DISTINCT
//   regulated.name AS 규제성분,
//   sc.name AS 효능,
//   alt.name AS 대체성분,
//   alt.ewg_grade AS 대체성분_EWG등급,
//   대체효능
// ORDER BY regulated.name, 대체효능 DESC;
