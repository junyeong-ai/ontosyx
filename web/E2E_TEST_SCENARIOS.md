# Ontosyx E2E Test Scenarios

반복 가능한 전체 기능 검증 시나리오. `agent-browser` CLI 기반으로 자동화 가능.

---

## 사전 조건

```bash
# 1. Docker 시작 (PostgreSQL, Neo4j)
docker compose up -d

# 2. API 서버 시작
./run.sh start

# 3. Frontend 개발 서버 시작
cd web && npm run dev
```

---

## Golden Path: 설계 → 배포 → 분석 전체 흐름

> TC-01 ~ TC-03 → TC-08 → TC-09 → TC-13 순서로 실행하면 전체 lifecycle 검증 가능.

---

## 1. Design Mode — 프로젝트 생성 & 분석

### TC-01: 프로젝트 생성 (Create Project)

| Step | Action | Expected |
|------|--------|----------|
| 1 | Design 모드 전환 | "Start Designing" 빈 상태 카드 2개: "Create Project" / "Import Ontology" |
| 2 | "Create Project" 카드 클릭 | Workflow 패널 포커스, 하단 패널 열림 |
| 3 | Title 입력 | 폼 반영 |
| 4 | Source Type 선택 (PostgreSQL) | Connection String, Schema 필드 표시 |
| 5 | Connection String, Schema 입력 | Create Project 버튼 활성화 |
| 6 | "Create Project" 클릭 | "Creating..." 스피너 → 분석 완료 |
| 7 | 결과 확인 | table count, column count, PII/clarification 수, Source History 표시 |

### TC-02: Analysis Review

| Step | Action | Expected |
|------|--------|----------|
| 1 | 진행률 바 확인 | "0% resolved (0/N)" |
| 2 | "Unresolved only" 체크 확인 | 기본 ON |
| 3 | 테이블 필터 입력 | 결과 필터링 |
| 4 | "Auto-fill" 클릭 | PII + clarification 자동 처리, 토스트 |
| 5 | PII 결정 수동 변경 | allow/mask/exclude 드롭다운 동작 |
| 6 | Relationship 체크박스 토글 | 체크/해제 반영 |
| 7 | "Accept all" 그룹 버튼 | 해당 테이블 전체 확인 |
| 8 | Partial analysis acknowledge | 체크박스 체크 |
| 9 | "Save Decisions" 클릭 | 토스트 확인 |

### TC-03: Design Ontology (LLM)

| Step | Action | Expected |
|------|--------|----------|
| 1 | Large schema acknowledge (>100 tables) | 체크박스 표시, 체크 시 Design 활성화 |
| 2 | Domain Hints 입력 (선택) | 텍스트 반영 |
| 3 | "Design Ontology" 클릭 | 진행률 표시 (Validating → Designing → Quality → Persisting) |
| 4 | 완료 확인 | 캔버스에 노드/엣지 렌더링, Explorer 노드 목록 |
| 5 | 헤더 배지 확인 | "XN · YE" 카운트 |

---

## 2. Design Mode — 캔버스 & 편집

### TC-04: Canvas Interaction

| Step | Action | Expected |
|------|--------|----------|
| 1 | 노드 클릭 | Inspector에 노드 상세 (label, properties, constraints, relationships) |
| 2 | 엣지 클릭 | Inspector에 엣지 상세 (source → target, cardinality, properties) |
| 3 | 빈 영역 클릭 | 선택 해제, Inspector "Select a node or edge" |
| 4 | 줌 인/아웃 (스크롤) | 캔버스 확대/축소 |
| 5 | MiniMap 확인 | 우측 하단 미니맵 표시 |
| 6 | Perspective Switcher → "Save" | 현재 레이아웃 저장, 이름 변경 가능 |
| 7 | "Search..." 클릭 | 검색 다이얼로그 열림 (노드/엣지 검색) |
| 8 | Canvas "Export" 드롭다운 | Image(PNG/SVG) + Schema(JSON/Cypher/Mermaid/GraphQL/OWL/SHACL) |

### TC-05: ⌘K Command Bar — Edit Mode

