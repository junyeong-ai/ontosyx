# 다음 세션 작업 계획

> 작성일: 2026-04-17 (업데이트)
> 이전 세션 진행: 13 commits — Bug #1, Phase 1.3b, 2.1~2.4, 3.6, 4.1~4.10 완료
> 남은 작업: Phase 5 (프론트엔드 모던화), Phase 6 (CI + Korean E2E)

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

검증: `cargo build --workspace` 0 warnings, `cargo test --workspace --lib` 408 passed.

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
