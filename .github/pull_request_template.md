## Summary

<!--
1~3 bullet 로 무엇이 변경됐는지. WHY 가 비자명하면 함께.
변경 list 가 아니라 의도. e.g.
- WorkbenchPageShell 의 chrome slot pattern 도입 — 탭 facet 이 page chrome row 에 액션을 portal 로 기여
- Operations 5 모드를 settings/* 에서 (workbench)/* 로 격상 — 사이드바 top-level Operations 그룹
-->

## Test plan

<!--
검증 단계. 가능한 한 명령으로 reproducible 하게.
- [ ] `pnpm gate` (16 audit + 939 vitest)
- [ ] `cargo test --workspace`
- [ ] 수동 확인: <surface / 흐름>
-->

- [ ] 

## Notes

<!--
- 검토자가 알아야 할 trade-off (예: 파일-레벨 분리 한계로 cross-cutting 변경이 일부 commit 에 mixed 되어 있음)
- 후속 작업 또는 follow-up
- Breaking change / migration 필요 여부
-->

---

<!--
* PR title: conventional commit (`feat(scope): …` / `fix(scope): …` / `refactor(scope): …` / `chore(scope): …`)
* CI: `summary` 게이트가 모두 green 이어야 merge 가능
* 라벨로 추가 검사 활성화: `e2e-korean`, `e2e-fe`, `lighthouse`, `bench`, `token-bench`
-->