| Step | Action | Expected |
|------|--------|----------|
| 1 | ⌘K 누름 | 커맨드바 오픈, Edit 모드 |
| 2 | "Describe changes..." placeholder | 입력 가능 |
| 3 | 편집 명령 입력 (예: "Add Payment node with amount, status, date") + Enter | "Generating edit commands..." 스피너 |
| 4 | Preview 확인 | 체크박스 리스트 (AddNode, AddProperty 등) + Apply/Cancel |
| 5 | 개별 체크박스 토글 | 선택/해제 반영 |
| 6 | "Select All" / "Deselect All" | 전체 토글 |
| 7 | "Apply" 클릭 | 온톨로지 변경, "Applied N commands" 토스트 |
| 8 | Escape 누름 | 커맨드바 닫힘 |

### TC-06: ⌘K Command Bar — Refine Mode

| Step | Action | Expected |
|------|--------|----------|
| 1 | ⌘K 누름 → "Refine" 모드 전환 | Refine 모드 활성 (그래프 프로파일링 기반) |
| 2 | 추가 컨텍스트 입력 (선택) | 텍스트 반영 |
| 3 | "Refine" 클릭 | SSE 스트리밍 (profiling → refining → reconcile → quality) |
| 4 | 완료 확인 | 온톨로지 업데이트, 캔버스 반영 |
| 5 | Reconcile Report 표시 (충돌 시) | 수락/거절 다이얼로그 |

### TC-07: Inspector — Node/Edge Editing

| Step | Action | Expected |
|------|--------|----------|
| 1 | 노드 선택 → Inspector 확인 | label, properties, description 표시 |
| 2 | 노드 라벨 인라인 편집 | 변경 반영 + 캔버스 업데이트 |
| 3 | "Add Property" 클릭 | 폼 오픈 (name, type, required, description) |
| 4 | 속성 추가 제출 | 속성 리스트에 추가됨 |
| 5 | 속성 삭제 (trash 아이콘) | 속성 제거 |
| 6 | 노드 삭제 | 노드 + 연결 엣지 제거, 선택 해제 |
| 7 | Undo (⌘Z) | 마지막 명령 되돌리기 |
| 8 | Redo (⇧⌘Z) | 되돌린 명령 재적용 |

### TC-08: Quality Report & AI Fix

| Step | Action | Expected |
|------|--------|----------|
| 1 | Quality 탭 클릭 | Quality Report 패널 표시 |
| 2 | Confidence 배지 확인 | high/medium/low 색상 표시 |
| 3 | Gap 목록 스크롤 | 심각도별 분류 (High/Medium/Low 카운트) |
| 4 | "Fix" 버튼 클릭 (quality gap) | AI 수정 제안 → CommandPreview |
| 5 | Apply 클릭 | 온톨로지 변경, quality 수치 개선 |
| 6 | AI Suggest Properties (Inspector) | 속성 제안 목록 |
| 7 | Accept/Reject 제안 | 수락 시 반영, 거절 시 제거 |

---

## 3. Design Mode — 프로젝트 완료 & 배포

### TC-09: Complete & Save + Schema Deploy

| Step | Action | Expected |
|------|--------|----------|
| 1 | Workflow 탭 → Finalize Project 섹션 | 이름 입력 + deploy 체크박스 + "Complete & Save" 버튼 |
| 2 | 이름 입력 | 반영 |
| 3 | "Deploy schema to Neo4j on complete" 체크 | 체크박스 ON |
| 4 | "Complete & Save" 클릭 | 프로젝트 완료 + 스키마 배포 |
| 5 | 토스트 확인 | "Project completed — ontology saved and schema deployed" (단일 토스트) |
| 6 | Deploy 체크 해제 후 Complete | "Project completed and ontology saved" (deploy 없음) |
| 7 | Quality gate 경고 시 | "Quality Gate Warning" 다이얼로그 → "Complete Anyway" |

### TC-10: Schema Deployment (Completed Project)

| Step | Action | Expected |
|------|--------|----------|
| 1 | Completed 프로젝트 로드 | "Ontology Saved" + "Schema Deployment" 섹션 표시 |
| 2 | "Preview DDL" 클릭 | DDL 문 미리보기 (CREATE CONSTRAINT/INDEX 목록) |
| 3 | DDL 내용 확인 | ontology의 constraint/index에 해당하는 DDL |
| 4 | "Execute" 클릭 | "Schema deployed: N statements executed" 토스트 |
| 5 | "Cancel" 클릭 | 미리보기 닫힘, 원래 버튼 복원 |
| 6 | "Deploy to Neo4j" 직접 클릭 | 확인 다이얼로그 표시 → "Deploy" 확인 → 배포 |
| 7 | Neo4j 검증 | `SHOW CONSTRAINTS` / `SHOW INDEXES` → 생성 확인 |

