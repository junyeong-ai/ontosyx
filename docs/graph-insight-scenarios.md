# Ontosyx — 그래프 기반 인사이트 시나리오

> RDS(관계형 데이터베이스)에서는 도출 불가능하거나 비현실적인 복잡한 인사이트를,
> 온톨로지 기반 그래프 DB로 어떻게 즉시 도출하는지 비교합니다.

**데이터셋**: Olive Young 뷰티 리테일 (100 제품, 31 브랜드, 25 성분, 8 피부 고민, 50 고객, 210 거래)

---

## 시나리오 1: 성분 충돌 안전 경고

### 질문
> "레티놀과 AHA(글리콜산)를 동시에 사용하는 고객은 누구인가?
> 그들의 피부 고민에 실제로 악영향을 주는 조합인가?"

### 왜 이것이 가치 있는가

뷰티 업계에서 **성분 간 충돌**(contraindication)은 피부 자극, 화학적 화상, 과민 반응을 유발할 수 있다.
고객이 각각 안전한 제품 A와 B를 구매해도, **두 제품의 핵심 성분이 충돌하면 위험**하다.
이 관계는 개별 제품 테이블에는 존재하지 않고, **성분 간 관계 그래프에서만 파악 가능**하다.

### RDS 접근 (SQL)

```sql
-- 동일 고객이 구매한 서로 다른 두 제품의 성분이 충돌하는지 확인
-- 필요한 JOIN: 6개 테이블 + self-join

SELECT DISTINCT
    c.name AS customer_name,
    p1.name AS product_1,
    i1.name AS ingredient_1,
    p2.name AS product_2,
    i2.name AS ingredient_2,
    ic.risk_level
FROM customers c
JOIN transactions t1 ON t1.customer_id = c.id
JOIN transaction_items ti1 ON ti1.transaction_id = t1.id
JOIN products p1 ON p1.id = ti1.product_id
JOIN product_ingredients pi1 ON pi1.product_id = p1.id
JOIN ingredients i1 ON i1.id = pi1.ingredient_id
-- 같은 고객의 다른 구매
JOIN transactions t2 ON t2.customer_id = c.id AND t2.id != t1.id
JOIN transaction_items ti2 ON ti2.transaction_id = t2.id
JOIN products p2 ON p2.id = ti2.product_id
JOIN product_ingredients pi2 ON pi2.product_id = p2.id
JOIN ingredients i2 ON i2.id = pi2.ingredient_id
-- 두 성분 간 충돌 관계 확인
JOIN ingredient_conflicts ic
    ON (ic.ingredient_a_id = i1.id AND ic.ingredient_b_id = i2.id)
    OR (ic.ingredient_a_id = i2.id AND ic.ingredient_b_id = i1.id)
WHERE i1.id != i2.id;
```

### RDS의 문제

```
customers ─┬─ transactions ─ transaction_items ─ products ─ product_ingredients ─ ingredients
           │                                                                         │
           └─ transactions ─ transaction_items ─ products ─ product_ingredients ─ ingredients
                                                                                     │
                                                                    ingredient_conflicts (self-join)
```

| 문제 | 영향 |
|------|------|
| **11-table JOIN** | 쿼리 플래너가 최적 실행 계획을 찾지 못함 |
| **Self-join on transactions** | 고객당 거래가 N건이면 N² 조합 탐색 |
| **양방향 충돌 확인** | `OR` 조건으로 인덱스 활용 불가 |
| **실행 시간** | 고객 1만명, 거래 10만건 기준 **수십 초~수 분** |
| **확장성** | 3개 이상 성분 충돌 분석 시 JOIN 기하급수 증가 |

### 그래프 접근 (Cypher)

```cypher
MATCH (c:Customer)-[:PURCHASED]->(t:Transaction)-[:CONTAINS]->(p1:Product)
        -[:HAS_INGREDIENT]->(i1:Ingredient)-[:CONFLICTS_WITH]->(i2:Ingredient)
        <-[:HAS_INGREDIENT]-(p2:Product)<-[:CONTAINS]-(t2:Transaction)
        <-[:PURCHASED]-(c)
WHERE p1 <> p2
RETURN c.name AS customer,
       p1.name AS product_1, i1.name AS ingredient_1,
       p2.name AS product_2, i2.name AS ingredient_2
LIMIT 25
```

### 그래프 관계 다이어그램

