// ============================================================
// S4. 다중 조건 피부 맞춤 안전 루틴 설계 엔진
// ============================================================
// 비즈니스 가치: "스킨케어 소믈리에" → 프리미엄 차별화
// RDS 불가능: 12×15×10=1,800 조합 × 성분 충돌/시너지/악화 검증 → NP-hard
//
// 제약 조건:
//   1. 토너 → TREATS → 건조 (HIGH)
//   2. 세럼 → TREATS → 노화 또는 미백 (HIGH)
//   3. 크림 → TREATS → 건조 (HIGH)
//   4. NOT EXISTS: 제품 간 CONFLICTS_WITH
//   5. NOT EXISTS: AGGRAVATES 민감
//   6. PREFER: SYNERGIZES_WITH 존재
// ============================================================

MATCH (toner:Product)-[:IN_CATEGORY]->(:Category {id: 'cat-toner'}),
      (serum:Product)-[:IN_CATEGORY]->(:Category {id: 'cat-serum'}),
      (cream:Product)-[:IN_CATEGORY]->(:Category {id: 'cat-cream'})

// 건조 효능 토너
MATCH (toner)-[:HAS_INGREDIENT]->(ti:Ingredient)-[:TREATS]->(:SkinConcern {id: 'sc-dryness'})
// 노화 또는 미백 효능 세럼
MATCH (serum)-[:HAS_INGREDIENT]->(si:Ingredient)-[:TREATS]->(sc_serum:SkinConcern)
WHERE sc_serum.id IN ['sc-aging', 'sc-brightening']
// 건조 효능 크림
MATCH (cream)-[:HAS_INGREDIENT]->(ci:Ingredient)-[:TREATS]->(:SkinConcern {id: 'sc-dryness'})

// 성분 충돌 없는 조합만
WHERE NOT EXISTS {
  MATCH (toner)-[:HAS_INGREDIENT]->(a:Ingredient)-[:CONFLICTS_WITH]-(b:Ingredient)<-[:HAS_INGREDIENT]-(serum)
}
AND NOT EXISTS {
  MATCH (serum)-[:HAS_INGREDIENT]->(a:Ingredient)-[:CONFLICTS_WITH]-(b:Ingredient)<-[:HAS_INGREDIENT]-(cream)
}
AND NOT EXISTS {
  MATCH (toner)-[:HAS_INGREDIENT]->(a:Ingredient)-[:CONFLICTS_WITH]-(b:Ingredient)<-[:HAS_INGREDIENT]-(cream)
}

// 민감 피부 악화 성분 없는 조합
AND NOT EXISTS {
  MATCH (toner)-[:HAS_INGREDIENT]->(a:Ingredient)-[:AGGRAVATES]->(:SkinConcern {id: 'sc-sensitivity'})
}
AND NOT EXISTS {
  MATCH (serum)-[:HAS_INGREDIENT]->(a:Ingredient)-[:AGGRAVATES]->(:SkinConcern {id: 'sc-sensitivity'})
}
AND NOT EXISTS {
  MATCH (cream)-[:HAS_INGREDIENT]->(a:Ingredient)-[:AGGRAVATES]->(:SkinConcern {id: 'sc-sensitivity'})
}

// 시너지 쌍 개수 계산 (선호도 지표)
WITH toner, serum, cream, sc_serum,
     toner.price + serum.price + cream.price AS 총가격
OPTIONAL MATCH (toner)-[:HAS_INGREDIENT]->(x:Ingredient)-[:SYNERGIZES_WITH]-(y:Ingredient)<-[:HAS_INGREDIENT]-(serum)
WITH toner, serum, cream, sc_serum, 총가격, count(DISTINCT x) AS 시너지_토너세럼
OPTIONAL MATCH (serum)-[:HAS_INGREDIENT]->(x:Ingredient)-[:SYNERGIZES_WITH]-(y:Ingredient)<-[:HAS_INGREDIENT]-(cream)
WITH toner, serum, cream, sc_serum, 총가격, 시너지_토너세럼, count(DISTINCT x) AS 시너지_세럼크림

RETURN DISTINCT
  toner.name AS 토너,
  serum.name AS 세럼,
  sc_serum.name AS 세럼_효능,
  cream.name AS 크림,
  시너지_토너세럼 + 시너지_세럼크림 AS 시너지_쌍수,
  총가격 AS 루틴_총가격_원
ORDER BY 시너지_쌍수 DESC, 총가격 ASC
LIMIT 10;
