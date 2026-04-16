# 다음 세션 작업 계획

> 작성일: 2026-04-16
> 기반: 현 세션 24 commits 자가 감사 결과
> 원칙: 하위호환 미고려, 근본 해결, 장기 유지보수성 우선

## 즉시 수정 필요한 결함 (P0)

### Bug #1 — OntologyVersion Display 이중 v
**위치**: `crates/ox-agent/src/lib.rs:328`
**현상**: `format!("... (v{}) ...", ontology.version)`에서 `OntologyVersion::Display`가 `v1` 출력 → 최종 `(vv1)` 이중 v.
**원인**: Phase 3.3에서 `version: u32` → `OntologyVersion` 교체 시 export 파일은 `version.number`로 수정했으나 ox-agent 사용처 누락.
**해결**: `ontology.version` → `ontology.version.number`. 전체 workspace `rg 'v\{\}.*version[^.]'`로 유사 케이스 재스캔.

### Bug #2 — ApiResponse dead code 6건
**위치**: `crates/ox-api/src/response.rs`
**현상**: `ApiResponse::of/page/ok`, `PageMeta` struct, `IntoResponse` impl이 정의됐으나 **실제 호출처 0건** → dead code warning 6건.
**원인**: Phase 1.3에서 구조체만 도입, 60+ 라우트 migration 미실행.
**해결**: 다음 세션의 Phase 1.3b 완주(아래).

### Bug #3 — `ApiResponse::ok()` 반환 타입 불일치 가능성
**위치**: `crates/ox-api/src/response.rs:94-100`
**현상**: `ApiResponse<serde_json::Value>` associated method — clippy가 "never constructed" 경고하는 이유.
**해결**: migration 완료 시 자동 해소.

---

## Phase 1.3b — ApiResponse<T> 라우트 일괄 migration (1일)

**범위**: 60+ 라우트 파일 전수 변환.

**변환 패턴** (response.rs의 migration guide 참조):
```
Before                                    After
Ok(Json(item))                           Ok(ApiResponse::of(item))
Ok(Json(page))  where CursorPage<T>      Ok(ApiResponse::page(page))
Ok(Json(json!({"status":"ok"})))         Ok(ApiResponse::ok())
Ok(StatusCode::NO_CONTENT)                unchanged
```

**Return type 시그니처도 변경**:
- `Result<Json<T>, AppError>` → `Result<Json<ApiResponse<T>>, AppError>`

**실행 전략**:
1. 파일별 처리 (ad-hoc sed 위험 — 함수 시그니처 중복 때문)
2. 라우트 그룹 단위 커밋 (dashboards, recipes, ontology 등)
3. 각 그룹 변환 후 빌드 검증
4. 커스텀 `*Response`, `*ListResponse` 래퍼 struct 삭제 (approvals.rs 등)

**검증**:
- OpenAPI 스키마 `openapi.json`의 모든 응답이 `{data, pagination?, meta?}` 형태 확인
- 프론트엔드 API 클라이언트 재생성 필요 (Phase 5에서 처리)

---

## Phase 2.1 — neo4j.rs 모듈 분해 (1.5일)

**현재**: 1299줄 단일 파일.

**목표 구조**:
```
crates/ox-runtime/src/neo4j/
  mod.rs             → re-exports + 공용 config (~80)
  runtime.rs         → GraphRuntime trait impl (~350)
  load.rs            → UNWIND 배치 + 재시도 (~250)
  isolation.rs       → 워크스페이스 predicate 주입 (~150)
  transience.rs      → Neo4jTransienceDetector (~80)
  schema.rs          → DDL 실행 (~120)
  search.rs          → search_nodes, expand_node, graph_overview (~150)
```

**순서**:
1. `mkdir neo4j/`, `mv neo4j.rs neo4j/mod.rs`
2. 각 기능군을 별도 파일로 이동 (struct/fn 단위)
3. `mod.rs`에서 `pub use` 재노출
4. 빌드 + 테스트 확인

