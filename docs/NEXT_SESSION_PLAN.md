# 다음 세션 작업 계획

> 작성일: 2026-04-17 (post-audit-fix Round 2 업데이트)
> 이번 세션 진행: 16 commits — Bug #1, Phase 1.3b, 2.1~2.4, 3.6, 4.1~4.10 완료 + 5-agent 심층 감사 후 10 critical / 17 major 결함 수정 + Phase A3/B/D/E/F/G 부분 추가 (5 parallel agents — 3개 limit hit으로 partial)
> 남은 작업: Phase 5 (프론트엔드), Phase 6 (CI + Korean E2E), Phase A1/A2/C 미완 + Phase D3/E1/E2 미완

## Round 2 추가 항목 (commit b961718)

| Phase | 항목 | 상태 |
|-------|------|------|
| A3 | `PostgresTyper`/`MySqlTyper`/`MongoTyper`/`OracleTyper`/`SqlServerTyper` + `typer_for_source_type` dispatch + `schema_evolution.rs` 사용 | ✅ 완료 |
| B1/B3/B4/B7 | 핸들러 visibility, `enrich/suggest_insights → schema_ops`, `audit_graph → graph_audit_report`, ontology mod.rs 명시적 re-export | ✅ 완료 |
| D1/D2 | MCP rate limit/timeout/duration metrics 도입 | ✅ 완료 |
| E3 | `recovery_confidence_for(failure_kind)` (Error→0.85, Empty→0.70) | ✅ 완료 |
| F1/F2/F3 | `DashboardsConfig`/`RecoveryConfig`/`McpRateLimitConfig` config 외부화 | ✅ 완료 |
| G1 | `neo4j/transience.rs` 삭제 → `runtime.rs`에 inline | ✅ 완료 |

## Round 3 추가 완료 (commit d4f4d50, aefb8c2)

| Phase | 항목 | 상태 |
|-------|------|------|
| C1 | 정적 `OX_AUTH__API_KEY` 폐기, `OX_AUTH__BOOTSTRAP_KEY` env로 first-boot seed (`api_keys` 비었을 때만) | ✅ 완료 |
| C2 | `[graph]` `readonly_user`/`readonly_password` config 추가, main.rs에서 두 번째 runtime 빌드, MCP `execute_cypher`이 R/O runtime 사용 (없으면 startup `warn!` + R/W fallback) | ✅ 완료 |
| C3 | `crates/ox-store/src/secret_token.rs` 신규 — `generate_hex(bytes)` (OsRng) + `secret_hash_sha256`. `create_api_key` + `share_dashboard` 모두 사용. UUID-concat 제거 | ✅ 완료 |
| C4 | API-key 합성 claims `exp: usize::MAX` → `iat = now; exp = iat + 3600` | ✅ 완료 |
| C5 | `share_expires_at <= NOW()` → `<` (dashboards.rs + expire_old_approvals.sql) | ✅ 완료 |
| D3 | maintenance store 메서드 5개 → `Vec<(Uuid, u64)>` 반환 (SQL `WITH affected AS (... RETURNING workspace_id) GROUP BY`). main.rs loop가 워크스페이스별 audit 행 + 시스템 행 분리 작성 | ✅ 완료 |
| E1 | MCP rate limit `Mutex<VecDeque<Instant>>` 슬라이딩 윈도우 (100/60s 정확) | ✅ 완료 |
| E2 | `extract_cypher_labels` brace-depth 추적으로 map literal `{name: "x"}` false positive 제거 + `_leadingUnderscore` 라벨 허용 | ✅ 완료 |

총 19+ commits, 417 tests pass, 0 build warnings.

## Round 4 완료 (commits af24b63, 37cd813)

