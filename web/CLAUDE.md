# web

Next.js 16 + React 19 + Tailwind v4 + Zustand 5.

## Dev

```bash
pnpm install
PORT=3100 pnpm dev
```

## Streamdown (Markdown Rendering)

Chat messages use `streamdown` with custom `components` prop (not CSS overrides).
Custom components in `components/chat/streamdown-components.tsx`:
- `pre`, `code` — block/inline code with dark-mode styling
- `table` — portal-based fullscreen via `createPortal`
- `a`, `blockquote`, `th`, `td`, `tr`, `thead`

Controls are disabled (`controls={false}`). Copy is handled by the message bubble's hover button.

Do NOT add CSS rules for `.prose-message table`, `.prose-message pre`, `.prose-message code` — streamdown components handle all styling.

## API Proxy

All API calls go through Next.js proxy routes in `app/api/proxy/`. Backend URL: `ONTOSYX_API_URL` env var (default: `http://localhost:3101/api`). Auth injected server-side.

## State Management

Zustand with slices in `lib/store/`. UI layout persisted, chat messages not persisted.

## Auth & Roles

`useAuth()` hook returns `{ isAdmin, canWrite, user }`. Use `isAdmin` to gate admin-only UI (e.g., Technical details in error cards, settings pages marked `adminOnly`).

## Settings Table Pattern

All settings tables use `py-3 pr-6` on `<th>` and `<td>` for consistent column spacing. Tables with 7+ columns need `min-w-[900px]` or higher to prevent header truncation.

## LocalizedText

Wire shape: `{ default: string; translations?: Record<string, string> }` — the canonical type lives in `@/types/ontology` and mirrors the Rust `ox_core::i18n::LocalizedText`. **Don't** declare ad-hoc inline shapes (`{default?: string; locales?: ...}` etc.) — they drift from the wire format and silently drop translations.

Read the displayable string through `localize()` / `localizePresent()` / `localizeWithFallback()` in `@/lib/locale/localize`. Direct `.default` access bypasses the locale chain. The static `DEFAULT_LOCALE_CHAIN = ["ko", "en"]` mirrors the `workspaces.locale_fallback` column default; surfaces with a workspace context should thread the actual chain in.

## Ontology cache state has two atomic entry points, no setters

`OntologySlice` no longer exposes `setOntology` / `loadOntology` / `resetOntology`. The two halves (`activeProject` + the local `ontology` cache) are kept in lockstep through:

- **`applyProjectSnapshot(project | null)`** — atomic project + cache update. Same-project refetches replay the unsaved `commandStack` on the new server snapshot so in-flight edits survive cache invalidation; project switches discard the stack. Pass `null` to leave project mode.
- **`loadStandaloneOntology(ir)`** — non-project mode (import / query-result viewer). Clears `activeProject` and replaces the cache atomically; the "project mode XOR standalone" invariant is enforced inside the action.

Every server-response handler that returned an `OntologyDraft` (handleSave, refine, restore, extend, reanalyze, complete, fork, delete) now lands its result through `applyProjectSnapshot`. Drift between `activeProject.ontology` and the slice cache is structurally impossible.

## Header actions: "Extend source" 1st-class

The design canvas top-bar carries an emerald "Extend source" button next to the inspector toggle. Clicking it fires `requestExtendSource()` (a monotonic counter on `ChromeSlice`) which `EnhanceActions` watches and auto-opens the extend sub-form on change. New header / shortcut callers should drive the same store action rather than prop-drilling — keeps the extension flow a one-click discovery without coupling.

## Master-detail over modal for entity CRUD

Vocabulary / list CRUD surfaces (CodeSystem, ValueSet, ConceptMap, NotationPattern, Rule, mappings, glossary) use **master-detail split**: list pane + always-visible editor pane (+ optional usage pane). Modals are reserved for destructive confirmations only. Industry pattern (Linear settings, Stripe Dashboard, Notion DB, Sanity Studio, Figma styles).