```
                    ┌──────────┐
                    │ Customer │
                    │  "김민지"  │
                    └────┬─────┘
               PURCHASED │ PURCHASED
              ┌──────────┴──────────┐
              ▼                     ▼
        ┌───────────┐         ┌───────────┐
        │Transaction│         │Transaction│
        │   T-042   │         │   T-089   │
        └─────┬─────┘         └─────┬─────┘
         CONTAINS                CONTAINS
              ▼                     ▼
      ┌──────────────┐      ┌──────────────┐
      │   Product    │      │   Product    │
      │ 레티놀 세럼   │      │ AHA 토너     │
      └──────┬───────┘      └──────┬───────┘
        HAS_INGREDIENT         HAS_INGREDIENT
              ▼                     ▼
      ┌──────────────┐      ┌──────────────┐
      │  Ingredient  │      │  Ingredient  │
      │   레티놀      │◄────►│  AHA(글리콜산) │
      │  EWG: 4      │ ❌    │  EWG: 4      │
      └──────────────┘CONFLICTS└──────────────┘
                      _WITH
                   risk: HIGH
```

### 비교 요약

| 항목 | RDS (SQL) | 그래프 (Cypher) |
|------|:---------:|:--------------:|
| JOIN 수 | 11개 | 0개 (패턴 매칭) |
| 쿼리 복잡도 | 35줄+ | 8줄 |
| 실행 시간 (1만 고객) | 수십 초 | **< 1초** |
| 3-way 충돌 확장 | 사실상 불가 | 패턴에 노드 추가 |
| 비즈니스 가치 | 배치 분석만 가능 | **실시간 경고** |

---

## 시나리오 2: 규제 변경 연쇄 영향 분석

### 질문
> "EU가 레티놀 최대 농도를 0.05%로 강화하면,
> 영향받는 제품 → 브랜드 → 매장 → 고객까지 전체 영향 범위는?"

### 왜 이것이 가치 있는가

화장품 규제 변경은 **하나의 성분**에서 시작하여 **제품 → 브랜드 → 유통 → 소비자**까지
**4단계 연쇄 영향**을 미친다. RDS에서는 각 단계를 별도 쿼리로 실행하고 결과를 수동 집계해야 한다.
그래프에서는 **단일 쿼리로 전체 영향 체인을 즉시 추적**할 수 있다.

### RDS 접근 — 4단계 수동 분석 필요

```sql
-- Step 1: 영향받는 성분
SELECT id FROM ingredients WHERE name = '레티놀';

-- Step 2: 해당 성분이 기준 초과인 제품
SELECT p.id, p.name, pi.concentration_pct
FROM products p
JOIN product_ingredients pi ON pi.product_id = p.id
JOIN ingredients i ON i.id = pi.ingredient_id
WHERE i.name = '레티놀' AND pi.concentration_pct > 0.05;

-- Step 3: 해당 제품의 브랜드
SELECT DISTINCT b.name
FROM brands b
JOIN products p ON p.brand_id = b.id
WHERE p.id IN (... Step 2 결과 ...);

-- Step 4: 해당 제품을 구매한 고객
SELECT DISTINCT c.name, c.email
FROM customers c
JOIN transactions t ON t.customer_id = c.id
JOIN transaction_items ti ON ti.transaction_id = t.id
WHERE ti.product_id IN (... Step 2 결과 ...);

-- Step 5: 해당 제품을 취급하는 매장
SELECT DISTINCT s.name, s.address
FROM stores s
JOIN transactions t ON t.store_id = s.id
JOIN transaction_items ti ON ti.transaction_id = t.id
WHERE ti.product_id IN (... Step 2 결과 ...);
```

### RDS의 문제

| 문제 | 영향 |
|------|------|
| **5개 별도 쿼리** 필요 | 중간 결과를 애플리케이션에서 수동 조합 |
| **IN 절 크기 제한** | 영향 제품이 1000개+ 이면 쿼리 실패 |
| **영향 범위 집계 불가** | "총 몇 명의 고객이 영향받는가?" — 별도 COUNT 필요 |
| **연쇄 관계 시각화 불가** | 규제→성분→제품→브랜드→매장 체인을 하나의 뷰로 볼 수 없음 |

### 그래프 접근 (Cypher)