### TC-11: Data Loading (Completed Project with Source Mapping)

| Step | Action | Expected |
|------|--------|----------|
| 1 | Completed 프로젝트 (source_mapping 있는) 로드 | "Data Loading" 섹션 표시 |
| 2 | "Generate Load Plan" 클릭 | LLM 호출 스피너 |
| 3 | 결과 확인 | Load step 목록 (순서, 설명) |
| 4 | "Compile DDL" 클릭 | "Load plan compiled: N statements" 토스트 |
| 5 | "Cancel" 클릭 | Load plan 초기화 |
| 6 | source_mapping 없는 프로젝트 확인 | "Data Loading" 섹션 미표시 |

---

## 4. Design Mode — 확장 & 버전 관리

### TC-12: Extend with Source (다중 소스)

| Step | Action | Expected |
|------|--------|----------|
| 1 | Designed 프로젝트에서 "Extend with Source" 클릭 | 소스 입력 폼 표시 |
| 2 | Source Type 선택 (PostgreSQL/MySQL/MongoDB/CSV/JSON/Code Repo) | 폼 필드 변경 |
| 3 | Connection String 등 입력 후 "Extend" | 분석 + 온톨로지 병합 |
| 4 | 완료 확인 | 온톨로지 업데이트, Source History에 추가, Reconcile Report |
| 5 | Analysis Review 섹션 열림 | 새 소스의 PII/clarification 표시 |

### TC-13: Fork Project

| Step | Action | Expected |
|------|--------|----------|
| 1 | 헤더 프로젝트 셀렉터 열기 | 프로젝트 목록 |
| 2 | Completed 프로젝트 옆 "+" (Fork) 클릭 | "Project forked" 토스트 |
| 3 | 결과 확인 | 새 프로젝트 로드, status "designed", title에 "(fork)" 접미사 |
| 4 | 온톨로지 확인 | 원본 ontology 복제됨, 편집 가능 |

### TC-14: Revision History, Diff & Migration

| Step | Action | Expected |
|------|--------|----------|
| 1 | Workflow 탭 → Revision History 펼침 | 리비전 목록 (rev 1, rev 2 등, 노드/엣지 카운트) |
| 2 | "Diff" 클릭 → 다른 리비전 "Compare" | 추가/제거/수정 노드/엣지 diff 패널 |
| 3 | Diff 결과 → Canvas diff overlay 확인 | 추가(초록)/제거(빨강) 하이라이트 |
| 4 | "Restore" 클릭 | 확인 다이얼로그 → 이전 리비전 복원 |
| 5 | "Migrate" 클릭 | Migration Preview 패널 (UP DDL + Warnings + Breaking Changes) |
| 6 | Breaking Changes 있을 때 | "Execute Migration" 버튼 숨김 |
| 7 | Breaking Changes 없을 때 "Execute Migration" | "Migration executed: N statements" 토스트 |
| 8 | "Dismiss" 클릭 | Migration Preview 닫힘 |
| 9 | 프로젝트 전환 시 | migration/diff/revision 상태 모두 초기화 |

### TC-15: Reanalyze Source

| Step | Action | Expected |
|------|--------|----------|
| 1 | Advanced 섹션 펼침 → "Reanalyze Source" 클릭 | 소스 입력 폼 표시 |
| 2 | Connection String 입력 + "Reanalyze" | 재분석 실행 |
| 3 | 결과 확인 | 분석 데이터 갱신, 무효화된 결정 목록 토스트 |
| 4 | status 확인 | "analyzed"로 리셋 (ontology 초기화) |

---

## 5. Export / Import

### TC-16: Export Ontology

| Step | Action | Expected |
|------|--------|----------|
| 1 | 헤더 Export 아이콘 클릭 | 드롭다운 메뉴 (6개 포맷) |
| 2 | "JSON" 클릭 | .json 파일 다운로드, OntologyIR 구조 |
| 3 | "Cypher DDL" 클릭 | .cypher 파일 다운로드 |
| 4 | "Mermaid Diagram" 클릭 | .mmd 파일 다운로드 |
| 5 | "GraphQL Schema" 클릭 | .graphql 파일 다운로드 |
| 6 | "OWL/Turtle" 클릭 | .ttl 파일 다운로드 |
| 7 | "SHACL Shapes" 클릭 | .shacl 파일 다운로드 |

