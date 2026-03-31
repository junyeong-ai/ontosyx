// ============================================================
// S1. 실시간 성분 충돌 안전 경고 시스템
// ============================================================
// 비즈니스 가치: 고객 안전 경고, 브랜드 신뢰, 법적 리스크 감소
// RDS 불가능: 고객당 12거래 × 20성분 = 26,400 쌍 비교 → 실시간 불가
//
// 경로 (4-hop):
//   (Customer)-[:PURCHASED]->(Tx)-[:CONTAINS]->(P1)-[:HAS_INGREDIENT]->(I1)
//   -[:CONFLICTS_WITH]-(I2)<-[:HAS_INGREDIENT]-(P2)<-[:CONTAINS]-(Tx2)
//   <-[:PURCHASED]-(Customer)
// ============================================================

MATCH (c:Customer)-[:PURCHASED]->(t1:Transaction)-[:CONTAINS]->(p1:Product)
        -[:HAS_INGREDIENT]->(i1:Ingredient)-[conf:CONFLICTS_WITH]-(i2:Ingredient)
        <-[:HAS_INGREDIENT]-(p2:Product)<-[:CONTAINS]-(t2:Transaction)
        <-[:PURCHASED]-(c)
WHERE p1 <> p2
WITH c, p1, p2, i1, i2, conf,
     t1.purchased_at AS 구매일1, t2.purchased_at AS 구매일2
ORDER BY
  CASE conf.risk_level WHEN 'high' THEN 1 WHEN 'medium' THEN 2 ELSE 3 END,
  c.name
RETURN DISTINCT
  c.name AS 고객명,
  c.membership_tier AS 등급,
  conf.risk_level AS 위험도,
  i1.name AS 성분1,
  i2.name AS 성분2,
  p1.name AS 제품1,
  p2.name AS 제품2,
  conf.reason AS 충돌_사유
LIMIT 20;
