// ============================================================
// S2. 시너지 기반 개인화 크로스셀링 엔진
// ============================================================
// 비즈니스 가치: AOV 증가, 시너지 번들 전환율 25-40%
// RDS 불가능: 구매 성분 컨텍스트 + 시너지 탐색 + 미구매 필터 → 5+ JOIN
//
// 경로 (4-hop):
//   (Customer)-[:PURCHASED]->(Tx)-[:CONTAINS]->(Bought)-[:HAS_INGREDIENT]->(I1)
//   -[:SYNERGIZES_WITH]-(I2)<-[:HAS_INGREDIENT]-(Recommend)
//   WHERE NOT Customer already bought Recommend
// ============================================================

MATCH (c:Customer)-[:PURCHASED]->(:Transaction)-[:CONTAINS]->(bought:Product)
        -[:HAS_INGREDIENT]->(i1:Ingredient)-[syn:SYNERGIZES_WITH]-(i2:Ingredient)
        <-[:HAS_INGREDIENT]-(rec:Product)
WHERE rec <> bought
  AND NOT EXISTS {
    MATCH (c)-[:PURCHASED]->(:Transaction)-[:CONTAINS]->(rec)
  }
WITH c, bought, i1, i2, rec, syn.boost_pct AS 시너지효과, syn.mechanism AS 메커니즘
ORDER BY 시너지효과 DESC, c.name
RETURN DISTINCT
  c.name AS 고객명,
  c.membership_tier AS 등급,
  bought.name AS 보유제품,
  i1.name AS 보유성분,
  i2.name AS 시너지성분,
  시너지효과 AS 효과_증폭률_pct,
  rec.name AS 추천제품,
  rec.price AS 추천제품_가격,
  메커니즘
LIMIT 20;