### TC-17: Import Ontology

| Step | Action | Expected |
|------|--------|----------|
| 1 | 헤더 Import 아이콘 클릭 | 파일 선택 대화상자 |
| 2 | JSON 파일 선택 | 온톨로지 로드, 캔버스 표시, "Ontology imported" 토스트 |
| 3 | OWL/Turtle 파일 선택 | 파싱 + 정규화 → 온톨로지 로드 |
| 4 | 빈 상태 "Import Ontology" 카드 클릭 | 파일 선택 대화상자 |

---

## 6. Analyze Mode

### TC-18: Analyze Mode — Auto-Load + AI Chat

| Step | Action | Expected |
|------|--------|----------|
| 1 | Analyze 모드 전환 (사이드바) | "Loading ontology..." → 최신 saved ontology 자동 로드 |
| 2 | 헤더 확인 | saved ontology 이름 + "XN · YE" 배지 (프로젝트 이름과 다를 수 있음) |
| 3 | Saved ontology 없는 상태 | "No saved ontology" 표시, 채팅 비활성 |
| 4 | Insight Suggestions 확인 | 5개 질문 제안 버튼 |
| 5 | 제안 클릭 | 자동 입력 + 전송 |
| 6 | 자연어 질문 입력 + Enter | SSE 스트리밍 (thinking → tool_start → tool_complete → text → complete) |
| 7 | Tool Call 카드 확인 | 도구명 (query_graph/execute_analysis), 실행 시간, 결과 미리보기 |
| 8 | Tool Call 확장 | raw JSON 출력 표시 |
| 9 | Results 탭 확인 | 자동 시각화 (bar/pie/line/stat/table/graph) |

### TC-19: Execution Mode (Auto/Supervised)

| Step | Action | Expected |
|------|--------|----------|
| 1 | Chat 입력 영역 하단 실행 모드 확인 | "auto" 기본값 |
| 2 | "supervised" 토글 클릭 | amber 하이라이트, 모드 변경 |
| 3 | Supervised 모드로 질문 전송 | tool_review 이벤트 → 사용자 승인 대기 |
| 4 | 도구 실행 승인/거절 | 승인 → 실행 진행, 거절 → 건너뜀 |

### TC-20: Raw Cypher Query (! prefix)

| Step | Action | Expected |
|------|--------|----------|
| 1 | "!" 입력 시작 | 하단에 "Raw Cypher mode" amber 라벨 |
| 2 | `!MATCH (n) RETURN labels(n)[0] AS label, count(n) AS cnt ORDER BY cnt DESC` 입력 + Enter | Neo4j 직접 실행 |
| 3 | 결과 확인 | tool_call_card (name: "raw_cypher"), 테이블 형태 결과 |

### TC-21: Query Panel (Raw Cypher Editor)

| Step | Action | Expected |
|------|--------|----------|
| 1 | Query 탭 클릭 | Cypher 에디터 표시 |
| 2 | Cypher 쿼리 입력 | 구문 반영 |
| 3 | "Execute" 클릭 | 결과 테이블 + 시각화 |

### TC-22: History Panel

| Step | Action | Expected |
|------|--------|----------|
| 1 | History 탭 클릭 | Recent/Pinned 탭 |
| 2 | Recent 리스트 확인 | 최근 실행 목록 (질문, 시간, 모델) |
| 3 | 실행 항목 클릭 | ExecutionDetail 패널 (쿼리, 결과, 설명, 위젯) |
| 4 | "Highlight" 클릭 | 캔버스에 바인딩 하이라이트 |
| 5 | "Replay" 클릭 | 스냅샷 온톨로지 로드 + 하이라이트 |
| 6 | "Load to chat" 클릭 | 채팅에 질문 복원 |
| 7 | "Pin" 클릭 | Pinned 탭에 추가 |
| 8 | Pinned 탭 → "Unpin" | 핀 제거 |

### TC-23: Session Management

| Step | Action | Expected |
|------|--------|----------|
| 1 | 세션 바 확인 | "SESSION New | N past | New" |
| 2 | 채팅 후 세션 ID 생성 확인 | 세션 셀렉터에 반영 |
| 3 | "New" 버튼 클릭 | 새 세션 시작, 이전 메시지 클리어 |
| 4 | 이전 세션 선택 | 메시지 복원 (role, content, tool_calls) |
| 5 | 세션 만료 시 | "session_expired" 이벤트 → 이전 대화 복원 + 새 세션 |