**주의**:
- `Neo4jTransienceDetector`는 이미 `transience.rs`에 rule 있음 — 중복 제거, 단일 소스 유지
- test 모듈은 각 파일 하단에 co-locate

## Phase 2.2 — query_bindings.rs 분해 (1일)

**현재**: 1264줄. **분해**:
```
query_bindings/
  mod.rs             → 공용 타입 + re-exports (~100)
  scope.rs           → 변수 스코프 트리 (~400)
  evaluator.rs       → Expr 평가 (~500)
  validator.rs       → 참조 무결성 검증 (~300)
```

## Phase 2.3 — routes/ontology.rs 분해 (0.5일)

**현재**: 790줄. **분해**:
```
routes/ontology/
  mod.rs             → 라우터 등록
  crud.rs            → CRUD
  revisions.rs       → 버전·diff
  validation.rs      → 검증 헬퍼
```

---

## Phase 2.4 — GraphRuntime pre/post_execute 훅 (1일)

**현재 문제**: isolation/enrichment가 trait 외부에서 호출. 새 백엔드 추가 시 forgot risk.

**설계**:
```rust
#[async_trait]
pub trait GraphRuntime: Send + Sync {
    // 기존 메서드들 ...

    /// Query pre-processing (workspace isolation injection).
    fn pre_execute(&self, query: &str, _params: &QueryParams) -> OxResult<String> {
        Ok(query.to_string())
    }

    /// Query post-processing (enrichment, audit logging).
    async fn post_execute(
        &self,
        _query: &str,
        result: QueryResult,
    ) -> OxResult<QueryResult> {
        Ok(result)
    }
}
```

**변경**:
- `execute_query` 구현이 `pre_execute` → 실행 → `post_execute` 파이프라인 자동 적용
- Neo4jRuntime impl이 `pre_execute`에서 isolation predicate 주입
- `enrichment.rs`는 `post_execute`에서 호출
- 호출자(`tools/query_graph.rs`)는 raw `execute_query`만 호출 — 부가 로직 은닉

**검증**: isolation/enrichment가 trait 외부 호출 지점에서 사라져야 함.

---

## Phase 3.6 — PropertyTyper trait (0.5일)

**현재**: `ox-core::types::PropertyType::infer_from_db_type(&str) -> PropertyType` 전역 함수.

**문제**: Oracle `varchar2`, SQL Server `nvarchar`, 배열 `decimal[]` 등 방언별 override 불가.

**설계**:
```rust
// ox-core/src/types.rs
pub trait PropertyTyper: Send + Sync {
    fn map_type(&self, raw_db_type: &str) -> Option<PropertyType>;
    fn ambiguous_suggestions(&self, raw_db_type: &str) -> Vec<PropertyType> {
        vec![]
    }
}

pub struct DefaultTyper;
impl PropertyTyper for DefaultTyper { /* 현재 infer_from_db_type 로직 */ }
```

각 커넥터(ox-source)가 방언별 impl 제공 — `PostgresTyper`, `MySqlTyper`, `OracleTyper`, `SqlServerTyper`.

**호출 경로**: `DataSourceIntrospector::introspect_schema`에 `typer: &dyn PropertyTyper` 주입.

---

## Phase 4 — DB 연동 완성 (2일)

**전제**: `migrations/0005_phase4_governance.sql` 적용됨 (Docker 기동 시 자동).

### 4.2 api_keys Store + middleware
- `Store::create_api_key(label, workspace_id, created_by) -> (id, raw_key)`
- `Store::find_api_key_by_hash(hash) -> Option<ApiKey>`
- `Store::revoke_api_key(id)`
- `middleware.rs` API key 인증: blake3 hash 조회 → `Principal { id: "apikey:{label}" }` 설정
- 기존 "system:api-key" 분기 삭제

### 4.4 audit.affected_workspace_id
- `AuditEntry`에 `affected_workspace_id: Option<Uuid>` 필드 추가
- `main.rs`의 SYSTEM_BYPASS 유지보수 태스크가 영향받은 워크스페이스 ID를 감사에 기록
- 워크스페이스 관리자가 "내 데이터에 영향 준 시스템 작업" 조회 가능