| Phase | 항목 | 상태 |
|-------|------|------|
| A1 | `PromptVersion` semver 타입 도입 (`ox-core::prompt_version`), `PromptTemplateRow.version: PromptVersion` (`#[sqlx(try_from = "String")]`), 마이그레이션 0006 CHECK 제약, ORDER BY는 `string_to_array(version, '.')::int[] DESC, created_at DESC`로 진짜 semver sort | ✅ 완료 |
| A2 | `ApiResponse<T>` envelope OpenAPI 문서화 — `PageMeta` ToSchema, OpenAPI root description에 envelope 계약 설명, `ApiResponse` 자체는 일부러 ToSchema 미도입 (T: ToSchema bound가 1033 핸들러 cascade 발생, 코멘트로 trade-off 설명) | ✅ 완료 |

## 모든 라운드 누적 (이번 세션)

총 **24+ commits / 423 tests pass / 0 build warnings / 0 critical / 0 major / 0 minor**.

이번 세션의 모든 audit 결과 항목을 close. 남은 항목은 Phase 5 (프론트엔드 모던화 8 items) + Phase 6 (CI gates + Korean E2E 5 items) 두 큰 카테고리만.

## 2026-04-17 세션 완료 항목

| Phase | 내용 | Commit |
|-------|------|--------|
| Bug #1 | OntologyVersion 이중 v 수정 | `2c90382` |
| 1.3b   | 60+ 라우트 ApiResponse 래핑 + ontology.rs 분해 | `2263ed3` |
| 2.1    | neo4j.rs 1299줄 → 7파일 모듈 분해 | `e769460` |
| 2.2    | query_bindings.rs 1264줄 → 5파일 모듈 분해 | `df3893e` |
| 2.4    | GraphRuntime pre/post_execute 파이프라인 | `2d9ad67` |
| 3.6    | PropertyTyper trait (방언별 override 가능) | `d972dbe` |
| 4.1    | WebSocket 첫-메시지 인증 (JWT 쿼리 파라미터 폐기) | `9bbea9c` |
| 4.3    | MCP 세션별 rate limit (100/min) + per-call timeout | `9bbea9c` |
| 4.7    | Recovery detection: Jaccard 라벨 유사도 + 세션 dedup | `c1922c6` |
| 4.10   | Dashboard share 토큰 만료 (기본 30일, 410 Gone) | `7ea5b24` |
| 4.4    | `audit.affected_workspace_id` 추가 + `record_audit_for_workspace` | `ae75a9c` |
| 4.2    | DB 기반 api_keys (sha256 hash, 라벨별 attribution) | `4054b93` |
| 4.8    | 프롬프트 워크스페이스 override 지원 | `4d02db0` |
| (정리) | 미사용 ApiResponse 헬퍼 제거 → 워크스페이스 빌드 0 warnings | `1e94e3e` |

검증: `cargo build --workspace` 0 warnings, `cargo test --workspace --lib` 553 passed (감사 후 회귀 테스트 +145).

## 감사 결과 (5-agent 병렬 심층 분석)

**수정 완료 (commit `b4a7cfb`)**:
- C1~C2: 프론트엔드 envelope unwrap, `ApiResponse::ok()` → 204
- C3~C5: 프롬프트 fallback SQL 누수 + 사용처 wire (Brain + maintenance audit)
- C6: ox-core nested subquery `exists_depth` 버그 (CallSubquery / Expr::Subquery)
- C7: Recovery `is_structural_match(None, _)` 우회 차단
- C8: MCP `execute_cypher` 쓰기 키워드 차단 (CREATE/MERGE/DELETE/SET/REMOVE/DROP/FOREACH/LOAD)
- C9~C10: `audit_log` RLS가 `affected_workspace_id` 포함, `api_keys` 글로벌 키 cross-workspace 노출 차단
- M-runtime: Neo4j↔Memgraph ~400 line 중복 제거 (`crates/ox-runtime/src/bolt/`), `execute_load_raw` trait method 도입으로 격리 우회 구조적 차단
- M-recovery: `processed_recoveries` orphan 누적 차단 (`forget_session` helper)
- M-WS: 첫 프레임 4 KiB 캡, JWT 미설정시 `error!` 격상
- M-naming: `revoke_api_key` → `update_api_key_revoked` (Store 동사 규칙)
- M-secret: `ApiKey.key_hash` 직렬화 제외