---

## 7. Explore Mode

### TC-24: Explore Mode — Auto-Load

| Step | Action | Expected |
|------|--------|----------|
| 1 | Explore 모드 전환 | "Loading ontology..." → 최신 saved ontology 자동 로드 |
| 2 | 헤더 확인 | ontology 이름 표시 |
| 3 | API 에러 시 | "Failed to load ontology" 표시 |
| 4 | 그래프 검색 기능 확인 | 검색 인터페이스 표시 |

---

## 8. Dashboard Mode

### TC-25: Dashboard — Widget CRUD

| Step | Action | Expected |
|------|--------|----------|
| 1 | Dashboard 모드 전환 | 대시보드 셀렉터 |
| 2 | "New Dashboard" 클릭 → 이름 입력 → Create | 대시보드 생성, 토스트 |
| 3 | "+ Add Widget" 클릭 | 위젯 추가 폼 (title, type, query) |
| 4 | 폼 제출 | 위젯 그리드에 추가, 배지 카운트 업데이트 |
| 5 | 위젯 클릭 | Widget Inspector 열림 |
| 6 | 위젯 설정 변경 → "Save Changes" | 업데이트 반영 |
| 7 | 위젯 삭제 | 그리드에서 제거 |
| 8 | 대시보드 삭제 | 대시보드 제거, 토스트 |

### TC-26: Dashboard AI Widget Generation

| Step | Action | Expected |
|------|--------|----------|
| 1 | AI 아이콘 클릭 | DashboardAiDialog 슬라이드오버 |
| 2 | 프롬프트 입력 (예: "Show top 5 categories by count") | 텍스트 반영 |
| 3 | Enter 또는 Send | SSE 스트리밍 → 위젯 프리뷰 카드 생성 |
| 4 | "Add to Dashboard" 클릭 | 위젯 추가, 그리드 반영 |
| 5 | 닫기 | 다이얼로그 닫힘 |

---

## 9. UI 인터랙션

### TC-27: Bottom Panel Toggle (VS Code Pattern)

| Step | Action | Expected |
|------|--------|----------|
| 1 | Workflow 탭 활성 상태에서 Workflow 탭 재클릭 | 하단 패널 접힘 (collapse) |
| 2 | Canvas 크기 확인 | Canvas가 전체 세로 영역 차지 (빈 공간 없음) |
| 3 | 접힌 상태에서 Chat 탭 클릭 | 하단 패널 열림 + Chat 탭 활성 |
| 4 | Chat 활성 상태에서 Chat 재클릭 | 하단 패널 접힘 |
| 5 | 화살표(↑/↓) 버튼 클릭 | 열림/접힘 토글 |

### TC-28: Canvas Empty State

| Step | Action | Expected |
|------|--------|----------|
| 1 | 프로젝트 없이 Design 모드 진입 | "Start Designing" 빈 상태 |
| 2 | "Create Project" 카드 클릭 | Workflow 탭 포커스 + 하단 패널 열림 |
| 3 | "Import Ontology" 카드 클릭 | 파일 선택 대화상자 |
| 4 | 프로젝트 선택 (ontology 있는) | 빈 상태 사라지고 Canvas 렌더링 |

### TC-29: Mode Context Isolation

| Step | Action | Expected |
|------|--------|----------|
| 1 | Design 모드에서 프로젝트 선택 | 프로젝트 ontology 로드 → Canvas 표시 |
| 2 | Analyze 모드 전환 | 최신 saved ontology 자동 로드 (Design 프로젝트와 다를 수 있음) |
| 3 | Design 모드 복귀 | activeProject의 ontology 복원 → Canvas 원래 상태 |
| 4 | Analyze Chat 전송 후 Network 탭 확인 | `saved_ontology_id` 전달, `project_id` 없음 |
| 5 | Design Chat 전송 후 Network 탭 확인 | `project_id` + `project_revision` 전달 |
| 6 | 새로고침 후 Analyze 모드 | `savedOntologyId` persist에서 복원, ontology 재로드 |
| 7 | 프로젝트 전환 시 | deployPreview, loadPlan, migrationResult 모두 초기화 |

### TC-30: Undo/Redo