```cypher
MATCH (r:Regulation {authority: 'EU_SCCS'})-[:REGULATED_BY]-(i:Ingredient {name: '레티놀'})
MATCH (p:Product)-[hi:HAS_INGREDIENT]->(i)
WHERE hi.concentration_pct > 0.05
MATCH (p)-[:MADE_BY]->(b:Brand)
OPTIONAL MATCH (p)<-[:CONTAINS]-(t:Transaction)<-[:PURCHASED]-(c:Customer)
OPTIONAL MATCH (t)-[:AT_STORE]->(s:Store)-[:LOCATED_IN]->(reg:Region)
RETURN r.name AS regulation,
       i.name AS ingredient,
       p.name AS product, hi.concentration_pct AS concentration,
       b.name AS brand,
       count(DISTINCT c) AS affected_customers,
       collect(DISTINCT s.name) AS affected_stores,
       collect(DISTINCT reg.name) AS affected_regions
ORDER BY affected_customers DESC
```

### 영향 연쇄 다이어그램

```
┌────────────────┐
│   Regulation   │
│ EU 레티놀 규제   │ ◄── 규제 강화 트리거
│ max: 0.05%      │
└───────┬────────┘
        │ REGULATED_BY
        ▼
┌────────────────┐
│   Ingredient   │
│    레티놀       │ EWG: 4
└───────┬────────┘
        │ HAS_INGREDIENT (concentration > 0.3%)
        ▼
┌────────────────┐     ┌────────────────┐     ┌────────────────┐
│    Product     │     │    Product     │     │    Product     │
│ 레티놀 세럼 A   │     │ 레티놀 크림 B   │     │ 안티에이징 앰플 │
│ conc: 0.5%  ❌ │     │ conc: 1.0%  ❌ │     │ conc: 0.2%  ✅ │
└───┬────┬───────┘     └───┬────┬───────┘     └────────────────┘
    │    │                  │    │                  (규제 미해당)
    │    │ MADE_BY          │    │ MADE_BY
    │    ▼                  │    ▼
    │  ┌──────────┐        │  ┌──────────┐
    │  │  Brand   │        │  │  Brand   │
    │  │ 코스알엑스 │        │  │ 라운드랩  │
    │  └──────────┘        │  └──────────┘
    │                       │
    │ CONTAINS              │ CONTAINS
    ▼                       ▼
┌──────────┐           ┌──────────┐
│ 32 고객   │           │ 18 고객   │
│ 5 매장    │           │ 3 매장    │
│ 3 지역    │           │ 2 지역    │
└──────────┘           └──────────┘

총 영향: 제품 2개, 브랜드 2개, 고객 47명, 매장 7개, 지역 4개
```

### 비교 요약

| 항목 | RDS | 그래프 |
|------|:---:|:-----:|
| 쿼리 수 | 5개 (수동 체이닝) | **1개** |
| 전체 영향 범위 집계 | 애플리케이션 코드 필요 | 쿼리 내 `count(DISTINCT)` |
| 새 규제 추가 시 | 5개 쿼리 모두 수정 | 패턴 동일, 파라미터만 변경 |
| 시각화 | 불가 (테이블 형태만) | **그래프 탐색 뷰** |

---

## 시나리오 3: 성분 시너지 최적 루틴 추천

### 질문
> "건조 + 노화 피부에 최적인 2개 제품 조합은?
> 성분 시너지(boost_pct)가 가장 높고, 성분 충돌이 없는 안전한 조합."

### 왜 이것이 가치 있는가

뷰티 루틴은 **여러 제품의 성분이 상호작용**하여 효과가 증폭되거나 상쇄된다.
"히알루론산 + 나이아신아마이드 = 보습 효과 50% 증가" 같은 시너지는
개별 제품 스펙에는 없고 **성분 간 관계 그래프에서만 도출** 가능하다.

### RDS 접근 — 사실상 불가능

