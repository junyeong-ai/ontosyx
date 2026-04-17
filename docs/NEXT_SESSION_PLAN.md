# 다음 세션 작업 계획

> 작성일: 2026-04-17 (Phase 5+6 완료 후 최종)
> 이번 세션 총 commits: 25+, tests pass: 441 (Rust) + 118 (Vitest) + 22 (Playwright, labeled)
> 남은 critical/major/minor: **0** — 모든 감사 항목 close

## 이번 세션 누적 완료

모든 Phase 1~6 완료:

| Phase | 내용 | 상태 |
|-------|------|------|
| Bug #1 | OntologyVersion display 이중 v | ✅ |
| 1.3b | ApiResponse 145-handler migration | ✅ |
| 2.1~2.3 | neo4j / query_bindings / routes/ontology 모듈 분해 | ✅ |
| 2.4 | GraphRuntime pre/post_execute + execute_load_raw (격리 우회 구조적 차단) | ✅ |
| 3.6 | PropertyTyper trait + 5 connector impl (Postgres/MySql/Mongo/Oracle/SqlServer) | ✅ |
| 4.1 | WebSocket 첫-메시지 인증 + 4 KiB cap | ✅ |
| 4.2 | DB-backed api_keys (sha256 + `apikey:{label}` principal) | ✅ |
| 4.3 | MCP rate limit (sliding 100/60s) + per-call timeout + metrics | ✅ |
| 4.4 | audit.affected_workspace_id + record_audit_for_workspace wiring | ✅ |
| 4.7 | Recovery Jaccard + dedup + confidence-by-kind | ✅ |
| 4.8 | 프롬프트 워크스페이스 override (브레인 ws-aware 로딩) | ✅ |
| 4.10 | Dashboard share 만료 (30d default, 410 Gone 전용 UI) | ✅ |
| A1 | `PromptVersion` semver 타입 + 마이그레이션 0006 CHECK | ✅ |
| A2 | OpenAPI envelope 계약 문서화 + PageMeta ToSchema | ✅ |
| A3 | PropertyTyper 실제 사용처 wiring (schema_evolution) | ✅ |
| B1~B7 | 네이밍 일관성 (handler visibility, patterns/mutations 분리, `audit_graph → graph_audit_report`, explicit re-export) | ✅ |
| C1~C5 | 보안 강화 (static api_key 폐기, R/O graph runtime, CSPRNG share token, 1h claim TTL, `<` boundary) | ✅ |
| D1~D3 | MCP metrics + WS metrics + per-workspace maintenance audit | ✅ |
| E1~E3 | MCP sliding window + Cypher label parser (map-literal/underscore) + confidence-by-kind | ✅ |
| F1~F3 | Dashboards/Recovery/McpRateLimit config 외부화 | ✅ |
| G1 | `neo4j/transience.rs` 삭제 + runtime.rs inline | ✅ |
| **5.1** | **TanStack Query v5 전면 도입 + 4 components migration** | **✅** |
| **5.2** | **URL state hook (use-query-state) + 4 페이지 wiring** | **✅** |
| **5.3** | **App Router error/loading/not-found + shared-dashboard 410** | **✅** |
| **5.4** | **CJK — Pretendard/Noto Sans KR + Intl.Collator + IME-aware input** | **✅** |
| **5.5** | **OpenAPI codegen pipeline (dump_openapi bin + scripts + CI drift gate)** | **✅** |
| **5.6** | **a11y — axe-core + vitest-axe + focus-trap + 8 tests + 구조적 수정** | **✅** |
| **5.7** | **Canvas Zustand slice split (577→179 lines, 5 slices)** | **✅** |
| **5.8** | **Logger + ESLint no-console rule** | **✅** |
| **6.1** | **CI gates (fmt/clippy/test/deny/udeps/summary + frontend test)** | **✅** |
| **6.2** | **Korean golden E2E (scripts/e2e-korean.sh + fixture + label-gated CI)** | **✅** |
| **6.3** | **LLM token regression (baseline JSON + check script, placeholder)** | **✅** |
| **6.4** | **Playwright 5 specs (11 tests × 2 browsers) + vitest widget tests** | **✅** |
| **6.5** | **Criterion benches + bench-regression gate (placeholder)** | **✅** |

## 빌드 / 테스트 / 문서화 상태

- `cargo build --workspace` — clean, 0 warnings
- `cargo test --workspace --lib` — 441 pass, 3 `#[ignore]` (env-var section tests needing config helper rework)
- `cd web && pnpm lint` — 0 errors, 69 pre-existing warnings
- `cd web && pnpm test` — 16 files / 118 tests pass (a11y 8, widget 12 추가)
- `cd web && pnpm build` — succeeds (83 static pages + dynamic shared-dashboards)
- CI jobs: `lint, test, deny, udeps, frontend, docker, openapi-drift, security, e2e-korean, token-regression, frontend-e2e, bench-regression, summary` (13 jobs, label-gated for heavy ones)

## 다음 세션 우선순위 (placeholder 실측값 주입 + small followups)

1. **`bench/token_baseline.json` 실제 토큰 수 기록** (1 대표 쿼리당 1회 프로덕션 LLM 호출 → input/output tokens)
2. **`bench/baseline.json` CI 러너 기준 재측정** (현재는 로컬 macOS; GitHub Actions Ubuntu runner에서 재측정 후 `_meta.placeholder=false`)
3. **`udeps` report 트리아지** → `continue-on-error` 제거
4. **config-section env-var 테스트 재설계** — 3개 `#[ignore]` 해제
5. **pattern-canvas `div role=button`** 구조적 개선 (현재 key handler만 추가; 중첩 버튼 회피로 div 유지)
6. **Pretendard CDN preload** strict-CSP 환경에서 `style-src https://cdn.jsdelivr.net` 필요 여부 확인
7. **Shared dashboard 위젯 pass-through** — 백엔드 payload 확장 필요 (현재는 title+type 스텁만 표시)
8. **Share token revocation cache** — 현재 매 요청 DB lookup, short TTL (1h) claim. 향후 Redis 캐시 + invalidate 조합 검토

모두 blocker 아님. 코어 기능 + 인프라는 완성.
