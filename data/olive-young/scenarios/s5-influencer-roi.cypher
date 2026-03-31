// ============================================================
// S5. 인플루언서 네트워크 ROI & 성분 트렌드 전파 분석
// ============================================================
// 비즈니스 가치: 인플루언서 투자 ROI 정량화, 바이럴 성분 트렌드 예측
// RDS 불가능: [:REFERRED*1..6] 가변 깊이 → 6중 self-join + 깊이별 집계
//
// 경로 (가변 길이):
//   (Root)-[:REFERRED*1..6]->(Referred)-[:PURCHASED]->(Tx)
//   + 구매 성분 교집합 분석
// ============================================================

// Part 1: 인플루언서별 ROI 분석
MATCH (root:Customer)
WHERE NOT ()-[:REFERRED]->(root)
  AND (root)-[:REFERRED]->()
MATCH path = (root)-[:REFERRED*1..6]->(referred:Customer)
OPTIONAL MATCH (referred)-[:PURCHASED]->(tx:Transaction)
WITH root,
     collect(DISTINCT referred) AS 추천고객목록,
     count(DISTINCT referred) AS 추천회원수,
     max(length(path)) AS 최대체인깊이,
     sum(COALESCE(tx.total_amount, 0)) AS 총매출전환
RETURN
  root.name AS 인플루언서,
  root.id AS 인플루언서_ID,
  root.membership_tier AS 등급,
  root.age AS 나이,
  추천회원수,
  최대체인깊이,
  총매출전환 AS 총매출전환_원
ORDER BY 총매출전환 DESC;

// Part 2: 성분 트렌드 전파 분석
// 루트 인플루언서가 구매한 핵심 성분 중 추천 체인으로 전파된 비율
// MATCH (root:Customer)
// WHERE NOT ()-[:REFERRED]->(root)
//   AND (root)-[:REFERRED]->()
// MATCH (root)-[:PURCHASED]->(:Transaction)-[:CONTAINS]->(:Product)
//         -[:HAS_INGREDIENT {is_key_ingredient: true}]->(rootIng:Ingredient)
// WITH root, collect(DISTINCT rootIng) AS 루트핵심성분
// MATCH (root)-[:REFERRED*1..6]->(child:Customer)
// MATCH (child)-[:PURCHASED]->(:Transaction)-[:CONTAINS]->(:Product)
//         -[:HAS_INGREDIENT]->(childIng:Ingredient)
// WHERE childIng IN 루트핵심성분
// WITH root, 루트핵심성분,
//      collect(DISTINCT childIng) AS 전파성분,
//      count(DISTINCT child) AS 전파고객수
// RETURN
//   root.name AS 인플루언서,
//   size(루트핵심성분) AS 핵심성분수,
//   size(전파성분) AS 전파된성분수,
//   전파고객수,
//   [s IN 전파성분 | s.name] AS 전파성분목록,
//   round(100.0 * size(전파성분) / size(루트핵심성분)) AS 성분전파율_pct
// ORDER BY 성분전파율_pct DESC;