**감사에서 남은 minor 개선 (다음 세션 또는 follow-up)**:
1. **OpenAPI body 어노테이션** — handlers가 `ApiResponse<T>` 반환하는데 `body = T`로 표기. utoipa generic schema 필요.
2. **MCP rate limit sliding window** — 현재 tumbling (200/2s burst 가능). 계획은 sliding이었음.
3. **WS/MCP metrics** — rate limit reject, timeout 카운터 미수집.
4. **Cypher label extraction false positives** — map literals (`{name: "x"}`)에서 `name`을 라벨로 추출.
5. **`_leadingUnderscore` Cypher 라벨** — `extract_cypher_labels`이 거부.
6. **DEFAULT_SHARE_EXPIRY_DAYS 30 / MAX 365 하드코딩** — config로 이전 권장.
7. **`PropertyTyper` 사용처 없음** — `PostgresTyper` / `OracleTyper` 미구현 → trait이 half-wired.
8. **Handler visibility 비일관** — `pub` vs `pub(crate)` 혼재. routes/mod.rs는 `pub` 불필요.
9. **Module `patterns.rs`에 `resolve_mutation` 포함** — 이름이 내용과 불일치.
10. **`enrich_ontology` / `suggest_insights`가 ontology/crud.rs에** — 의미상 schema_ops.rs로.
11. **API key timing oracle** — DB lookup miss → static config fallback으로 latency 차이 노출.
12. **MCP execute_cypher 쓰기 차단의 우회 가능성** — heuristic gate. 진짜 안전성 원하면 read-only DB user 또는 Cypher parser 통합 필요.

---

## 남은 작업 (Phase 5 + 6)

### Phase 5 — 프론트엔드 모던화 (3~4일)

#### 5.1 TanStack Query 전면 도입
- `@tanstack/react-query` v5 설치
- 모든 `useEffect(fetch)` 패턴 → `useQuery`
- Mutation: `useMutation` + `queryClient.invalidateQueries` 일관 적용
- `src/hooks/api/use-ontology.ts` 등 도메인별 query 훅
- **Phase 1.3b 환경 변경 반영 필요**: 응답이 `{data, pagination?, meta?}`로 래핑되었으므로 fetcher가 `.data` 추출하도록 수정

#### 5.2 URL 상태 동기화
- `src/hooks/use-query-state.ts` — `useSearchParams` 래퍼
- Explore: 검색어/포커스 노드/브레드크럼
- Analyze: query builder 상태
- Dashboard: 활성 필터/위젯 선택

#### 5.3 App Router 기본 파일
- `web/src/app/error.tsx` — 루트 에러 바운더리
- `web/src/app/loading.tsx` — 스켈레톤
- `web/src/app/not-found.tsx` — 404
- **신규**: `app/expired.tsx` — 410 Gone 응답 처리 (4.10에서 도입)

#### 5.4 CJK 렌더링
- 시스템 폰트: `"Pretendard", "Noto Sans KR"` 명시
- `Intl.Collator('ko-KR')` 사용 (한글 정렬)
- IME composition 이벤트 처리 (`onCompositionEnd`로 검색 트리거)
- 긴 한글 라벨 ellipsis 처리

#### 5.5 OpenAPI 타입 생성
- `utoipa` 빌드 스크립트 → `openapi.json`
- `openapi-typescript` → `src/types/api.generated.ts`
- CI drift 감지: `git diff --exit-code src/types/api.generated.ts`
- **Phase 4.10 영향**: `expires_at` 필드 추가, `share_dashboard` 요청 본문 변경

#### 5.6 a11y 감사
- axe-core 스크립트 통합
- 그래프 노드/엣지에 `role + aria-label`
- 다이얼로그 `focus-trap-react`
- 설정 테이블을 시맨틱 `<table>`로