### 4.8 prompt_templates.workspace_id
- `Store::get_prompt_template(name, workspace_id)` — ws-specific → global fallback
- `PromptRegistry::load(workspace_id)` — per-workspace 캐시
- 관리 API: 워크스페이스 관리자가 프롬프트 override 생성

### 4.10 dashboards.share_expires_at
- `Dashboard` 모델에 `share_expires_at: Option<DateTime<Utc>>` 추가
- 공유 생성 API에 `expires_in_days: u32` 필수 (기본 30)
- 만료된 토큰 접근 시 `410 Gone`

---

## Phase 4.1 — WebSocket 첫-메시지 인증 (0.5일)

**현재**: JWT가 쿼리 파라미터 `?token=...`로 전달 → 브라우저 히스토리 + 프록시 로그에 노출.

**설계**:
```rust
// ws.rs handler
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (sender, mut receiver) = socket.split();

    // 첫 메시지 5초 내 auth 필수
    let auth_msg = tokio::time::timeout(
        Duration::from_secs(5),
        receiver.next(),
    ).await;

    let principal = match auth_msg {
        Ok(Some(Ok(Message::Text(text)))) => {
            let payload: AuthMessage = serde_json::from_str(&text)?;
            verify_token(&payload.token)?
        }
        _ => return,  // close connection
    };

    // 이후 일반 메시지 처리
}

#[derive(Deserialize)]
struct AuthMessage {
    #[serde(rename = "type")]
    msg_type: String,  // "auth"
    token: String,
}
```

**프론트엔드 영향**: WS 연결 직후 `{"type": "auth", "token": "..."}` 전송 필요 — Phase 5와 조율.

---

## Phase 4.3 — MCP rate limit (1일)

**현재**: `mcp.rs`의 tool 호출에 제한 없음 → DOS 가능.

**설계**:
```rust
// OntosyxMcpServer에 추가
struct SessionLimiter {
    // session_id → (window_start, count)
    counters: DashMap<String, (Instant, u32)>,
}

impl SessionLimiter {
    fn check(&self, session_id: &str) -> Result<(), McpError> {
        let mut entry = self.counters.entry(session_id.into())
            .or_insert((Instant::now(), 0));
        let (start, count) = *entry;
        if start.elapsed() > Duration::from_secs(60) {
            *entry = (Instant::now(), 1);
            Ok(())
        } else if count < 100 {
            *entry = (start, count + 1);
            Ok(())
        } else {
            Err(McpError::rate_limited())
        }
    }
}
```

각 tool handler 진입 시 `check()` 호출. 초과 시 `McpError::RateLimited`.

또한 각 tool 호출에 timeout 강제 (config.timeouts.raw_query 기준).

---

## Phase 4.7 — Recovery detection 재설계 (1일)

**현재 문제**:
- session_id 기반만으로 페어링 → 세션 UUID 충돌 시 무관 쿼리 페어링
- 10분 window 내 failure→success 감지하되 쿼리 내용 비교 없음

**재설계**:
```rust
fn is_recovery_pair(failed: &ToolOutcome, succeeded: &ToolOutcome) -> bool {
    // 1. 동일 session + workspace + ontology
    if failed.session_id != succeeded.session_id { return false; }
    if failed.workspace_id != succeeded.workspace_id { return false; }
    if failed.ontology_id != succeeded.ontology_id { return false; }

    // 2. 10분 window
    let window = Duration::from_secs(600);
    if succeeded.timestamp - failed.timestamp > window { return false; }

    // 3. 구조적 유사도 — 쿼리 라벨 집합 Jaccard ≥ 0.5
    let labels_a = extract_labels(&failed.query);
    let labels_b = extract_labels(&succeeded.query);
    let intersection = labels_a.intersection(&labels_b).count();
    let union = labels_a.union(&labels_b).count();
    if union == 0 { return false; }
    let jaccard = intersection as f64 / union as f64;
    jaccard >= 0.5
}
```