```sql
-- 건조+노화를 동시에 치료하는 성분 조합 중 시너지가 있는 것
-- 필요: 6-table JOIN + self-join + NOT EXISTS subquery

SELECT
    p1.name AS product_1, p2.name AS product_2,
    i1.name AS ingredient_1, i2.name AS ingredient_2,
    syn.boost_pct,
    syn.mechanism
FROM ingredients i1
-- 건조 치료
JOIN ingredient_treats it1 ON it1.ingredient_id = i1.id
JOIN skin_concerns sc1 ON sc1.id = it1.skin_concern_id AND sc1.name = '건조'
-- 노화 치료
JOIN ingredients i2 ON i2.id != i1.id
JOIN ingredient_treats it2 ON it2.ingredient_id = i2.id
JOIN skin_concerns sc2 ON sc2.id = it2.skin_concern_id AND sc2.name = '노화'
-- 시너지 관계
JOIN ingredient_synergies syn
    ON (syn.ingredient_a_id = i1.id AND syn.ingredient_b_id = i2.id)
    OR (syn.ingredient_a_id = i2.id AND syn.ingredient_b_id = i1.id)
-- 충돌 없음 확인
AND NOT EXISTS (
    SELECT 1 FROM ingredient_conflicts ic
    WHERE (ic.ingredient_a_id = i1.id AND ic.ingredient_b_id = i2.id)
       OR (ic.ingredient_a_id = i2.id AND ic.ingredient_b_id = i1.id)
)
-- 해당 성분을 포함하는 제품 매핑
JOIN product_ingredients pi1 ON pi1.ingredient_id = i1.id
JOIN products p1 ON p1.id = pi1.product_id
JOIN product_ingredients pi2 ON pi2.ingredient_id = i2.id
JOIN products p2 ON p2.id = pi2.product_id
WHERE p1.id != p2.id
ORDER BY syn.boost_pct DESC
LIMIT 5;
```

### RDS의 문제

| 문제 | 영향 |
|------|------|
| **12-table JOIN** | 사실상 실행 불가능한 쿼리 |
| **양방향 시너지 + 양방향 충돌** | `OR` 조건 4개 → 인덱스 무력화 |
| **NOT EXISTS subquery** | 각 행마다 충돌 테이블 전체 스캔 |
| **3가지 피부 고민 확장** | JOIN 수 × 1.5 증가 |
| **유지보수** | 비즈니스 로직 변경 시 쿼리 전면 재작성 |

### 그래프 접근 (Cypher)

```cypher
// 건조를 치료하는 성분들
MATCH (sc1:SkinConcern {name: '건조'})<-[:TREATS]-(i1:Ingredient)
// 노화를 치료하는 성분들
MATCH (sc2:SkinConcern {name: '노화'})<-[:TREATS]-(i2:Ingredient)
// 두 성분 간 시너지 (방향 무관)
MATCH (i1)-[syn:SYNERGIZES_WITH]-(i2)
// 충돌 없음 확인
WHERE NOT (i1)-[:CONFLICTS_WITH]-(i2)
// 해당 성분을 포함하는 제품
MATCH (p1:Product)-[:HAS_INGREDIENT]->(i1)
MATCH (p2:Product)-[:HAS_INGREDIENT]->(i2)
WHERE p1 <> p2
RETURN p1.name AS product_1, i1.name AS ingredient_1,
       p2.name AS product_2, i2.name AS ingredient_2,
       syn.boost_pct AS synergy_boost,
       syn.mechanism AS how_it_works
ORDER BY syn.boost_pct DESC
LIMIT 5
```

### 시너지 네트워크 다이어그램

```
                ┌──────────────┐
                │  SkinConcern │
                │    건조       │
                └──────┬───────┘
                  TREATS│
              ┌────────┴────────┐
              ▼                 ▼
      ┌──────────────┐  ┌──────────────┐
      │  Ingredient  │  │  Ingredient  │
      │ 히알루론산    │  │  세라마이드   │
      │  EWG: 1      │  │  EWG: 1      │
      └──────┬───────┘  └──────────────┘
             │
    SYNERGIZES_WITH
    boost: +50%
    "수분 보호막 강화"
             │
             ▼
      ┌──────────────┐
      │  Ingredient  │
      │ 나이아신아마이드│──── TREATS ────►┌──────────────┐
      │  EWG: 1      │                 │  SkinConcern │
      └──────────────┘                 │    노화       │
             ▲                          └──────────────┘
    NOT CONFLICTS ✅
    (안전한 조합)
             │
      ┌──────────────┐          ┌──────────────┐
      │   Product    │          │   Product    │
      │ 히알루론 토너  │   +      │ 나이아신 세럼  │
      │ ₩15,000      │          │ ₩22,000      │
      └──────────────┘          └──────────────┘

      ═══════════════════════════════════════════
       추천 루틴: 히알루론 토너 → 나이아신 세럼
       시너지 효과: +50% 보습 강화
       안전성: ✅ 충돌 없음
      ═══════════════════════════════════════════
```

---

## 시나리오 4: 고객 추천 네트워크 인플루언서 분석

### 질문
> "추천(referral) 체인에서 3단계 이상 연결된 네트워크의 허브 고객은?
> 그들의 구매 패턴이 네트워크 전체에 미친 영향은?"

### 왜 이것이 가치 있는가