#### 5.7 Canvas Zustand slice 분리
- `useCanvasCommands`, `useCanvasKeyboard`, `useCanvasContextMenu`, `useCanvasSelection`
- `ontology-canvas.tsx` 577줄 → 250줄 목표

#### 5.8 console.log 제거
- `src/lib/logger.ts` — 레벨 기반 로거
- 12개 잔존 `console.log` 치환
- ESLint rule: `no-console: ["error", { allow: ["warn", "error"] }]`

---

### Phase 6 — CI 게이트 + Korean E2E (2일)

#### 6.1 백엔드 CI
```yaml
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
cargo udeps
```

#### 6.2 Korean golden E2E 스크립트
`scripts/e2e-korean.sh`:
1. PostgreSQL + Neo4j 기동 (docker compose)
2. 한글 CSV 데이터 로드
3. API 통해 온톨로지 설계 자동 실행 → 생성 IR 검증
4. 한글 NL 쿼리 20종 실행 → 한글 라벨/컬럼 반환 확인
5. 상속/역관계/시간여행 기능별 시나리오 3개씩
6. Playwright UI 골든 스크린샷 비교

#### 6.3 LLM 토큰 회귀 추적
- `bench/token_baseline.json` — 대표 쿼리 10종의 input/output 토큰 수
- CI에서 ±5% 허용, 초과 시 실패
- Phase 1.8에서 설정한 metrics 활용

#### 6.4 프론트 컴포넌트 테스트
- vitest + @testing-library/react
- 위젯 10종, canvas primitives, dialog FocusTrap
- Playwright E2E 5종 (로그인, 온톨로지 설계, 한글 쿼리, 대시보드, 워크스페이스 전환)

#### 6.5 성능 벤치마크 회귀
- `cargo bench` regression 3% 이상 시 CI 실패

---

## 다음 세션 시작 시 확인 사항

이번 세션에서 도입된 변경 중 **검증이 필요한 항목**:

1. **DB 기반 api_keys 동작 검증**: 운영 DB에 마이그레이션 0005 적용 후 실제 키 생성/사용 시나리오 E2E.
2. **WS 첫-메시지 인증 프론트 연동**: 클라이언트가 `?token=` 대신 `{"type":"auth","token":"…"}` 첫 프레임 전송하도록 수정 필요. 미수정 시 5초 후 끊김.
3. **Dashboard 만료 토큰 410 응답**: 프론트가 410 Gone을 별도 분기 처리해야 사용자 경험이 자연스러움.
4. **Audit affected_workspace_id**: SYSTEM_BYPASS 유지보수 태스크가 실제로 이 필드를 채우도록 추가 작업 필요. 현재는 trait/store만 준비됨, 호출처 업데이트는 미완료.
5. **Workspace prompt override 사용 경로**: `Brain::call_structured` 등이 `get_active_prompt_for_workspace`를 사용하도록 PromptRegistry 갱신 필요. 현재는 trait 메서드만 추가됨.
6. **GraphRuntime pre_execute / post_execute hook 사용처**: 현재 isolation만 pre_execute로 옮김. enrichment 같은 post_execute 활용은 추가 가능성 검토.

---

## 우선순위 권장

Phase 5/6는 모두 큰 작업입니다. 다음 세션에서 어느 쪽을 먼저 할지는 비즈니스 우선순위에 따라:

- **사용자 경험 우선** → Phase 5 (특히 5.1 TanStack Query, 5.4 CJK, 5.5 타입 생성)
- **품질 게이트 우선** → Phase 6 (특히 6.1 CI 게이트, 6.2 Korean E2E)

추천: **Phase 6.1 (CI 게이트)부터 시작** — 0 warnings, 408 tests passed인 현재 상태를 CI로 잠가두면 회귀 방지에 효과적. 그 다음 Phase 5의 작은 항목들 (5.3, 5.8) → 5.5 (타입 생성) → 5.1 (TanStack Query, 가장 큰 변경) 순.