**Dedup 강화**: `(session_id, workspace_id, query_hash)` 튜플로 중복 제거.

---

## Phase 5 — 프론트엔드 모던화 (3~4일)

### 5.1 TanStack Query 전면 도입
- `@tanstack/react-query` v5 설치
- 모든 `useEffect(fetch)` 패턴 → `useQuery`
- Mutation: `useMutation` + `queryClient.invalidateQueries` 일관 적용
- `src/hooks/api/use-ontology.ts` 등 도메인별 query 훅

### 5.2 URL 상태 동기화
- `src/hooks/use-query-state.ts` — `useSearchParams` 래퍼
- Explore: 검색어/포커스 노드/브레드크럼
- Analyze: query builder 상태
- Dashboard: 활성 필터/위젯 선택

### 5.3 App Router 기본 파일
- `web/src/app/error.tsx` — 루트 에러 바운더리
- `web/src/app/loading.tsx` — 스켈레톤
- `web/src/app/not-found.tsx` — 404

### 5.4 CJK 렌더링
- 시스템 폰트: `"Pretendard", "Noto Sans KR"` 명시
- `Intl.Collator('ko-KR')` 사용 (한글 정렬)
- IME composition 이벤트 처리 (`onCompositionEnd`로 검색 트리거)
- 긴 한글 라벨 ellipsis 처리

### 5.5 OpenAPI 타입 생성
- `utoipa` 빌드 스크립트 → `openapi.json`
- `openapi-typescript` → `src/types/api.generated.ts`
- CI drift 감지: `git diff --exit-code src/types/api.generated.ts`

### 5.6 a11y 감사
- axe-core 스크립트 통합
- 그래프 노드/엣지에 `role + aria-label`
- 다이얼로그 `focus-trap-react`
- 설정 테이블을 시맨틱 `<table>`로

### 5.7 Canvas Zustand slice 분리
- `useCanvasCommands`, `useCanvasKeyboard`, `useCanvasContextMenu`, `useCanvasSelection`
- `ontology-canvas.tsx` 577줄 → 250줄 목표

### 5.8 console.log 제거
- `src/lib/logger.ts` — 레벨 기반 로거
- 12개 잔존 `console.log` 치환
- ESLint rule: `no-console: ["error", { allow: ["warn", "error"] }]`

---

## Phase 6 — CI 게이트 + Korean E2E (2일)

### 6.1 백엔드 CI
```yaml
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
cargo udeps
```

### 6.2 Korean golden E2E 스크립트
`scripts/e2e-korean.sh`:
1. PostgreSQL + Neo4j 기동 (docker compose)
2. 한글 CSV 데이터 로드
3. API 통해 온톨로지 설계 자동 실행 → 생성 IR 검증
4. 한글 NL 쿼리 20종 실행 → 한글 라벨/컬럼 반환 확인
5. 상속/역관계/시간여행 기능별 시나리오 3개씩
6. Playwright UI 골든 스크린샷 비교

### 6.3 LLM 토큰 회귀 추적
- `bench/token_baseline.json` — 대표 쿼리 10종의 input/output 토큰 수
- CI에서 ±5% 허용, 초과 시 실패
- Phase 1.8에서 설정한 metrics 활용

### 6.4 프론트 컴포넌트 테스트
- vitest + @testing-library/react
- 위젯 10종, canvas primitives, dialog FocusTrap
- Playwright E2E 5종 (로그인, 온톨로지 설계, 한글 쿼리, 대시보드, 워크스페이스 전환)

### 6.5 성능 벤치마크 회귀
- `cargo bench` regression 3% 이상 시 CI 실패

---

## 우선순위 및 의존성