The `MasterDetailEntityPage` (`components/vocabulary/master-detail-entity-page.tsx`) is the canonical scaffold; `EntityWorkbench` (`components/workbench/entity-workbench.tsx`) is the lower-level shell that only handles pane layout. New CRUD surfaces drop into one of those — never introduce a modal-based create/edit flow.

URL state: `?id=<entityId>` round-trips selection. `?id=__new__` is the draft-create state. Deep-linking + back-button navigation behave naturally.

## Form section chrome is lightweight

Dense schema-driven entity editors (`StructuredForm` / `StructuredEntityEditor`) group fields with **fieldset + legend**, NOT card chrome. The `FormSection` primitive (`components/forms/form-section.tsx`) renders the standard `<fieldset>` with a regular-case `text-2xs font-medium text-foreground` legend.

`CollapsibleSection` (`components/ui/collapsible-section.tsx`) is the *card-chrome* variant — reserved for page-level tiles (settings dashboard panels, signals facet, stale facet) where the card itself is meaningful. Don't use it inside a master-detail editor.

## Plugin registries — `PluginRegistry<T>` only

New plugin / extensibility surfaces (command sources, inspector facets, future i18n catalogues / validators / edit-op contributions) build on the generic `PluginRegistry<T>` (`lib/plugins/registry.ts`):

```ts
const myRegistry = new PluginRegistry<MyItem>({ compare });
// React: usePlugin(myRegistry, item) handles unmount cleanup.
```

**Don't** re-implement module-singleton + listener Set + snapshot caching — the primitive owns those. The cached `list()` returns a referentially-stable array, which `useSyncExternalStore` requires.

Two registries currently retrofitted: `lib/command-registry.ts` (Cmd+K palette) and `components/workbench/inspector/facets/registry.tsx` (inspector tabs).

## Heading primitive — `<Heading level={N} size={M}>`

New headings use `<Heading level={N} size={M}>` (`components/ui/heading.tsx`). The primitive decouples document outline (level — h1…h6 tag) from visual tier (size — `--heading-{1..6}-size` tokens). Raw `<h2 className="text-...">` JSX leaks the design system into call sites and silently drifts; `pnpm heading-primitive-audit` (CI gate) blocks new violations against a baseline ratchet.

`size` cheat sheet: `display` (hero), `1` (page title), `2` (main section), `3` (subsection), `4` (`text-lg`), `5` (`text-base`), `6` (`text-sm`, dense section subheader).

## Save bar — sticky footer for entity editors

Master-detail editor panes (`StructuredEntityEditor`, `GlossaryForm`, `RuleForm`) close with `<SaveBar dirty={...} pending={...} onSave onDiscard />`. The bar slides in from the bottom only when there are unsaved changes — same pattern as Linear / Sanity / Notion. Don't render explicit submit/cancel buttons at the bottom of an editor; route through SaveBar so the dirty/save state stays consistent across surfaces.

Dirty calculation: `snapshotEqual(currentSnapshot, initialSnapshot)` (`@/lib/snapshot-equal`) where each snapshot is a `useMemo`'d object holding every editable slot. **Don't** use `JSON.stringify` — implementation-defined key order and `undefined`-vs-absent collapse both produce false dirty / false clean. The same snapshot drives `useDraftPersistence` auto-save.

## Multi-select + BulkActionBar — sticky-bottom action bar for cohort flows

List surfaces with row-level actions (knowledge base, stale-concept proposals, governance approvals, …) compose a multi-select cohort with row checkboxes + `<BulkActionBar />` (`components/ui/bulk-action-bar.tsx`). The bar slides in when `count > 0`, slides out otherwise, and disables every button while a mutation is pending. Industry pattern (Linear / Slack).

Standard call shape:

```tsx
<BulkActionBar
  count={selectedIds.size}
  countLabel={t("bulkSelectedCount", { count: selectedIds.size })}
  clearLabel={t("bulkClear")}
  ariaLabel={t("bulkBarLabel")}
  actions={[
    { key: "reject", label: t("bulkReject"), variant: "danger", onClick: ... },
    { key: "approve", label: t("bulkApprove"), variant: "primary", onClick: ... },
  ]}
  onClear={clearSelection}
  pending={mutation.isPending}
/>
```

The primitive takes pre-translated strings — i18n stays at the call site so each locale formats its own plural rules. **Don't** re-implement the inline `<div className="fixed inset-x-0 bottom-6 ...">` shape; that drifts (animation timing, pointer-events, z-index) and there are already three call sites to keep in sync. Add a fourth → `BulkActionBar`.

Selection state lives in a local `Set<string>`. Reset on filter / tab change so a leftover selection can never silently target ids from a different cohort. The header uses a tri-state checkbox: checked when every visible row is selected, indeterminate when some are.

## Optimistic mutations — `useOptimisticMutation` only

Every mutation that wants immediate visual feedback (status flip, row remove, bulk decision, …) goes through `useOptimisticMutation` (`hooks/api/use-optimistic-mutation.ts`). The hook codifies the `onMutate` / `onError` / `onSettled` triad — cancel in-flight refetches, snapshot the cache, apply the optimistic delta, roll back atomically on error, invalidate post-settle.

The bare TanStack `useMutation` admits four flavours of the same flow (no optimism, setQueryData without rollback, onMutate without invalidation, the full triad). Adopting one shape keeps every list-action behaviour consistent across surfaces.

```ts
useOptimisticMutation<Vars, Data>({
  mutationFn,
  queryKeys: [knowledgeKeys.list(filters)],
  optimisticUpdate: (prev, vars) => {
    if (!isExpectedShape(prev)) return prev; // runtime guard
    return /* next cache value */;
  },
});
```

Pair every list page that wants optimistic delete / status / bulk-review with the matching hook (`useDeleteKnowledge`, `useUpdateKnowledgeStatus`, `useBulkReviewKnowledge`, `useBulkReviewApprovals`, `useBulkDecideStaleProposals`, …). New BE bulk endpoints land alongside their FE optimistic hook in the same commit.

## Typed error model — `errors.<code>` i18n catalog

Backend error responses ride the typed wire shape `{ code, class, params }` (see `crates/ox-api/src/error.rs::ApiErrorCode`). The FE renders prose by reading the i18n catalog at `errors.<code>`:

```ts
toast.error(error.localize(t)); // t = useTranslations("errors")
```

Don't read `error.message` or interpolate `params.detail` into raw English — the catalog template owns the locale.

Adding a new code is a 4-side sync — see `feedback_typed_error_phased_migration` and the Rust enum's `every_variant_has_string_and_class` test for the procedure. `pnpm error-code-parity-audit` (CI gate) verifies the catalog matches the enum at every PR.

## i18n bundle keys must not contain `.`

next-intl parses `.` as a path separator, so a key like `codes.add` collides with the parent path lookup and crashes with `INVALID_KEY` at runtime. Either nest as `{ codes: { add } }` or flatten the segment (`codesAdd`). `pnpm i18n-dotted-key-audit` (CI gate) blocks dotted keys in either bundle.

## Unified command palette (⌘K)

The Cmd+K palette (`components/ui/command-palette.tsx`) reads from a single registry (`lib/command-registry.ts`). Surfaces contribute commands by registering a `CommandSource` while their subtree is mounted:

```tsx
function MySource() {
  const source = useMemo(() => ({ id: "my-surface", groupLabel, order, commands }), [...]);
  usePlugin(commandRegistry, source);
  return null;
}
```

The palette host (`components/layout/command-palette-host.tsx`) is mounted once at the root layout. Don't introduce a second palette — the canvas command-bar (Cmd+E) is a natural-language prompt input, semantically different from the discrete command palette.