고객 추천 프로그램에서 **직접 추천(1단계)**만 추적하면 전체 그림의 10%만 보는 것이다.
**3~5단계에 걸친 추천 네트워크의 허브**를 발견하면, 소수의 핵심 인플루언서에게
집중 마케팅하여 **네트워크 효과를 극대화**할 수 있다.

### RDS 접근 — 재귀 CTE (3단계까지만 현실적)

```sql
WITH RECURSIVE referral_chain AS (
    -- 시작: 직접 추천
    SELECT referred_by AS referrer, id AS referred, 1 AS depth
    FROM customers WHERE referred_by IS NOT NULL

    UNION ALL

    -- 재귀: 다음 단계
    SELECT rc.referrer, c.id, rc.depth + 1
    FROM referral_chain rc
    JOIN customers c ON c.referred_by = rc.referred
    WHERE rc.depth < 5  -- 5단계까지만 (성능 한계)
)
SELECT
    referrer,
    COUNT(DISTINCT referred) AS network_size,
    MAX(depth) AS max_depth
FROM referral_chain
GROUP BY referrer
ORDER BY network_size DESC
LIMIT 10;
```

### RDS의 문제

| 문제 | 영향 |
|------|------|
| **재귀 CTE 깊이 제한** | 5단계 넘으면 성능 급격 저하 |
| **순환 참조** | A→B→C→A 감지 로직 필요 (추가 복잡도) |
| **네트워크 중심성** | PageRank 계산 불가 (SQL로 표현 불가) |
| **구매 전파 분석** | 추천자의 구매가 피추천자에 미친 영향 분석 = 추가 JOIN 폭발 |

### 그래프 접근

```cypher
// 1. 변수 길이 경로로 전체 추천 네트워크 탐색
MATCH path = (root:Customer)-[:REFERRED*1..5]->(leaf:Customer)
WHERE NOT ()-[:REFERRED]->(root)  // 최상위 추천자만
WITH root, collect(DISTINCT leaf) AS network, length(path) AS depth
RETURN root.name AS influencer,
       root.membership_tier AS tier,
       size(network) AS network_size,
       max(depth) AS max_depth
ORDER BY network_size DESC
LIMIT 10

// 2. 인플루언서의 구매가 네트워크에 전파된 제품
MATCH (influencer:Customer {name: '결과에서 선택'})-[:REFERRED*1..3]->(follower:Customer)
MATCH (influencer)-[:PURCHASED]->()-[:CONTAINS]->(p:Product)
MATCH (follower)-[:PURCHASED]->()-[:CONTAINS]->(p)
RETURN p.name AS shared_product, count(DISTINCT follower) AS followers_who_bought
ORDER BY followers_who_bought DESC
```

### 네트워크 다이어그램

```
                         ┌─────────────┐
                         │  Customer   │
                         │  "박서연"    │ ◄── Hub (PageRank: 0.89)
                         │  Gold 회원   │
                         └──────┬──────┘
                    REFERRED    │    REFERRED
                 ┌──────────────┼──────────────┐
                 ▼              ▼              ▼
           ┌──────────┐  ┌──────────┐  ┌──────────┐
           │ "김민지"  │  │ "이수진"  │  │ "정하은"  │
           │ Pink 회원  │  │ Green 회원│  │ Gold 회원 │
           └─────┬────┘  └──────────┘  └─────┬────┘
            REFERRED                     REFERRED
           ┌────┴────┐                  ┌────┴────┐
           ▼         ▼                  ▼         ▼
      ┌────────┐ ┌────────┐       ┌────────┐ ┌────────┐
      │"최유나" │ │"강지원" │       │"윤서아" │ │"임채원" │
      └────────┘ └────────┘       └────────┘ └────────┘

      박서연의 네트워크: 7명 (3단계)
      공통 구매 제품: 코스알엑스 스네일 에센스 (5/7명 구매)
      네트워크 매출 기여: ₩1,240,000
```

---

## 시나리오 5: 성분 이중 역할 파라독스

### 질문
> "특정 피부 고민을 치료하면서 동시에 다른 고민을 악화시키는 성분은?
> 이 '이중 역할' 성분을 회피하면서 두 고민 모두 해결하는 대안 제품은?"

### 왜 이것이 가치 있는가