```
Bug #1 (OntologyVersion Display)  ← 5분, 즉시 수정
     │
Phase 1.3b (ApiResponse migration) ← 60+ 라우트, 1일
     │                                │
     │                                ↓
Phase 2.1~2.3 (모듈 분해, 3일)    Phase 4 DB 연동 (2일)
     │                                │
     ↓                                ↓
Phase 2.4 (GraphRuntime hooks)    Phase 4.1/4.3/4.7 (코드 보안, 2.5일)
     │                                │
     └──────────┬─────────────────────┘
                ↓
Phase 3.6 (PropertyTyper, 0.5일)
                │
                ↓
Phase 5 (프론트엔드, 3~4일)
                │
                ↓
Phase 6 (CI + E2E, 2일)
```

**총 예상 소요**: 14~17일 (1인 기준).

---

## 네이밍 일관성 잔여 이슈

이번 세션에서 해결되지 않은 네이밍 불일치:

1. **reconcile.rs의 `owner_id: String`**: PropertyOwner enum 도입 후 일부 helper 함수가 여전히 `owner_id: &str` 시그니처 유지 (타입 변환 boilerplate). 근본 해결 시 이 helper들도 `&PropertyOwner` 수용하도록 변경.

2. **Store trait의 `replace_*` vs `update_*`**: `replace_analysis_snapshot`, `complete_design_project`, `archive_stale_projects` 같은 lifecycle 메서드는 여전히 혼재. CLAUDE.md 규약("never use set_*")은 준수하나 `replace_` vs `update_` 통일 필요.

3. **`find_*` vs `get_*`**: Store trait에서 `find_*`는 거의 사용 안 되고 대부분 `get_*` + `Option` 반환. CLAUDE.md 정의("find_* — 조건부 검색, Option 반환")와 실제 사용 괴리.

4. **`Governance.owner_principal` vs `OntologyCommand::PropertyOwner`**: 용어 중첩. `owner`가 두 다른 개념(거버넌스 책임자 vs 프로퍼티 owning entity)을 지시.

---

## 논리적 결함 추가 조사 대상

다음 세션 초반에 verify할 것:

1. **Python spread 스크립트로 잘못 삽입된 곳**: 131곳 + 55곳 + 46곳 수동 검토 (Default::default() 위치 검증)
2. **serde tagged enum LLM 호환성**: `PropertyOwner`, `GraphFunction` 등이 branchforge 0.9 structured output에서 올바르게 생성/역직렬화되는지 실 LLM 호출 검증
3. **OntologyVersion JSON roundtrip**: 기존 `"version": 1` DB 레코드 → 새 custom deserializer → `OntologyVersion { number: 1, ... }` 왕복 테스트
4. **Phase 4 migration idempotency**: `ADD COLUMN IF NOT EXISTS` 지원되지 않는 Postgres 버전 호환성 재확인

---

## 세션 완료 정의

다음 세션이 "완료"로 간주되려면:

- [ ] Bug #1 수정
- [ ] Phase 1.3b ApiResponse migration 60+ 라우트 전환
- [ ] Phase 2.1~2.3 모듈 분해 (3353줄 → 10+ 파일)
- [ ] Phase 2.4 GraphRuntime hooks
- [ ] Phase 3.6 PropertyTyper trait
- [ ] Phase 4 Store trait + impl 연동 (migration 적용 후)
- [ ] Phase 4.1 WS 인증 재구조
- [ ] Phase 4.3 MCP rate limit
- [ ] Phase 4.7 Recovery detection Jaccard
- [ ] Phase 5 프론트엔드 7개 항목
- [ ] Phase 6 CI + Korean E2E

각 항목에 **acceptance criterion**:
- 빌드 통과 + 테스트 녹색
- clippy 0 violations
- 기존 테스트 회귀 없음
- 한글 fixture E2E 통과 (Phase 6 완료 후)

---

## 메모리에 저장될 컨텍스트

다음 세션이 이 파일을 `docs/NEXT_SESSION_PLAN.md`에서 읽어 시작점으로 사용.

`MEMORY.md`에 참조 추가:
- [docs/NEXT_SESSION_PLAN.md] — 전체 잔여 작업 및 발견 이슈 상세
