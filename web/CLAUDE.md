# web

Next.js 16 + React 19 + Tailwind v4 + Zustand 5.

## Dev

```bash
pnpm install
PORT=3100 pnpm dev
```

## API proxy

All API calls go through Next.js BFF routes in `app/api/proxy/`. Backend URL: `ONTOSYX_API_URL` (default `http://localhost:3101/api`). Auth is injected server-side; client code never sees the API key.

## Auth & roles

`useAuth()` returns `{ isAdmin, canWrite, user }`. Use `isAdmin` to gate admin-only UI (technical details in error cards, settings pages flagged `adminOnly`).

## Typed error model — `errors.<code>` i18n catalogue

Backend errors ride the typed wire shape `{ code, class, params }` (`crates/ox-api/src/error.rs::ApiErrorCode`). Render prose through the catalogue:

```ts
toast.error(error.localize(t)); // t = useTranslations("errors")
```

Don't read `error.message` or interpolate `params.detail` into raw English — the catalogue template owns the locale. Adding a new code is a four-side sync gated by `pnpm error-code-parity-audit`.

## i18n bundle keys must not contain `.`

next-intl parses `.` as a path separator, so a key like `codes.add` collides with the parent path lookup and crashes with `INVALID_KEY` at runtime. Either nest as `{ codes: { add } }` or flatten the segment (`codesAdd`). `pnpm i18n-dotted-key-audit` blocks dotted keys in either bundle.

## LocalizedText

Wire shape: `{ default: string; translations?: Record<string, string> }` — canonical type in `@/types/ontology`, mirrors the Rust `ox_core::i18n::LocalizedText`. Don't declare ad-hoc inline shapes (`{default?: string; locales?: ...}`) — they drift and silently drop translations.

Read displayable strings through `localize()` / `localizePresent()` / `localizeWithFallback()` (`@/lib/locale/localize`). Direct `.default` access bypasses the locale chain. `DEFAULT_LOCALE_CHAIN = ["ko", "en"]` mirrors the `workspaces.locale_fallback` column default; surfaces with workspace context should thread the actual chain in.

## Ontology cache state — two atomic entry points, no setters

`OntologySlice` does not expose `setOntology` / `loadOntology` / `resetOntology`. The two halves (`activeProject` + the local `ontology` cache) stay in lockstep through:

- `applyProjectSnapshot(project | null)` — atomic project + cache update. Same-project refetches replay the unsaved `commandStack` on the new server snapshot so in-flight edits survive cache invalidation; project switches discard the stack. Pass `null` to leave project mode.
- `loadStandaloneOntology(ir)` — non-project mode (import / query-result viewer). Clears `activeProject` and replaces the cache atomically.

Every server-response handler that returns an `OntologyDraft` (handleSave / refine / restore / extend / reanalyze / complete / fork / delete) lands its result through `applyProjectSnapshot`. Drift between `activeProject.ontology` and the slice cache is structurally impossible.

## Master-detail over modal for entity CRUD

Vocabulary / list CRUD surfaces (CodeSystem, ValueSet, ConceptMap, NotationPattern, Rule, mappings, glossary) use **master-detail split**: list pane + always-visible editor (+ optional usage pane). Modals are reserved for destructive confirmations only. `MasterDetailEntityPage` (`components/vocabulary/master-detail-entity-page.tsx`) is the canonical scaffold; `EntityWorkbench` (`components/workbench/entity-workbench.tsx`) is the lower-level pane shell. New CRUD surfaces drop into one of those — never introduce a modal-based create/edit flow. URL state: `?id=<entityId>` round-trips selection; `?id=__new__` is the draft-create state.

## Form section chrome is lightweight

Dense schema-driven editors (`StructuredForm` / `StructuredEntityEditor`) group fields with **fieldset + legend**, NOT card chrome. Use the `FormSection` primitive (`components/forms/form-section.tsx`). `CollapsibleSection` is the *card-chrome* variant — reserved for page-level tiles where the card itself is meaningful. Don't use it inside a master-detail editor.

