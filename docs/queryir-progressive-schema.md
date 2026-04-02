# QueryIR Progressive Schema — Structured Output 복귀 전략

## Context

### 현재 문제

QueryIR JSON 스키마: **487 optional, 635 total properties** → Bedrock 한계(24/50) **20배 초과**.
결과: 모든 `translate_query` 호출이 JSON mode fallback → LLM이 추론 텍스트 출력 + 엣지 이름 할루시네이션.

```
로그: Schema too complex for structured output, using JSON mode
      optional_count=487, total_props=635
```

### 근본 원인

QueryIR 전체(8 QueryOp variants × 15 Expr variants × 5 Projection variants)를 **한 번에** LLM에 전달.
이는 nl2sql-poc에서 "전체 DDL을 한 번에 보내는 것"과 동일한 안티패턴.

### 해결 원칙: Progressive Disclosure for IR

nl2sql-poc의 GraphRAG처럼 **필요한 부분만 선택적으로 제공**:
- nl2sql-poc: 질문 → 관련 테이블 발견 → 해당 DDL만 제공
- **Ontosyx: 질문 → 필요한 QueryOp 판단 → 해당 IR 스키마만 제공**

---

## 설계

### Phase 1: MatchOnlyIR (95% 커버리지)

생산 쿼리의 ~95%가 `Match` 연산. `Match` 전용 축약 IR을 만들어 structured output 한계 내에 넣는다.

#### MatchOnlyIR 구조 (~30 properties, 한계 50 이내)

```rust
/// translate_query 전용 축약 IR.
/// Match 연산만 지원하며, structured output 한계(50 props) 내에서 동작.
/// 생성 후 QueryIR로 변환하여 기존 컴파일러와 호환.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MatchOnlyIR {
    pub patterns: Vec<SimplePattern>,
    pub filter: Option<SimpleExpr>,
    pub projections: Vec<SimpleProjection>,
    #[serde(default)]
    pub group_by: Vec<SimpleProjection>,
    pub order_by: Vec<SimpleOrderClause>,
    pub limit: Option<usize>,
    pub skip: Option<usize>,
}
```

#### SimpleExpr (6 variants, 현재 15에서 축소)

```rust
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "expr_type", rename_all = "snake_case")]
pub enum SimpleExpr {
    Literal { value: PropertyValue },
    Property { variable: String, field: Option<String> },
    Comparison { left: Box<SimpleExpr>, op: ComparisonOp, right: Box<SimpleExpr> },
    Logical { left: Box<SimpleExpr>, op: LogicalOp, right: Box<SimpleExpr> },
    In { expr: Box<SimpleExpr>, values: Vec<PropertyValue> },
    IsNull { expr: Box<SimpleExpr>, negated: bool },
    // StringOp는 Comparison으로 표현 가능 (contains → string_op)
    StringOp { left: Box<SimpleExpr>, op: StringOp, right: Box<SimpleExpr> },
}
```

7 variants (StringOp 포함) — 나머지 8개(Not, FunctionCall, Exists, Case, Subquery 등) 제거.

#### SimpleProjection (3 variants, 현재 5에서 축소)

```rust
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SimpleProjection {
    Field { variable: String, field: String, alias: Option<String> },
    Variable { variable: String, alias: Option<String> },
    Aggregation {
        function: AggFunction,
        argument: Box<SimpleProjection>,
        distinct: bool,
        alias: String,
    },
}
```

#### SimplePattern (2 variants, 현재 4에서 축소)

```rust
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SimplePattern {
    Node {
        variable: String,
        label: String,
        property_filters: Vec<PropertyFilter>,
    },
    Relationship {
        variable: Option<String>,
        label: String,
        source: String,
        target: String,
        direction: Direction,
        property_filters: Vec<PropertyFilter>,
        var_length: Option<VarLength>,
    },
}
```

### Phase 2: MatchOnlyIR → QueryIR 변환

LLM이 `MatchOnlyIR` 생성 → Rust에서 `QueryIR`로 trivial 변환:

```rust
impl From<MatchOnlyIR> for QueryIR {
    fn from(m: MatchOnlyIR) -> Self {
        QueryIR {
            operation: QueryOp::Match {
                patterns: m.patterns.into_iter().map(Into::into).collect(),
                filter: m.filter.map(Into::into),
                projections: m.projections.into_iter().map(Into::into).collect(),
                optional: false,
                group_by: m.group_by.into_iter().map(Into::into).collect(),
            },
            limit: m.limit,
            skip: m.skip,
            order_by: m.order_by.into_iter().map(Into::into).collect(),
        }
    }
}
```

`SimpleExpr` → `Expr`, `SimpleProjection` → `Projection` 등도 동일한 `From` impl.

### Phase 3: translate_query 통합

```rust
// ox-brain/src/lib.rs — translate_query

// Step 1: MatchOnlyIR로 structured completion 시도
let result: OxResult<MatchOnlyIR> = self.call_structured(
    "translate_query", Some("3.0.0"), "translate_query", &vars, "..."
).await;

match result {
    Ok(match_ir) => {
        // MatchOnlyIR → QueryIR 변환
        let query_ir: QueryIR = match_ir.into();
        // 기존 validate_query_labels 등 후처리
        Ok(query_ir)
    }
    Err(_) => {
        // Fallback: 풀 QueryIR로 JSON mode 시도 (advanced queries)
        self.call_structured::<QueryIR>(
            "translate_query", Some("2.0.0"), "translate_query", &vars, "..."
        ).await
    }
}
```

### Phase 4: 프롬프트 업데이트

`translate_query.toml` v3.0.0: MatchOnlyIR 스키마에 맞는 예제와 규칙.

현재 v2.0.0의 예제들은 이미 모두 Match 연산 — 변경 최소.

---

## 예상 효과

| 지표 | 현재 (JSON mode) | 개선 후 (Structured) |
|------|-----------------|---------------------|
| Optional params | 487 | **~15** |
| Total properties | 635 | **~30** |
| Structured output | 불가 (항상 fallback) | **가능 (95% 쿼리)** |
| 엣지 이름 정확도 | 낮음 (할루시네이션) | **높음 (스키마 강제)** |
| 파싱 실패율 | 높음 (추론 텍스트) | **0% (스키마 검증)** |
| 토큰 비용 | 높음 (prompt cache miss) | **낮음 (cache hit)** |
| 응답 시간 | ~10s (retry 포함) | **~3-5s** |

---

## 변경 파일

| File | Action |
|------|--------|
| `crates/ox-core/src/match_only_ir.rs` | **신규** — MatchOnlyIR, SimpleExpr, SimpleProjection, SimplePattern |
| `crates/ox-core/src/match_only_ir/convert.rs` | **신규** — MatchOnlyIR → QueryIR 변환 (From impls) |
| `crates/ox-core/src/lib.rs` | pub mod match_only_ir 등록 |
| `crates/ox-brain/src/lib.rs` | translate_query에서 MatchOnlyIR 우선 사용 |
| `prompts/translate_query.toml` | v3.0.0: MatchOnlyIR 스키마 기반 예제 |
| `crates/ox-brain/src/provider.rs` | structured output 성공 경로 최적화 |

---

## 검증

1. `schemars::schema_for!(MatchOnlyIR)` → JSON 스키마 생성 → 속성 수 확인 (<50)
2. `translate_query` 호출 → structured output 모드로 동작 확인 (JSON mode fallback 없음)
3. EU 레티놀 규제 질문 → 1-2개 쿼리로 정확한 결과 (엣지 이름 할루시네이션 없음)
4. 기존 모든 쿼리 예제가 MatchOnlyIR로 표현 가능한지 확인
5. 5가지 graph-insight-scenarios.md 시나리오 테스트

---

## nl2sql-poc 패턴 매핑

| nl2sql-poc | Ontosyx QueryIR |
|------------|----------------|
| 전체 DDL → 선택된 테이블 DDL | 전체 QueryIR 스키마 → **MatchOnlyIR 스키마** |
| business terms (0.9) | 질문에서 Match 연산 감지 (기본, 95%) |
| vector similarity | PathFind/Aggregate 감지 (키워드 기반) |
| graph expansion (FK) | 필요 시 Chain/Union으로 확장 |
| build_partial_ddl() | **MatchOnlyIR 스키마만 structured output에 전달** |
| filter_sample_values() | translate_query 프롬프트 예제 |