| Step | Action | Expected |
|------|--------|----------|
| 1 | 노드 추가 (⌘K Edit) | commandStack +1 |
| 2 | ⌘Z 누름 | 추가 취소 |
| 3 | ⇧⌘Z 누름 | 재적용 |
| 4 | 다수 명령 후 ⌘Z 반복 | 순서대로 되돌리기 (max 50) |

### TC-31: Keyboard Shortcuts

| Step | Action | Expected |
|------|--------|----------|
| 1 | ⌘/ 누름 | 키보드 단축키 다이얼로그 표시 |
| 2 | ⌘K | Command Bar 오픈 |
| 3 | ⌘Z / ⇧⌘Z | Undo / Redo |
| 4 | ⌘[ / ⌘] | Explorer / Inspector 토글 |
| 5 | Escape | 선택 해제 / 다이얼로그 닫기 |

---

## 10. Settings & Admin

### TC-32: Settings Pages

| Step | Action | Expected |
|------|--------|----------|
| 1 | Settings 링크 클릭 (사이드바 하단) | 설정 페이지 이동 |
| 2 | System 설정 → 탭 목록 확인 | UI, LLM, Thresholds, Profiling, Timeouts, Lifecycle |
| 3 | 값 수정 → "Save Changes" | "Updated N settings" 토스트 |
| 4 | "Discard" 클릭 | 원래 값 복원 |
| 5 | Prompts 페이지 | 프롬프트 목록, 버전 표시, 편집 가능 |
| 6 | Recipes 페이지 | 레시피 CRUD (name, algorithm, code template) |
| 7 | Sessions 페이지 | 세션 목록, 이벤트 재생 |
| 8 | Reports 페이지 | 저장된 보고서 CRUD |

---

## 11. Error Handling & Edge Cases

### TC-33: Error Handling

| Step | Action | Expected |
|------|--------|----------|
| 1 | 미저장 편집 상태에서 프로젝트 전환 | "You have unsaved changes" 확인 다이얼로그 |
| 2 | 잘못된 Cypher 실행 | 에러 토스트 (에러 메시지 표시) |
| 3 | 네트워크 타임아웃 | "Request timed out" 토스트 |
| 4 | 0개 commands preview | "No structural changes needed" 메시지 |
| 5 | Graph DB 미연결 시 Deploy | "Graph database not connected" 503 에러 |
| 6 | Ontology 없는 프로젝트에서 Deploy | "Project has no ontology" 400 에러 |
| 7 | Deploy-on-complete 실패 시 | "Project completed but schema deploy failed: {reason}" warning |
| 8 | Migration breaking changes 시 | Execute 버튼 숨김, breaking changes 목록 표시 |
| 9 | 빈 diff (동일 리비전 비교) | "No schema changes between revisions" info 토스트 |

---

## 실행 방법

```bash
# agent-browser CLI로 자동 실행
agent-browser open "http://localhost:3000"
agent-browser snapshot -i          # 인터랙티브 요소 확인
agent-browser click @e1            # 요소 클릭
agent-browser fill @e3 "text"      # 텍스트 입력
agent-browser press Enter          # 키 입력
agent-browser screenshot /tmp/x.png # 스크린샷 캡처
```

### 핵심 검증 경로 (Golden Path)

```
TC-01 → TC-02 → TC-03 → TC-09 → TC-10 → TC-18
(생성)   (분석)   (설계)   (완료+배포) (로딩)  (AI 분석)
```

---

## 검증 결과 기록

| TC | 날짜 | 결과 | 비고 |
|----|------|------|------|
| TC-01 | | | |
| TC-02 | | | |
| TC-03 | | | |
| TC-04 | | | |
| TC-05 | | | |
| TC-06 | | | |
| TC-07 | | | |
| TC-08 | | | |
| TC-09 | | | |
| TC-10 | | | |
| TC-11 | | | |
| TC-12 | | | |
| TC-13 | | | |
| TC-14 | | | |
| TC-15 | | | |
| TC-16 | | | |
| TC-17 | | | |
| TC-18 | | | |
| TC-19 | | | |
| TC-20 | | | |
| TC-21 | | | |
| TC-22 | | | |
| TC-23 | | | |
| TC-24 | | | |
| TC-25 | | | |
| TC-26 | | | |
| TC-27 | | | |
| TC-28 | | | |
| TC-29 | | | |
| TC-30 | | | |
| TC-31 | | | |
| TC-32 | | | |
| TC-33 | | | |