## Save bar — sticky footer for entity editors

Master-detail editor panes close with `<SaveBar dirty={...} pending={...} onSave onDiscard />`. The bar slides in only when there are unsaved changes. Don't render explicit submit/cancel at the bottom of an editor — route through SaveBar so dirty/save state stays consistent across surfaces.

Dirty calculation: `snapshotEqual(currentSnapshot, initialSnapshot)` (`@/lib/snapshot-equal`). Don't use `JSON.stringify` — implementation-defined key order and `undefined`-vs-absent collapse both produce false dirty / false clean. The same snapshot drives `useDraftPersistence` auto-save.

## Multi-select + `BulkActionBar`

List surfaces with row-level actions (knowledge base, stale-concept proposals, governance approvals, …) use `<BulkActionBar />` (`components/ui/bulk-action-bar.tsx`). The bar slides in when `count > 0`, slides out otherwise, and disables every button while a mutation is pending. Pass pre-translated strings — i18n stays at the call site so each locale formats its own plural rules.

Selection state is a local `Set<string>`. Reset on filter / tab change so a leftover selection can never silently target ids from a different cohort. The header uses a tri-state checkbox (checked when every visible row is selected, indeterminate when some are).

## Optimistic mutations — `useOptimisticMutation`

Every mutation that wants immediate visual feedback (status flip, row remove, bulk decision) uses `useOptimisticMutation` (`hooks/api/use-optimistic-mutation.ts`). The hook codifies the `onMutate` / `onError` / `onSettled` triad — cancel in-flight refetches, snapshot, apply optimistic delta, atomic rollback on error, invalidate on settle. The bare TanStack `useMutation` admits four flavours of the same flow; adopting one shape keeps every list-action behaviour consistent.

## Plugin registries — `PluginRegistry<T>`

New plugin / extensibility surfaces (command sources, inspector facets, future i18n catalogues / validators / edit-op contributions) build on `PluginRegistry<T>` (`lib/plugins/registry.ts`):

```ts
const myRegistry = new PluginRegistry<MyItem>({ compare });
// React: usePlugin(myRegistry, item) handles unmount cleanup.
```

Don't re-implement module-singleton + listener Set + snapshot caching — the primitive owns those. The cached `list()` returns a referentially-stable array, which `useSyncExternalStore` requires.

## Heading primitive

New headings use `<Heading level={N} size={M}>` (`components/ui/heading.tsx`). The primitive decouples document outline (level — h1…h6 tag) from visual tier (size — `--heading-{1..6}-size` tokens). Raw `<h2 className="text-...">` JSX leaks the design system into call sites; `pnpm heading-primitive-audit` blocks new violations against a baseline.

`size` cheat sheet: `display` (hero) · `1` (page title) · `2` (main section) · `3` (subsection) · `4` (`text-lg`) · `5` (`text-base`) · `6` (`text-sm`, dense subheader).

## Unified command palette (⌘K)

The Cmd+K palette (`components/ui/command-palette.tsx`) reads from one registry (`lib/command-registry.ts`). Surfaces contribute by registering a `CommandSource` while their subtree is mounted (`usePlugin(commandRegistry, source)`). The host is mounted once at the root layout — don't introduce a second palette. The canvas command-bar (Cmd+E) is a natural-language prompt input, semantically different from the discrete palette.

## Streamdown (markdown rendering)

Chat messages use `streamdown` with the custom `components` prop in `components/chat/streamdown-components.tsx` (`pre`, `code`, `table` portal-fullscreen, `a`, `blockquote`, `th`, `td`, `tr`, `thead`). Controls are disabled (`controls={false}`); copy is handled by the message bubble's hover button. Don't add CSS rules for `.prose-message table` / `.prose-message pre` / `.prose-message code` — the components handle styling.
