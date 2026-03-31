// ============================================================
// Olive Young Knowledge Graph — Enrichment Data
// Additive-only (MERGE 기반, 멱등). seed.cypher 로드 후 실행.
// 목적: 5대 핵심 그래프 시나리오가 유의미한 결과를 반환하도록 보강
// ============================================================

// ============================================================
// 1. 크로스 카테고리 성분 연결 (HAS_INGREDIENT)
//    45개 미연결 제품에 실제 K-뷰티 성분 기반 연결 추가
//    핵심: ig-ha, ig-panthenol, ig-niacinamide, ig-ceramide가
//          6+ 카테고리를 관통하여 크로스 카테고리 분석 활성화
// ============================================================

// --- 마스크/팩 (누락분) ---
MATCH (p:Product {id: 'p048'}), (i:Ingredient) WHERE i.id IN ['ig-aha','ig-centella','ig-aloe'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-aha'}]->(i);

// --- 클렌저 (누락분) ---
MATCH (p:Product {id: 'p053'}), (i:Ingredient) WHERE i.id IN ['ig-greentea','ig-ha'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-greentea'}]->(i);
MATCH (p:Product {id: 'p055'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-ceramide','ig-panthenol'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ha'}]->(i);
MATCH (p:Product {id: 'p056'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-niacinamide'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-niacinamide'}]->(i);

// --- 립 제품 (p057-p065) — 보습 성분 ---
MATCH (p:Product {id: 'p057'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-vite'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p058'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-vite'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p059'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-collagen'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p060'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-vite'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p061'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-collagen'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p062'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-ceramide'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p063'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-vite'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p064'}), (i:Ingredient) WHERE i.id IN ['ig-ceramide','ig-vite','ig-panthenol'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ceramide'}]->(i);
MATCH (p:Product {id: 'p065'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-collagen'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);

// --- 아이 제품 (p066, p075-p079) — 판테놀, 비타민 ---
MATCH (p:Product {id: 'p066'}), (i:Ingredient) WHERE i.id IN ['ig-panthenol','ig-vite'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p075'}), (i:Ingredient) WHERE i.id IN ['ig-vite','ig-aloe'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p076'}), (i:Ingredient) WHERE i.id IN ['ig-vite','ig-panthenol'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p077'}), (i:Ingredient) WHERE i.id IN ['ig-vite','ig-aloe'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p078'}), (i:Ingredient) WHERE i.id IN ['ig-vite','ig-panthenol'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p079'}), (i:Ingredient) WHERE i.id IN ['ig-vite','ig-centella'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);

// --- 파운데이션/베이스 (p067-p074) — 스킨케어 메이크업 트렌드 ---
MATCH (p:Product {id: 'p067'}), (i:Ingredient) WHERE i.id IN ['ig-niacinamide','ig-ha','ig-centella'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-niacinamide'}]->(i);
MATCH (p:Product {id: 'p068'}), (i:Ingredient) WHERE i.id IN ['ig-niacinamide','ig-ha'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-niacinamide'}]->(i);
MATCH (p:Product {id: 'p069'}), (i:Ingredient) WHERE i.id IN ['ig-niacinamide','ig-ha','ig-ceramide'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-niacinamide'}]->(i);
MATCH (p:Product {id: 'p070'}), (i:Ingredient) WHERE i.id IN ['ig-greentea','ig-zincoxide'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-greentea'}]->(i);
MATCH (p:Product {id: 'p071'}), (i:Ingredient) WHERE i.id IN ['ig-niacinamide','ig-ha','ig-ceramide'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-niacinamide'}]->(i);
MATCH (p:Product {id: 'p072'}), (i:Ingredient) WHERE i.id IN ['ig-vite','ig-ha'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p073'}), (i:Ingredient) WHERE i.id IN ['ig-niacinamide','ig-ha','ig-centella'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-niacinamide'}]->(i);
MATCH (p:Product {id: 'p074'}), (i:Ingredient) WHERE i.id IN ['ig-niacinamide','ig-collagen','ig-adenosine'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-collagen'}]->(i);

// --- 헤어케어 (p080-p088) — 판테놀 중심 ---
MATCH (p:Product {id: 'p080'}), (i:Ingredient) WHERE i.id IN ['ig-vite','ig-panthenol'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p081'}), (i:Ingredient) WHERE i.id IN ['ig-panthenol','ig-greentea'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-panthenol'}]->(i);
MATCH (p:Product {id: 'p082'}), (i:Ingredient) WHERE i.id IN ['ig-panthenol','ig-ceramide'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-panthenol'}]->(i);
MATCH (p:Product {id: 'p083'}), (i:Ingredient) WHERE i.id IN ['ig-panthenol','ig-aloe'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p084'}), (i:Ingredient) WHERE i.id IN ['ig-greentea','ig-panthenol'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-greentea'}]->(i);
MATCH (p:Product {id: 'p085'}), (i:Ingredient) WHERE i.id IN ['ig-panthenol','ig-vite'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p086'}), (i:Ingredient) WHERE i.id IN ['ig-panthenol','ig-niacinamide'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-panthenol'}]->(i);
MATCH (p:Product {id: 'p087'}), (i:Ingredient) WHERE i.id IN ['ig-panthenol','ig-ceramide','ig-vite'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-panthenol'}]->(i);
MATCH (p:Product {id: 'p088'}), (i:Ingredient) WHERE i.id IN ['ig-panthenol','ig-collagen'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-collagen'}]->(i);

// --- 바디케어 (누락분) ---
MATCH (p:Product {id: 'p090'}), (i:Ingredient) WHERE i.id IN ['ig-aloe','ig-panthenol'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p092'}), (i:Ingredient) WHERE i.id IN ['ig-vite','ig-ha','ig-ceramide'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ceramide'}]->(i);
MATCH (p:Product {id: 'p093'}), (i:Ingredient) WHERE i.id IN ['ig-ceramide','ig-panthenol','ig-niacinamide'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ceramide'}]->(i);
MATCH (p:Product {id: 'p095'}), (i:Ingredient) WHERE i.id IN ['ig-deepsea','ig-ha','ig-panthenol'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-deepsea'}]->(i);

// --- 남성 (p096-p100) ---
MATCH (p:Product {id: 'p096'}), (i:Ingredient) WHERE i.id IN ['ig-bha','ig-panthenol','ig-centella'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-bha'}]->(i);
MATCH (p:Product {id: 'p097'}), (i:Ingredient) WHERE i.id IN ['ig-niacinamide','ig-ha','ig-ceramide'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-niacinamide'}]->(i);
MATCH (p:Product {id: 'p098'}), (i:Ingredient) WHERE i.id IN ['ig-vite','ig-panthenol'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: false}]->(i);
MATCH (p:Product {id: 'p099'}), (i:Ingredient) WHERE i.id IN ['ig-greentea','ig-niacinamide','ig-ha'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-greentea'}]->(i);
MATCH (p:Product {id: 'p100'}), (i:Ingredient) WHERE i.id IN ['ig-deepsea','ig-ha','ig-niacinamide'] MERGE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-deepsea'}]->(i);


// ============================================================
// 2. 추천 체인 심화 (REFERRED)
//    최대 깊이 4→6, 고아 고객 17명→0명, 루트 인플루언서 4→5명
// ============================================================

// Chain 2 확장 (cu-006 루트): depth 4→6
MATCH (a:Customer {id: 'cu-042'}), (b:Customer {id: 'cu-050'}) MERGE (a)-[:REFERRED {referred_at: '2023-06-15'}]->(b);
MATCH (a:Customer {id: 'cu-050'}), (b:Customer {id: 'cu-045'}) MERGE (a)-[:REFERRED {referred_at: '2023-09-20'}]->(b);

// Chain 1 확장 (cu-001 루트): depth 3→5
MATCH (a:Customer {id: 'cu-029'}), (b:Customer {id: 'cu-043'}) MERGE (a)-[:REFERRED {referred_at: '2023-08-15'}]->(b);
MATCH (a:Customer {id: 'cu-043'}), (b:Customer {id: 'cu-003'}) MERGE (a)-[:REFERRED {referred_at: '2024-01-10'}]->(b);
MATCH (a:Customer {id: 'cu-047'}), (b:Customer {id: 'cu-014'}) MERGE (a)-[:REFERRED {referred_at: '2023-10-05'}]->(b);

// Chain 3 확장 (cu-017 루트): depth 3→5
MATCH (a:Customer {id: 'cu-033'}), (b:Customer {id: 'cu-018'}) MERGE (a)-[:REFERRED {referred_at: '2021-03-05'}]->(b);
MATCH (a:Customer {id: 'cu-036'}), (b:Customer {id: 'cu-048'}) MERGE (a)-[:REFERRED {referred_at: '2022-01-15'}]->(b);
MATCH (a:Customer {id: 'cu-044'}), (b:Customer {id: 'cu-009'}) MERGE (a)-[:REFERRED {referred_at: '2022-04-20'}]->(b);

// Chain 4 확장 (cu-020 루트): depth 3→5
MATCH (a:Customer {id: 'cu-022'}), (b:Customer {id: 'cu-005'}) MERGE (a)-[:REFERRED {referred_at: '2022-09-10'}]->(b);
MATCH (a:Customer {id: 'cu-037'}), (b:Customer {id: 'cu-011'}) MERGE (a)-[:REFERRED {referred_at: '2023-01-20'}]->(b);
MATCH (a:Customer {id: 'cu-028'}), (b:Customer {id: 'cu-041'}) MERGE (a)-[:REFERRED {referred_at: '2023-03-08'}]->(b);
MATCH (a:Customer {id: 'cu-005'}), (b:Customer {id: 'cu-025'}) MERGE (a)-[:REFERRED {referred_at: '2023-02-14'}]->(b);

// 신규 Chain 5 (root: cu-002) — 나머지 고아 고객 연결
MATCH (a:Customer {id: 'cu-002'}), (b:Customer {id: 'cu-012'}) MERGE (a)-[:REFERRED {referred_at: '2022-01-08'}]->(b);
MATCH (a:Customer {id: 'cu-002'}), (b:Customer {id: 'cu-027'}) MERGE (a)-[:REFERRED {referred_at: '2021-04-15'}]->(b);
MATCH (a:Customer {id: 'cu-012'}), (b:Customer {id: 'cu-034'}) MERGE (a)-[:REFERRED {referred_at: '2022-06-20'}]->(b);
MATCH (a:Customer {id: 'cu-027'}), (b:Customer {id: 'cu-023'}) MERGE (a)-[:REFERRED {referred_at: '2021-08-10'}]->(b);


// ============================================================
// 3. 다중 상품 장바구니 (CONTAINS on existing tx)
//    성분 충돌/시너지를 탐지할 수 있는 의도적 장바구니 설계
//    거래→고객 매핑 검증 완료 (seed.cypher UNWIND 공식 기반)
// ============================================================

// --- 충돌 장바구니 5건 (S1 시나리오 활성화) ---

// cu-026: tx-1025에 p026(retinol) 이미 있음. tx-1225에 p004(AHA/BHA) 추가 → retinol+AHA HIGH
MATCH (t:Transaction {id: 'tx-1225'}), (p:Product {id: 'p004'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 13800}]->(p);

// cu-002: tx-1101에 p004(AHA/BHA) 이미 있음. tx-1001에 p026(retinol) 추가 → retinol+AHA HIGH
MATCH (t:Transaction {id: 'tx-1001'}), (p:Product {id: 'p026'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 24000}]->(p);

// cu-004: tx-1003에 p004(AHA/BHA) 이미 있음. tx-1103에 p023(VitC) 추가 → AHA+VitC MEDIUM
MATCH (t:Transaction {id: 'tx-1103'}), (p:Product {id: 'p023'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 15800}]->(p);

// cu-009: tx-1108에 p025(VitC) 이미 있음. tx-1008에 p034(retinol) 추가 → retinol+VitC MEDIUM
MATCH (t:Transaction {id: 'tx-1008'}), (p:Product {id: 'p034'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 89000}]->(p);

// cu-034: tx-1033에 p034(retinol) 이미 있음. tx-1433에 p011(BHA) 추가 → retinol+BHA HIGH
MATCH (t:Transaction {id: 'tx-1433'}), (p:Product {id: 'p011'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 15000}]->(p);

// --- 시너지 장바구니 8건 (S2 시나리오 활성화) ---

// VitC+VitE 시너지 (boost 400%) — 가장 강력한 시너지
// cu-003: tx-1002에 p003(ha,panthenol,aloe). tx-1002에 p018(VitC+VitE) 추가
MATCH (t:Transaction {id: 'tx-1002'}), (p:Product {id: 'p018'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 16000}]->(p);

// Centella+Panthenol 시너지 (boost 80%)
// cu-005: tx-1004에 p005(ha,panthenol,centella). tx-1104에 p019(centella) 추가
MATCH (t:Transaction {id: 'tx-1104'}), (p:Product {id: 'p019'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 18500}]->(p);

// Ceramide+HA 시너지 (boost 60%)
// cu-006: tx-1005에 p006(ha,niacinamide). tx-1105에 p030(ceramide) 추가
MATCH (t:Transaction {id: 'tx-1105'}), (p:Product {id: 'p030'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 18900}]->(p);

// Niacinamide+HA 시너지 (boost 50%)
// cu-010: tx-1009에 p010(greentea,ha,niacinamide). tx-1109에 p014(ha,panthenol,ceramide) 추가
MATCH (t:Transaction {id: 'tx-1109'}), (p:Product {id: 'p014'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 18500}]->(p);

// Snail+HA 시너지 (boost 50%)
// cu-012: tx-1011에 p012(noni). tx-1411에 p013(snail) 추가
MATCH (t:Transaction {id: 'tx-1411'}), (p:Product {id: 'p013'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 16800}]->(p);

// Retinol+Adenosine 시너지 (boost 70%)
// cu-025: tx-1024에 p025(VitC). tx-1424에 p034(retinol+adenosine) 추가
MATCH (t:Transaction {id: 'tx-1424'}), (p:Product {id: 'p034'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 89000}]->(p);

// Niacinamide+Retinol 시너지 (boost 30%)
// cu-015: tx-1014에 p015(glutathione,VitC,niacinamide). tx-1414에 p026(retinol) 추가
MATCH (t:Transaction {id: 'tx-1414'}), (p:Product {id: 'p026'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 24000}]->(p);

// VitC+Centella 시너지 (boost 40%)
// cu-020: tx-1019에 p020(galactomyces). tx-1119에 p023(VitC) 추가
MATCH (t:Transaction {id: 'tx-1119'}), (p:Product {id: 'p023'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 15800}]->(p);

// --- 루틴 장바구니 7건 (S4 시나리오 활성화) ---

// 토너+세럼 조합
MATCH (t:Transaction {id: 'tx-1010'}), (p:Product {id: 'p014'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 18500}]->(p);
MATCH (t:Transaction {id: 'tx-1015'}), (p:Product {id: 'p001'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 13500}]->(p);

// 세럼+크림 조합
MATCH (t:Transaction {id: 'tx-1020'}), (p:Product {id: 'p030'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 18900}]->(p);
MATCH (t:Transaction {id: 'tx-1025'}), (p:Product {id: 'p031'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 22000}]->(p);

// 토너+크림 조합
MATCH (t:Transaction {id: 'tx-1030'}), (p:Product {id: 'p028'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 19500}]->(p);
MATCH (t:Transaction {id: 'tx-1035'}), (p:Product {id: 'p005'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 15800}]->(p);
MATCH (t:Transaction {id: 'tx-1040'}), (p:Product {id: 'p029'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 19800}]->(p);


// ============================================================
// 4. 시너지/충돌 쌍 확대
//    피부과 문헌 기반 실제 성분 상호작용 추가
// ============================================================

// --- 시너지 4쌍 ---
// 비타민C → 콜라겐 합성 촉진 (피부과 문헌 확립)
MATCH (a:Ingredient {id: 'ig-collagen'}), (b:Ingredient {id: 'ig-vitc'})
MERGE (a)-[:SYNERGIZES_WITH {boost_pct: 60, mechanism: '비타민C가 프롤린 수산화를 촉진하여 콜라겐 합성을 가속 (Pullar et al., Nutrients 2017)'}]->(b);

// 세라마이드 → 레티놀 건조함 완화 (피부 장벽 보호)
MATCH (a:Ingredient {id: 'ig-retinol'}), (b:Ingredient {id: 'ig-ceramide'})
MERGE (a)-[:SYNERGIZES_WITH {boost_pct: 40, mechanism: '세라마이드가 레티놀 사용 시 발생하는 경피수분손실(TEWL)을 감소시켜 자극 완화'}]->(b);

// 나이아신아마이드+아데노신 복합 항노화
MATCH (a:Ingredient {id: 'ig-niacinamide'}), (b:Ingredient {id: 'ig-adenosine'})
MERGE (a)-[:SYNERGIZES_WITH {boost_pct: 35, mechanism: '나이아신아마이드(세포 에너지 대사) + 아데노신(콜라겐 합성)의 이중 항노화 경로 활성화'}]->(b);

// 히알루론산+알로에 이중 보습
MATCH (a:Ingredient {id: 'ig-ha'}), (b:Ingredient {id: 'ig-aloe'})
MERGE (a)-[:SYNERGIZES_WITH {boost_pct: 30, mechanism: '히알루론산(수분 흡착 보습) + 알로에(수분 밀봉 보습)의 이중 보습 레이어 형성'}]->(b);

// --- 충돌 2쌍 ---
// BHA+VitC: pH 감도 (낮은 위험)
MATCH (a:Ingredient {id: 'ig-bha'}), (b:Ingredient {id: 'ig-vitc'})
MERGE (a)-[:CONFLICTS_WITH {risk_level: 'low', reason: 'BHA(pH 3-4)와 비타민C(pH 2-3) 동시 사용 시 피부 자극 가능. 시간차 사용 권장'}]->(b);

// VitC+Niacinamide: 논쟁적 충돌 (데모 가치 높음)
MATCH (a:Ingredient {id: 'ig-vitc'}), (b:Ingredient {id: 'ig-niacinamide'})
MERGE (a)-[:CONFLICTS_WITH {risk_level: 'low', reason: 'pH 차이로 효능 감소 가능성 (과거 연구). 최근 재평가에서는 병용 안전으로 결론. 민감 피부는 시간차 사용 권장'}]->(b);


// ============================================================
// 5. 계절 패턴 거래 (10건)
//    비서울 지역, 다중 상품 장바구니, 계절 패턴 반영
// ============================================================

// --- 3월: 봄맞이 스킨케어 (토너+세럼 번들) ---
MERGE (t1:Transaction {id: 'tx-2001'}) SET t1.total_amount = 32300, t1.payment_method = 'kakao_pay', t1.purchased_at = '2025-03-15T14:30:00';
MATCH (t:Transaction {id: 'tx-2001'}), (s:Store {id: 'st-busan-haeundae'}) MERGE (t)-[:AT_STORE]->(s);
MATCH (t:Transaction {id: 'tx-2001'}), (c:Customer {id: 'cu-007'}) MERGE (c)-[:PURCHASED]->(t);
MATCH (t:Transaction {id: 'tx-2001'}), (p:Product {id: 'p001'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 13500}]->(p);
MATCH (t:Transaction {id: 'tx-2001'}), (p:Product {id: 'p014'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 18500}]->(p);

MERGE (t2:Transaction {id: 'tx-2002'}) SET t2.total_amount = 37800, t2.payment_method = 'card', t2.purchased_at = '2025-03-18T11:15:00';
MATCH (t:Transaction {id: 'tx-2002'}), (s:Store {id: 'st-daegu'}) MERGE (t)-[:AT_STORE]->(s);
MATCH (t:Transaction {id: 'tx-2002'}), (c:Customer {id: 'cu-016'}) MERGE (c)-[:PURCHASED]->(t);
MATCH (t:Transaction {id: 'tx-2002'}), (p:Product {id: 'p005'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 15800}]->(p);
MATCH (t:Transaction {id: 'tx-2002'}), (p:Product {id: 'p031'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 22000}]->(p);

MERGE (t3:Transaction {id: 'tx-2003'}) SET t3.total_amount = 34500, t3.payment_method = 'naver_pay', t3.purchased_at = '2025-03-22T16:45:00';
MATCH (t:Transaction {id: 'tx-2003'}), (s:Store {id: 'st-busan-nampo'}) MERGE (t)-[:AT_STORE]->(s);
MATCH (t:Transaction {id: 'tx-2003'}), (c:Customer {id: 'cu-013'}) MERGE (c)-[:PURCHASED]->(t);
MATCH (t:Transaction {id: 'tx-2003'}), (p:Product {id: 'p002'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 18000}]->(p);
MATCH (t:Transaction {id: 'tx-2003'}), (p:Product {id: 'p016'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 17000}]->(p);

// --- 6-7월: 여름 선케어 피크 (선크림+클렌저 번들) ---
MERGE (t4:Transaction {id: 'tx-2004'}) SET t4.total_amount = 31800, t4.payment_method = 'kakao_pay', t4.purchased_at = '2025-06-20T10:30:00';
MATCH (t:Transaction {id: 'tx-2004'}), (s:Store {id: 'st-incheon'}) MERGE (t)-[:AT_STORE]->(s);
MATCH (t:Transaction {id: 'tx-2004'}), (c:Customer {id: 'cu-022'}) MERGE (c)-[:PURCHASED]->(t);
MATCH (t:Transaction {id: 'tx-2004'}), (p:Product {id: 'p038'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 15000}]->(p);
MATCH (t:Transaction {id: 'tx-2004'}), (p:Product {id: 'p051'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 15000}]->(p);

MERGE (t5:Transaction {id: 'tx-2005'}) SET t5.total_amount = 31600, t5.payment_method = 'card', t5.purchased_at = '2025-07-05T15:20:00';
MATCH (t:Transaction {id: 'tx-2005'}), (s:Store {id: 'st-gwangju'}) MERGE (t)-[:AT_STORE]->(s);
MATCH (t:Transaction {id: 'tx-2005'}), (c:Customer {id: 'cu-042'}) MERGE (c)-[:PURCHASED]->(t);
MATCH (t:Transaction {id: 'tx-2005'}), (p:Product {id: 'p040'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 16800}]->(p);
MATCH (t:Transaction {id: 'tx-2005'}), (p:Product {id: 'p054'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 17000}]->(p);

MERGE (t6:Transaction {id: 'tx-2006'}) SET t6.total_amount = 32300, t6.payment_method = 'naver_pay', t6.purchased_at = '2025-07-12T13:10:00';
MATCH (t:Transaction {id: 'tx-2006'}), (s:Store {id: 'st-daejeon'}) MERGE (t)-[:AT_STORE]->(s);
MATCH (t:Transaction {id: 'tx-2006'}), (c:Customer {id: 'cu-025'}) MERGE (c)-[:PURCHASED]->(t);
MATCH (t:Transaction {id: 'tx-2006'}), (p:Product {id: 'p042'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 16500}]->(p);
MATCH (t:Transaction {id: 'tx-2006'}), (p:Product {id: 'p052'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 9800}]->(p);

// --- 11월: 올영세일/블프 (3개 상품 번들) ---
MERGE (t7:Transaction {id: 'tx-2007'}) SET t7.total_amount = 52200, t7.payment_method = 'card', t7.purchased_at = '2025-11-21T10:05:00';
MATCH (t:Transaction {id: 'tx-2007'}), (s:Store {id: 'st-suwon'}) MERGE (t)-[:AT_STORE]->(s);
MATCH (t:Transaction {id: 'tx-2007'}), (c:Customer {id: 'cu-004'}) MERGE (c)-[:PURCHASED]->(t);
MATCH (t:Transaction {id: 'tx-2007'}), (p:Product {id: 'p001'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 13500}]->(p);
MATCH (t:Transaction {id: 'tx-2007'}), (p:Product {id: 'p014'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 18500}]->(p);
MATCH (t:Transaction {id: 'tx-2007'}), (p:Product {id: 'p030'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 18900}]->(p);

MERGE (t8:Transaction {id: 'tx-2008'}) SET t8.total_amount = 65800, t8.payment_method = 'kakao_pay', t8.purchased_at = '2025-11-23T14:30:00';
MATCH (t:Transaction {id: 'tx-2008'}), (s:Store {id: 'st-bundang'}) MERGE (t)-[:AT_STORE]->(s);
MATCH (t:Transaction {id: 'tx-2008'}), (c:Customer {id: 'cu-010'}) MERGE (c)-[:PURCHASED]->(t);
MATCH (t:Transaction {id: 'tx-2008'}), (p:Product {id: 'p005'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 15800}]->(p);
MATCH (t:Transaction {id: 'tx-2008'}), (p:Product {id: 'p019'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 18500}]->(p);
MATCH (t:Transaction {id: 'tx-2008'}), (p:Product {id: 'p028'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 19500}]->(p);

MERGE (t9:Transaction {id: 'tx-2009'}) SET t9.total_amount = 57700, t9.payment_method = 'card', t9.purchased_at = '2025-11-25T11:45:00';
MATCH (t:Transaction {id: 'tx-2009'}), (s:Store {id: 'st-ilsan'}) MERGE (t)-[:AT_STORE]->(s);
MATCH (t:Transaction {id: 'tx-2009'}), (c:Customer {id: 'cu-033'}) MERGE (c)-[:PURCHASED]->(t);
MATCH (t:Transaction {id: 'tx-2009'}), (p:Product {id: 'p002'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 18000}]->(p);
MATCH (t:Transaction {id: 'tx-2009'}), (p:Product {id: 'p013'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 16800}]->(p);
MATCH (t:Transaction {id: 'tx-2009'}), (p:Product {id: 'p031'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 22000}]->(p);

MERGE (t10:Transaction {id: 'tx-2010'}) SET t10.total_amount = 48300, t10.payment_method = 'naver_pay', t10.purchased_at = '2025-11-28T17:20:00';
MATCH (t:Transaction {id: 'tx-2010'}), (s:Store {id: 'st-hanam'}) MERGE (t)-[:AT_STORE]->(s);
MATCH (t:Transaction {id: 'tx-2010'}), (c:Customer {id: 'cu-038'}) MERGE (c)-[:PURCHASED]->(t);
MATCH (t:Transaction {id: 'tx-2010'}), (p:Product {id: 'p003'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 14900}]->(p);
MATCH (t:Transaction {id: 'tx-2010'}), (p:Product {id: 'p017'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 20000}]->(p);
MATCH (t:Transaction {id: 'tx-2010'}), (p:Product {id: 'p029'}) MERGE (t)-[:CONTAINS {quantity: 1, unit_price: 19800}]->(p);