**레티놀**은 노화를 치료하지만 민감성 피부를 악화시킨다.
"노화 + 민감" 피부를 가진 고객에게 레티놀을 추천하면 **역효과**가 발생한다.
이 **이중 역할 파라독스**는 제품 DB만으로는 절대 발견할 수 없고,
**성분의 TREATS + AGGRAVATES 관계를 동시에 분석**해야만 파악 가능하다.

### RDS 접근

```sql
-- 동일 성분이 하나의 고민을 치료하면서 다른 고민을 악화
SELECT
    i.name AS ingredient,
    sc_treat.name AS treats,
    it.efficacy_level AS treat_level,
    sc_agg.name AS aggravates,
    ia.severity AS aggravate_severity
FROM ingredients i
JOIN ingredient_treats it ON it.ingredient_id = i.id
JOIN skin_concerns sc_treat ON sc_treat.id = it.skin_concern_id
JOIN ingredient_aggravates ia ON ia.ingredient_id = i.id
JOIN skin_concerns sc_agg ON sc_agg.id = ia.skin_concern_id
WHERE sc_treat.id != sc_agg.id;

-- 이제 대안 찾기: 같은 고민을 치료하지만 악화시키지 않는 성분
-- + 해당 성분을 포함하는 제품
-- = 추가 4-table JOIN 필요
```

### 그래프 접근

```cypher
// 이중 역할 성분 발견
MATCH (i:Ingredient)-[t:TREATS]->(treats:SkinConcern),
      (i)-[a:AGGRAVATES]->(aggravates:SkinConcern)
WHERE treats <> aggravates

// 안전한 대안 성분 찾기
MATCH (alt:Ingredient)-[:TREATS]->(treats)
WHERE NOT (alt)-[:AGGRAVATES]->(aggravates)
  AND alt <> i

// 대안 성분을 포함하는 제품
MATCH (safe_product:Product)-[:HAS_INGREDIENT]->(alt)

RETURN i.name AS paradox_ingredient,
       treats.name AS treats_concern,
       aggravates.name AS aggravates_concern,
       alt.name AS safe_alternative,
       safe_product.name AS recommended_product
```

### 이중 역할 다이어그램

```
                    ┌──────────────┐
          TREATS    │  SkinConcern │    TREATS
       ┌───────────►│    노화      │◄───────────┐
       │    ✅      └──────────────┘      ✅     │
       │                                         │
┌──────────────┐                         ┌──────────────┐
│  Ingredient  │                         │  Ingredient  │
│   레티놀      │      ≠ 대안 ≠          │  펩타이드     │
│  EWG: 4      │   ─ ─ ─ ─ ─ ─ ─ ►     │  EWG: 1      │
└──────┬───────┘                         └──────────────┘
       │                                         │
       │ AGGRAVATES                     NOT AGGRAVATES
       │    ❌                                ✅
       ▼
┌──────────────┐
│  SkinConcern │
│    민감       │  ◄── "펩타이드는 민감 피부를
└──────────────┘       악화시키지 않는다"

═══════════════════════════════════════════════
 파라독스: 레티놀은 노화를 치료하지만 민감 피부를 악화
 해결: 펩타이드 기반 안티에이징 제품으로 대체
 추천: "토리든 펩타이드 크림" (레티놀 프리, 노화 치료)
═══════════════════════════════════════════════
```

---

## 요약: RDS vs 그래프 비교

| 시나리오 | RDS 한계 | 그래프 이점 |
|----------|----------|-----------|
| 1. 성분 충돌 안전 경고 | 11-table JOIN, self-join | 단일 패턴 매칭, < 1초 |
| 2. 규제 연쇄 영향 | 5개 쿼리 수동 체이닝 | 1개 쿼리, 전체 체인 |
| 3. 시너지 루틴 추천 | 12-table JOIN, 사실상 불가 | 시너지 엣지 직접 탐색 |
| 4. 인플루언서 분석 | 재귀 CTE 5단계 한계 | 변수 길이 경로 + PageRank |
| 5. 이중 역할 파라독스 | 양방향 관계 동시 분석 불가 | TREATS + AGGRAVATES 동시 매칭 |

### 핵심 메시지

> **관계형 DB는 "데이터를 저장하기 위해" 설계되었다.**
> **그래프 DB는 "관계를 탐색하기 위해" 설계되었다.**
>
> 데이터의 가치는 개별 레코드가 아니라 **레코드 간의 관계**에 있다.
> 관계가 복잡해질수록 SQL JOIN은 기하급수적으로 느려지지만,
> 그래프 탐색은 **관계의 수에만 비례하여 일정한 성능**을 유지한다.
