# Ontosyx Design System Master Plan (v3, clean-slate)

Last updated: 2026-05-02
Status: **Final blueprint — designed as if from scratch. No backward compatibility, no migration shims, no deprecated exports kept "for safety". Every legacy is removed in the same PR that introduces the replacement.**

This document supersedes v1 and v2 of the same name. v3 differs by:

- Treating the system as **greenfield**: every rename, deletion, and consolidation is a hard cutover.
- Including a **deletion list** alongside the addition list — every phase includes what to remove.
- Auditing not just visual design but **logical flaws, file/folder organization, naming consistency, dead code**.
- Naming conventions specified at every layer: components, hooks, types, props, files, folders, tokens.

---

## 0. Evidence-grounded findings (audit results)

All claims trace to grep-counted facts or screenshots under `/tmp/ontosyx-audit-v2/`.

### 0.1 Visual / token findings (from v2 audit, unchanged)

- `text-xs` (12px) used **719×** as default vs `text-sm` (14px) only **234×** — body text below industry baseline.
- `text-[10px]` used **513×** — too tiny for non-status-pill content.
- Bare `rounded` (4px) used **397×** vs `rounded-md` 231 / `rounded-lg` 172 — defaults ad-hoc.
- `<EmptyState>` used 33×, but **14+ pages** dump plain `<p>{t("empty")}</p>` instead.
- `motion/react` used only **16×** (chat-panel + bottom-panel) — page transitions, dialog choreography, list staggering missing.
- 4 violet UI-affordance tabs (this session: fixed in vocabulary, glossary, quality/stale, ambiguity).
- `text-emerald-600` foreground on white (3.6:1) failed WCAG AA — fixed this session.

### 0.2 Logical / structural findings (NEW — added in v3)

#### 0.2.1 Component duplication

| Symptom | Count | Source primitive | Action |
|---|---|---|---|
| Re-implementations of `StatusBadge` | **6 in feature code** + 1 ui primitive | `<StatusBadge>` at `components/ui/status-badge.tsx` exists | **Delete all 6 copies**, migrate to primitive (with extended `colorMap`). |
| Bespoke tab strips (hardcoded `<button role="tab">` + `<span absolute -bottom-px>` indicator) | **7 files** | `<TabBar>` exists, used only **4×** | **Delete bespoke**, all routes through `<TabBar>` or `<WorkbenchPageShell>` tabs. |
| Hand-rolled card containers (`rounded-(lg/xl) border border-zinc-200 bg-(white/zinc-50)`) | **96 occurrences** | No primitive | **New** `<Card>` primitive (variant: surface/raised/inset). |
| Plain-text empty states (no icon, no description) | **14 pages** | `<EmptyState>` exists | **Delete plain text**, all empty states route through `<EmptyState>`. |
| Raw `<button>` markup with bespoke styling | **131 files** vs 32 using `<Button>` | `<Button>` exists with 6 variants | **Audit + migrate**: any clickable interactive element is `<Button>` or `<IconButton>`. |
| Bespoke dialog patterns | **10+ files** with `Dialog.Root` / `fixed inset-0 ... bg-black/40` | Base UI `Dialog` + `<ConfirmDialog>` / `<PromptDialog>` exist | **New** `<Modal>` primitive that wraps base-ui Dialog with our backdrop + animation choreography. |

#### 0.2.2 Naming inconsistencies

**File-vs-component mismatches** (filename ≠ default exported component):

| File | Currently exports | Should export |
|---|---|---|
| `confirm-dialog.tsx` | `ConfirmProvider` + `ConfirmDialog` (internal) + `useConfirm` hook | `ConfirmDialog` should be the default export; provider is a side-effect of `useConfirm`. |
| `prompt-dialog.tsx` | `PromptProvider` + `usePrompt` hook | Same pattern fix. |
| `keyboard-shortcuts.tsx` | `KeyboardShortcutsDialog` | **Rename file** to `keyboard-shortcuts-dialog.tsx`. |
| `binding-panel.tsx` | `GlossaryBindingPanel` | **Rename file** to `glossary-binding-panel.tsx`. |
| `widget-toolbar.tsx` | `WidgetWithToolbar` | **Rename file** to `widget-with-toolbar.tsx` OR rename component to `WidgetToolbar`. |
| `graph-legend.tsx` | `Legend` | **Rename component** to `GraphLegend`. |
| `graph-detail-panel.tsx` | `NodeDetailPanel` | **Rename component** to `GraphDetailPanel`. |
| `entity-detail.tsx` | `NodeDetail` | **Rename component** to `EntityDetail`. |
| `property-editor.tsx` | `AddPropertyForm` | **Rename file** to `add-property-form.tsx`. |
| `node-group.tsx` | `GroupNode` | **Rename component** to `NodeGroup`. |
| `quality-gaps.tsx` | `GapsList` | **Rename component** to `QualityGapsList`. |
| `workflow-indicators.tsx` | `ProgressIndicator` | **Rename file** OR component to align. |
| `review-toc.tsx` | `ReviewTOC` (acronym) | **Rename component** to `ReviewToc` (PascalCase consistency). |
| `TableSelector.tsx` | `TableSelector` (PascalCase filename!) | **Rename file** to `table-selector.tsx`. |

**Provider/hook naming**:
- `<ConfirmProvider>` lives in a file called `confirm-dialog.tsx` — confusing. Provider files should be named after their function (`confirm-provider.tsx` if extracted) or the dialog file should not export the provider.

**Boolean prop naming** (audit count):
- `open=` (27), `visible:` (15), `open:` (13), `visible=` (8), `isOpen?` (4) — five different conventions for "is this thing visible".
- **Standardize**: `open` for boolean state on dialogs/popovers/drawers. `visible` only for layout visibility (CSS display:none equivalent). Never `isOpen`.

**Button variant inconsistency**:
- `<Button variant="danger">` used in prompts page (1 site).
- `<Alert variant="error">` used in chat panels (2 sites).
- `Button.variants` enum contains both `error` AND `danger`.
- **Standardize**: `danger` everywhere (action-tone semantics). `error` is for state, not affordance.

**Store selector naming** (consistent — keep):
- All selectors named `selectStateX` — uniform, well-applied. ✓
- All actions accessed inline at call site. ✓

**Hook naming**:
- 96 hooks total, all `useXxx` PascalCase suffix — consistent. ✓
- Issue is **location** not name.

#### 0.2.3 Folder organization flaws

| Current path | Issue | Target path |
|---|---|---|
| `components/settings/vocabulary/*.tsx` (8 files) AND `components/workbench/vocabulary/*.tsx` (6 files) | Same domain split | Merge under `components/vocabulary/{tabs,form,list,editor}/` |
| `components/recipes/*.tsx` AND `components/workbench/recipes/*.tsx` | Same domain split | Merge under `components/recipes/` (drop `workbench/recipes/`). |
| `components/widgets/*.tsx` (17 files) | Standalone; only used by dashboard | Move under `components/dashboard/widgets/`. |
| `components/glossary/binding-panel.tsx` (1 file) AND `components/workbench/glossary/*.tsx` | Same domain split | Move binding-panel under `components/glossary/`. |
| `components/workbench/dashboard-layout.tsx` AND `components/workbench/dashboard/*` | Some at workbench root, some under workbench/dashboard | Move all dashboard chrome into `components/dashboard/`. |
| `lib/use-*.ts` (12 files) AND `hooks/*.ts` (23 files) | Hooks split across two roots | Consolidate under `hooks/` (workspace, locale, dom, store-side hooks). |

#### 0.2.4 Dead code / deprecated exports

| Item | Location | Action |
|---|---|---|
| `WIDE_SETTINGS_PAGES = new Set<string>()` (deprecated empty Set) | `lib/constants/settings.ts:33` | **Delete export entirely**. |
| `_DeletedHeaderPlaceholder` (placeholder note from v1 migration) | `components/settings/vocabulary/json-entity-crud-page.tsx` | **Delete the comment**, file is clean. |
| `Button` variants `error` AND `danger` (semantically duplicate) | `components/ui/button.tsx` | **Drop `error` variant** from Button (keep on Alert where state semantics fit). |

#### 0.2.5 Logical flaws

| Flaw | Evidence | Fix |
|---|---|---|
| `Header` component pattern in `json-entity-crud-page.tsx` was deleted, but the comment block remains | dev-loop noise | Strip the migration comment; the parent `<WorkbenchPageShell>` is the contract now. |
| Glossary RightPane and Vocabulary inner tabs both render `<button role="tab">` directly instead of using `<TabBar>` | bespoke patterns flagged this session, fixed colors but not structure | Migrate to `<TabBar>` so all tab strips share keyboard, focus, ARIA. |
| `Modal` open/close not animated; backdrop fades but content doesn't scale | inconsistent with motion plan | All dialogs go through one `<Modal>` primitive that owns the choreography. |
| Settings sidebar "back to workbench" link is a manual `<Link>`+SVG pattern duplicated in workbench too | DRY violation | Lift to `<BackToWorkbenchLink>` primitive. |

---

## 1. Design principles (final)

1. **Single source of truth per concern.** One token file, one primitive per concept, one folder per domain.
2. **Semantic over palette.** Feature code never mentions `emerald`/`amber`/`red`/`violet`/`zinc`. Only `brand`/`success`/`warning`/`danger`/`info`/`concept`/`surface`/`divider`/`muted`/`foreground`.
3. **Greenfield refactor.** No backward-compat exports, no `@deprecated`, no shim files. Every PR is a clean cutover.
4. **kebab-case files, PascalCase components, camelCase functions/hooks/variables.** Filename matches default export.
5. **Co-location by domain, not by type.** All vocabulary code under `components/vocabulary/`, not split between `settings/` and `workbench/`.
6. **One primitive per affordance.** Buttons → `<Button>` + `<IconButton>`. Tabs → `<TabBar>`. Empty → `<EmptyState>`. Modals → `<Modal>`. Cards → `<Card>`. Tables → `<DataTable>`. Page chrome → `<WorkbenchPageShell>` / `<SettingsPageShell>` / `<BootstrapPageShell>`.
7. **Motion is built-in, not opt-in.** Page transitions, dialog enter/exit, list stagger, tab indicator slide are part of the primitive — feature code doesn't import `motion` directly except for unique cases.
8. **Reading first, density second.** 14px body, 12-13px secondary, 10-11px badges only. CJK leading 1.6.
9. **Accessibility is structural.** WCAG AA contrast at the token layer, semantic HTML at the primitive layer, keyboard support inside primitives.
10. **Reduced-motion is honored at the token layer.** `--duration-X: 0ms` under `prefers-reduced-motion: reduce` — primitives compose against tokens, not magic numbers.

---

## 2. Naming conventions (final)

### 2.1 Files

- **Always** kebab-case: `workbench-page-shell.tsx`, `status-badge.tsx`, `recipe-card.tsx`.
- **No exceptions** — `TableSelector.tsx` becomes `table-selector.tsx`.
- Test files: `*.test.tsx` next to source OR `__tests__/X.test.tsx` colocated.
- One primary export per file. Companion types/sub-components allowed if they don't need their own file.

### 2.2 Components

- **PascalCase**, file-name-matches-default-export: `recipes-workbench.tsx` exports `RecipesWorkbench` (not `RecipesPage`, not `RecipesPageContent`).
- **Suffix**: only when functionally meaningful and documented:
  - `Provider` — context provider with no UI surface other than `{children}`.
  - `Dialog` — modal surface.
  - `Panel` — bounded surface inside a layout.
  - `Card` — visually elevated bounded surface.
  - `Form` — form with submit semantics.
  - `Layout` / `Shell` — full-page chrome.
  - `Section` — sub-region inside a panel/card.
  - **No** generic `Component` / `Wrapper` / `Container` / `Box`.
- **Prefix `App`** is reserved for the root app shell. Don't prefix domain components with `App`.

### 2.3 Props

- **Booleans**:
  - `open` (visibility of overlay surfaces — dialog, popover, drawer, menu).
  - `disabled` (interaction).
  - `loading` (async state).
  - `selected` / `active` (UI state).
  - `defaultX` for uncontrolled initial value.
  - **Forbidden**: `isOpen`, `isVisible`, `isLoading`, `isDisabled`, `isSelected`, `isActive`, `visible`. Lint-enforced.
- **Callbacks**:
  - `onChange(value)` — value update.
  - `onSelect(item)` — item-level selection.
  - `onClose()` — user-driven dismissal.
  - `onSubmit(values)` — form submit.
  - `on<Domain>Action` for domain verbs (`onCreateRecipe`, `onPinResult`).
  - **Forbidden**: `handleX` as prop (only as internal handler in component body).
- **Slots**:
  - `children` for the primary content.
  - `actions` for trailing actions (right-aligned in headers, footer in cards).
  - `leading` / `trailing` for sides (icons, accessories).
  - `header` / `footer` for explicit named slots.
  - **Forbidden**: `slot1`, `extra`, `topContent`.

### 2.4 Hooks

- `useX` always. PascalCase X.
- Domain prefix when applicable: `useApi*` for query/mutation, `useStore*` for store selectors (rare; usually use `selectStateX` directly).
- File: `hooks/use-x.ts` (not `lib/use-x.ts`). Single root.

### 2.5 Types

(Mirrors `crates/ox-api/CLAUDE.md` for FE-side types.)

- Action DTOs: `Verb + Noun + (Request|Response)` — `CreateRecipeRequest`, `RunRecipeResponse`.
- Read-only data shapes: noun-only — `RecipeStatus`, `WorkspaceMember`, `OntologyMeta`.
- Component props interface: `<ComponentName>Props`.
- Discriminated unions: explicit `kind` field (`type Action = { kind: "create"; ... } | { kind: "delete"; ... }`).

### 2.6 Folders (the full target tree)

```
web/src/
├── app/                      ← Next.js App Router only. Pages re-export from components.
│   ├── (workbench)/
│   │   ├── analyze/page.tsx
│   │   ├── design/page.tsx
│   │   ├── dashboard/page.tsx
│   │   ├── explore/page.tsx
│   │   ├── glossary/page.tsx
│   │   ├── projects/page.tsx
│   │   ├── recipes/page.tsx
│   │   ├── vocabulary/page.tsx
│   │   └── layout.tsx
│   ├── settings/...
│   ├── bootstrap/...
│   ├── login/page.tsx
│   ├── layout.tsx, error.tsx, loading.tsx, not-found.tsx, page.tsx
│
├── components/
│   ├── ui/                   ← PRIMITIVES (no domain knowledge).
│   │   ├── button.tsx
│   │   ├── icon-button.tsx
│   │   ├── card.tsx          ← NEW
│   │   ├── modal.tsx         ← NEW (replaces 10+ bespoke Dialog patterns)
│   │   ├── empty-state.tsx
│   │   ├── status-badge.tsx
│   │   ├── tab-bar.tsx
│   │   ├── data-table.tsx    ← NEW
│   │   ├── kpi-card.tsx      ← NEW
│   │   ├── form-input.tsx
│   │   ├── select.tsx        ← extracted from form-input
│   │   ├── code-editor.tsx
│   │   ├── tooltip.tsx
│   │   ├── confirm-dialog.tsx
│   │   ├── prompt-dialog.tsx
│   │   ├── keyboard-shortcuts-dialog.tsx  ← renamed
│   │   ├── spinner.tsx
│   │   ├── skeleton.tsx
│   │   └── resize-handle.tsx
│   │
│   ├── layout/               ← APP-LEVEL CHROME.
│   │   ├── workbench-page-shell.tsx
│   │   ├── settings-page-shell.tsx       ← NEW (replaces hand-rolled headers)
│   │   ├── bootstrap-page-shell.tsx      ← NEW
│   │   ├── header.tsx
│   │   ├── sidebar.tsx
│   │   ├── settings-sidebar.tsx
│   │   └── back-to-workbench-link.tsx    ← NEW (DRY the "← 워크벤치로 돌아가기" link)
│   │
│   ├── motion/               ← NEW. Motion primitives + tokens.
│   │   ├── page-transition.tsx
│   │   ├── modal-transition.tsx
│   │   └── stagger-list.tsx
│   │
│   ├── providers/            ← React context wiring.
│   │   ├── query-provider.tsx
│   │   ├── confirm-provider.tsx          ← extracted from confirm-dialog.tsx
│   │   ├── prompt-provider.tsx           ← extracted from prompt-dialog.tsx
│   │   ├── tooltip-provider.tsx
│   │   ├── a11y-provider.tsx
│   │   └── bootstrap-provider.tsx
│   │
│   ├── analyze/              ← DOMAIN COMPONENTS by domain.
│   ├── chat/
│   ├── dashboard/
│   │   ├── dashboard-layout.tsx
│   │   ├── widget-grid.tsx
│   │   ├── widget-inspector.tsx
│   │   └── widgets/                      ← moved from /components/widgets
│   │       ├── bar-chart-widget.tsx
│   │       └── ...
│   ├── design/
│   ├── explore/
│   ├── glossary/                         ← merged binding-panel + workbench/glossary
│   ├── projects/
│   ├── recipes/                          ← merged /recipes + /workbench/recipes
│   ├── settings/                         ← settings-only components (sidebar items, page-specific forms)
│   └── vocabulary/                       ← merged settings/vocabulary + workbench/vocabulary
│
├── hooks/                    ← All custom hooks. /lib/use-*.ts moves here.
│   ├── api/                  ← React Query hooks (one per resource).
│   ├── use-auth.ts
│   ├── use-locale-chain.ts
│   ├── use-click-outside.ts
│   └── ...
│
├── lib/                      ← Pure functions, no React.
│   ├── api/                  ← API client (request, normalization, per-resource).
│   ├── store/                ← Zustand slices + selectors.
│   ├── locale/               ← localize, locale-chain.
│   ├── i18n-utils.ts
│   ├── cn.ts
│   └── constants/
│       └── settings.ts       ← NARROW_SETTINGS_PAGES only. WIDE_SETTINGS_PAGES deleted.
│
├── types/                    ← TypeScript types (no runtime).
│   ├── api.ts                ← Hand-curated shapes that wrap api.generated.
│   ├── api.generated.ts      ← openapi-typescript output (do not edit).
│   ├── ontology.ts
│   └── ...
│
└── messages/                 ← i18n catalogs (ko.json, en.json).
```

---

## 3. Token system (final)

### 3.1 Color (semantic only)

```css
@theme inline {
  /* Brand — primary affordance + active state. */
  --color-brand-foreground:        var(--color-emerald-700);
  --color-brand-foreground-strong: var(--color-emerald-800);
  --color-brand-surface:           var(--color-emerald-50);
  --color-brand-surface-strong:    var(--color-emerald-100);
  --color-brand-border:            var(--color-emerald-200);

  /* Status — AA-safe pairs. */
  --color-success-foreground: var(--color-emerald-700);
  --color-success-surface:    var(--color-emerald-50);
  --color-warning-foreground: var(--color-amber-700);
  --color-warning-surface:    var(--color-amber-50);
  --color-danger-foreground:  var(--color-red-700);
  --color-danger-surface:     var(--color-red-50);
  --color-info-foreground:    var(--color-sky-700);
  --color-info-surface:       var(--color-sky-50);

  /* Concept — ontology/glossary/governance content marker. */
  --color-concept-foreground: var(--color-violet-700);
  --color-concept-surface:    var(--color-violet-50);
  --color-concept-border:     var(--color-violet-200);

  /* Surface elevation. */
  --color-surface-base:    #ffffff;
  --color-surface-raised:  #fafafa;
  --color-surface-inset:   #f4f4f5;
  --color-surface-overlay: rgba(0,0,0,0.5);   /* modal backdrop */

  /* Divider. */
  --color-divider:      #e4e4e7;
  --color-divider-soft: rgba(0,0,0,0.04);

  /* Foreground. */
  --color-foreground:        #18181b;
  --color-foreground-muted:  #52525b;
  --color-foreground-subtle: #71717b;
  --color-foreground-strong: #09090b;
  --color-foreground-onbrand: #ffffff;
}

@media (prefers-color-scheme: dark) {
  :root {
    --color-brand-foreground:        var(--color-emerald-400);
    --color-brand-foreground-strong: var(--color-emerald-300);
    --color-brand-surface:           color-mix(in oklab, var(--color-emerald-950) 50%, transparent);
    --color-brand-surface-strong:    color-mix(in oklab, var(--color-emerald-950) 70%, transparent);
    --color-brand-border:            var(--color-emerald-800);

    --color-success-foreground: var(--color-emerald-400);
    --color-success-surface:    color-mix(in oklab, var(--color-emerald-950) 40%, transparent);
    --color-warning-foreground: var(--color-amber-400);
    --color-warning-surface:    color-mix(in oklab, var(--color-amber-950) 40%, transparent);
    --color-danger-foreground:  var(--color-red-400);
    --color-danger-surface:     color-mix(in oklab, var(--color-red-950) 40%, transparent);
    --color-info-foreground:    var(--color-sky-400);
    --color-info-surface:       color-mix(in oklab, var(--color-sky-950) 40%, transparent);

    --color-concept-foreground: var(--color-violet-400);
    --color-concept-surface:    color-mix(in oklab, var(--color-violet-950) 40%, transparent);
    --color-concept-border:     var(--color-violet-800);

    --color-surface-base:    #09090b;
    --color-surface-raised:  #18181b;
    --color-surface-inset:   #27272a;
    --color-surface-overlay: rgba(0,0,0,0.7);

    --color-divider:      #3f3f46;
    --color-divider-soft: rgba(255,255,255,0.06);

    --color-foreground:        #ededed;
    --color-foreground-muted:  #a1a1aa;
    --color-foreground-subtle: #71717b;
    --color-foreground-strong: #fafafa;
    --color-foreground-onbrand: #09090b;
  }
}
```

**Lint rule**: feature code may **not** reference `text-emerald-X`, `bg-emerald-X`, `text-amber-X`, `text-red-X`, `text-violet-X`, `text-sky-X` directly. Only the token names.

### 3.2 Typography

```css
@theme {
  --text-2xs: 11px;
  --text-xs:  13px;
  --text-sm:  14px;       /* DEFAULT body */
  --text-base: 15px;
  --text-lg:  17px;
  --text-xl:  20px;
  --text-2xl: 24px;
  --text-3xl: 30px;
  --text-4xl: 36px;       /* hero only */

  --leading-snug:    1.4;
  --leading-normal:  1.6;     /* CJK-friendly default */
  --leading-relaxed: 1.75;
}
```

**Lint rule**: `text-\[<11px\]` requires opt-out comment.

### 3.3 Radius

```css
@theme {
  --radius-xs: 2px;       /* not used in feature code; reserved */
  --radius-sm: 4px;       /* small chips, tags */
  --radius-md: 6px;       /* buttons, inputs, pills */
  --radius-lg: 8px;       /* cards, panels */
  --radius-xl: 12px;      /* dialogs, large surfaces */
  --radius-2xl: 16px;     /* hero/promotional only */
  --radius-full: 9999px;  /* pills, avatars */
}
```

Migrate `rounded` (no modifier) → `rounded-md`. Migrate stray `rounded-xl` on cards → `rounded-lg`.

### 3.4 Width

```css
@theme {
  --width-rail:           48px;     /* primary nav rail */
  --width-sidebar-narrow: 192px;    /* settings sidebar */
  --width-sidebar:        240px;    /* main sidebar with text labels */
  --width-inspector:      360px;    /* right-side inspector */
  --width-panel-narrow:   280px;    /* dropdowns, popovers */
  --width-panel:          480px;    /* small dialogs */
  --width-panel-wide:     640px;    /* large dialogs */
  --width-panel-full:     960px;    /* full-screen takeovers */
}
```

### 3.5 Motion

```css
@theme {
  --ease-out:    cubic-bezier(0.22, 1, 0.36, 1);
  --ease-in-out: cubic-bezier(0.65, 0, 0.35, 1);
  --ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1);  /* gentle overshoot */

  --duration-instant: 100ms;
  --duration-quick:   150ms;
  --duration-base:    200ms;
  --duration-slow:    350ms;
  --duration-slower:  500ms;
}

@media (prefers-reduced-motion: reduce) {
  :root {
    --duration-instant: 0ms;
    --duration-quick:   0ms;
    --duration-base:    0ms;
    --duration-slow:    0ms;
    --duration-slower:  0ms;
  }
}
```

### 3.6 Elevation (shadow)

```css
@theme {
  --shadow-1: 0 1px 2px 0 rgba(0,0,0,0.05);
  --shadow-2: 0 4px 8px -2px rgba(0,0,0,0.08), 0 2px 4px -1px rgba(0,0,0,0.04);
  --shadow-3: 0 12px 24px -4px rgba(0,0,0,0.10), 0 4px 8px -2px rgba(0,0,0,0.05);
  --shadow-4: 0 24px 48px -8px rgba(0,0,0,0.14), 0 8px 16px -4px rgba(0,0,0,0.08);
}
```

Map: `shadow-sm` → `shadow-1`, `shadow-md` → `shadow-2`, `shadow-lg` → `shadow-3`, `shadow-xl/2xl` → `shadow-4`.

---

## 4. Component library (final)

### 4.1 Primitives (`components/ui/`)

| Primitive | Replaces | Variants / props |
|---|---|---|
| `<Button>` | 131 raw `<button>` markup | `variant: primary \| secondary \| outline \| ghost \| danger`. Sizes: `xs \| sm \| md \| lg`. Slots: `leading`, `trailing`. **No** `error` variant. |
| `<IconButton>` | inline icon-only `<button>` | Same variants as Button. Required `aria-label`. |
| `<Card>` | 96 hand-rolled `rounded-X border bg-X` | `variant: surface \| raised \| inset`. Slots: `header`, `footer`. Composable: `<Card.Header>`, `<Card.Body>`, `<Card.Footer>`. |
| `<Modal>` | 10+ bespoke `Dialog.Root` + backdrop blocks | Wraps base-ui Dialog. Owns backdrop animation, content scale, focus trap, ESC. Slots: `header`, `footer`. Sizes: `sm \| md \| lg \| xl`. |
| `<EmptyState>` | 14 plain-text empty patterns | Already exists. Add `variant: inline \| card` and standardize icon-in-soft-circle treatment. |
| `<StatusBadge>` | 6 re-implementations | Already exists. Extend `colorMap` with token-based palette so callers pass `tone="success"` instead of a colorMap. |
| `<TabBar>` | 7 bespoke tab strips | Already exists. Add `motion`-backed indicator slide via `layoutId="tab-indicator"`. |
| `<DataTable>` | bespoke `<table>` markup in 6+ settings pages | Headless wrapper around `@tanstack/react-table`. Built-in sort/filter/pagination/sticky-header. |
| `<KpiCard>` | bespoke stat tiles in quality/models/signals/usage | `label`, `value`, `tone`, optional `delta`, optional `format` for thousand-separator/percent. |
| `<FormInput>` | already exists | Extract `<Select>` to its own primitive. |
| `<Tooltip>` | already exists | Keep. |
| `<Spinner>` | already exists | Keep. |
| `<Skeleton>` | already exists | Add `<SkeletonRow>`, `<SkeletonGrid>` composites. |
| `<ConfirmDialog>` | already exists | Move provider to `components/providers/confirm-provider.tsx`. |
| `<PromptDialog>` | already exists | Move provider to `components/providers/prompt-provider.tsx`. |
| `<KeyboardShortcutsDialog>` | already exists | Rename file to match. |

### 4.2 Layout shells (`components/layout/`)

| Shell | When to use | Renders |
|---|---|---|
| `<WorkbenchPageShell>` | content-oriented workbench modes (recipes, vocabulary, glossary, projects) | h-12 header + optional h-9 tab strip + scrollable content. |
| `<WorkbenchActionShell>` | action-oriented workbench modes (analyze, design, dashboard, explore) | h-8 toolbar with mode toggles + slot for action icons. NO title (sidebar conveys identity). |
| `<SettingsPageShell>` | every `/settings/*` page | Sidebar + h-12 page header + content. **Replaces** every hand-rolled `<header>`. |
| `<BootstrapPageShell>` | every `/bootstrap/*` step | Step rail + content + footer with Back/Next. |
| `<AuthPageShell>` | login | Centered card on dark bg. |

### 4.3 Motion primitives (`components/motion/`)

| Component | Behavior |
|---|---|
| `<PageTransition>` | Cross-fade + 4px y-offset. Wraps every shell's `{children}`. Reads `--duration-base`. |
| `<ModalTransition>` | Backdrop blur 0→8px, content scale 0.96→1, both 200ms ease-out. Used internally by `<Modal>`. |
| `<StaggerList>` | `staggerChildren: 0.04, delayChildren: 0.05`. For lists with N ≤ 8. |
| `<FadeIn>` | One-shot fade for late-arriving content (e.g., chart data load complete). |
| `<NumberTicker>` | Animated count-up for KPI values. Used by `<KpiCard>`. |

---

## 5. Phased plan (all phases hard cutover, no shims)

Each phase = one PR. Sequence is dependency-driven.

### Phase 1 — Token foundation (3h)

- Add semantic tokens to `globals.css` (§3.1–3.6).
- ESLint rule: `forbid-raw-palette-foreground` flagging `text-emerald-X`, `text-amber-X`, etc. in feature code.
- ESLint rule: `forbid-tiny-pixel-text` for `text-[<11px]`.
- ESLint rule: `forbid-magic-width` for `w-[Npx]` outside icon size range.

**Cutover**: tokens are introduced. Feature code is NOT migrated yet. Rules are warning-only initially.

### Phase 2 — Primitives + Shells (1 week, 5 PRs)

| PR | Adds | Deletes |
|---|---|---|
| 2.1 | `<Card>` primitive | — |
| 2.2 | `<Modal>` primitive | bespoke Dialog markup in 10 files |
| 2.3 | `<DataTable>` primitive | bespoke `<table>` markup in 6 settings pages |
| 2.4 | `<KpiCard>` primitive | bespoke stat-tiles in 4 settings pages |
| 2.5 | `<SettingsPageShell>` | hand-rolled `<header>` in every `/settings/*` page |

ESLint warning-only flips to **error** after this phase.

### Phase 3 — Naming + Folder cutover (1 day, 1 big PR)

Hard cutover. Single PR; no compat exports.

- Rename `TableSelector.tsx` → `table-selector.tsx`.
- Rename `keyboard-shortcuts.tsx` → `keyboard-shortcuts-dialog.tsx`.
- Rename `binding-panel.tsx` → `glossary-binding-panel.tsx`.
- Rename `widget-toolbar.tsx` → `widget-with-toolbar.tsx`.
- Rename component `Legend` → `GraphLegend`, `NodeDetailPanel` → `GraphDetailPanel`, `NodeDetail` → `EntityDetail`, `GroupNode` → `NodeGroup`, `GapsList` → `QualityGapsList`, `ReviewTOC` → `ReviewToc`.
- Move `lib/use-*.ts` (12 files) → `hooks/`.
- Move `components/widgets/*` → `components/dashboard/widgets/`.
- Move `components/recipes/*` → ... actually keep at `components/recipes/`. Move `components/workbench/recipes/recipes-workbench.tsx` → `components/recipes/recipes-workbench.tsx`. Drop `workbench/recipes/`.
- Merge `components/settings/vocabulary/*` + `components/workbench/vocabulary/*` → `components/vocabulary/`.
- Move `components/glossary/binding-panel.tsx` → `components/glossary/glossary-binding-panel.tsx`.
- Move `components/workbench/dashboard-layout.tsx` → `components/dashboard/dashboard-layout.tsx`. Drop `workbench/dashboard-layout.tsx`.
- Extract `confirm-dialog.tsx` provider → `providers/confirm-provider.tsx`. Same for prompt.
- Lift "← 워크벤치로 돌아가기" link into `<BackToWorkbenchLink>`.
- Standardize boolean prop names (`isOpen` → `open`, `isVisible` → `visible`).
- Drop `Button` `error` variant; the 1 site uses `danger`.
- **Delete** `WIDE_SETTINGS_PAGES` empty Set export from `lib/constants/settings.ts`.
- **Delete** `_DeletedHeaderPlaceholder` placeholder block from `json-entity-crud-page.tsx`.

ESLint adds `consistent-bool-prop-name` rule (forbids `isX` pattern except inside known third-party).

### Phase 4 — Token migration (2 days, 1 PR)

Codemod pass: replace all `text-emerald-X dark:text-emerald-Y` → `text-brand-foreground` etc. across feature code. After this, the Phase 1 ESLint rule flips from warning to **error**.

### Phase 5 — Empty state + StatusBadge sweep (1 day, 1 PR)

- All 14 plain-text empty states migrated to `<EmptyState>`.
- All 6 `StatusBadge` re-implementations deleted; callers use `<StatusBadge>` from `ui/`.
- All 7 bespoke tab strips migrated to `<TabBar>` or `<WorkbenchPageShell>` tabs.

### Phase 6 — Motion (3 PRs over 1 week)

| PR | Adds |
|---|---|
| 6.1 | Motion tokens (§3.5), `<PageTransition>`, applied to all shells. |
| 6.2 | `<ModalTransition>` integrated into `<Modal>`. `<TabBar>` indicator slide via `layoutId`. |
| 6.3 | `<StaggerList>`, `<NumberTicker>`, micro-interactions (button press, toast slide-in). |

### Phase 7 — Heading order + a11y (1 day, 1 PR)

- Fix axe `heading-order` violations on `/design`, `/dashboard`.
- Audit ARIA labels on all `<IconButton>` usages (lint enforced).
- Skip-link target verification.

### Phase 8 — CI gates (1 day, 1 PR)

- Playwright a11y suite covering 16 canonical routes, asserts 0 violations.
- ESLint design rules promoted to error.
- i18n key coverage script extended to fail on any missing key referenced by `t()`.
- Pixel-diff baseline stored for future visual regressions.

---

## 6. Deletion list (concrete)

Items deleted across phases — no shims, no compat:

```
DELETE web/src/lib/constants/settings.ts        :: WIDE_SETTINGS_PAGES export
DELETE web/src/components/settings/vocabulary/json-entity-crud-page.tsx :: _DeletedHeaderPlaceholder block
DELETE web/src/components/ui/button.tsx         :: variant="error" path
DELETE web/src/app/settings/providers/page.tsx  :: local StatusBadge fn
DELETE web/src/app/settings/lineage/page.tsx    :: local StatusBadge fn
DELETE web/src/app/settings/notifications/page.tsx :: local StatusBadge fn
DELETE web/src/components/workbench/bottom-panel/design-panel-shared.tsx :: local StatusBadge export
DELETE web/src/components/workbench/bottom-panel/recent-projects.tsx :: local StatusBadge fn
DELETE web/src/components/workbench/recipes/recipes-workbench.tsx :: local StatusBadge fn

DELETE web/src/components/widgets/              :: directory (move to dashboard/widgets/)
DELETE web/src/components/workbench/recipes/    :: directory (move to recipes/)
DELETE web/src/components/workbench/vocabulary/ :: directory (merge into vocabulary/)
DELETE web/src/components/settings/vocabulary/  :: directory (merge into vocabulary/)
DELETE web/src/components/workbench/dashboard-layout.tsx :: file (move to dashboard/)
DELETE web/src/components/glossary/binding-panel.tsx     :: file (rename glossary-binding-panel.tsx)

DELETE every plain-text empty pattern in 14 settings pages (replaced by <EmptyState>)
DELETE every bespoke <button role="tab"> + <span absolute -bottom-px /> in 7 files (replaced by <TabBar>)
DELETE every bespoke Dialog.Root + backdrop block in 10 files (replaced by <Modal>)
DELETE every raw text-emerald-X / bg-emerald-X / text-violet-X reference in feature code (replaced by tokens)
DELETE every text-[10px], text-[9px], text-[8px] reference in feature code (replaced by text-2xs OR explicit opt-out)
DELETE every w-[Npx] magic width in feature code (replaced by width tokens)
```

---

## 7. CI gates (final)

```yaml
# .github/workflows/ci.yml additions
- name: typecheck
  run: cd web && pnpm typecheck

- name: lint (design rules error)
  run: cd web && pnpm lint
  # forbid-raw-palette-foreground, forbid-tiny-pixel-text,
  # forbid-magic-width, consistent-bool-prop-name, no-bespoke-dialog,
  # no-bespoke-tab-strip, no-plain-empty-state, status-badge-only.

- name: a11y
  run: cd web && pnpm playwright test playwright/a11y.spec.ts

- name: i18n coverage
  run: cd web && node scripts/i18n-audit.mjs --typed --strict

- name: visual regression (advisory)
  run: cd web && pnpm playwright test playwright/pixel-diff.spec.ts || true
```

---

## 8. Success criteria (final, verifiable)

| Metric | Target | Verification |
|---|---|---|
| `text-emerald-X` / `text-amber-X` / `text-violet-X` in feature code | **0** | grep across `web/src` excluding `globals.css`, primitive files |
| `text-[<11px]` in feature code | **0** | grep |
| `w-[Npx]` magic widths | **< 10** (icons only) | grep + manual review |
| Bespoke `<button role="tab">` outside `TabBar` source | **0** | grep |
| Bespoke `Dialog.Root` outside `Modal` source | **0** | grep |
| Local `StatusBadge` re-implementations | **0** | grep |
| Plain-text `<p>{t("empty")}</p>` | **0** | grep |
| Files where filename ≠ default export | **0** | script (the audit grep loop in §0.2.2) |
| `isOpen` / `isVisible` props | **0** | grep |
| axe-core violations on canonical routes | **0** | Playwright |
| `pnpm typecheck` | exit 0 | CI |
| `pnpm lint --max-warnings=0` | exit 0 | CI |
| Brand color rebrand effort | < 5 file edits | grep `--color-brand-X` references |
| Adding a new settings page | composes existing primitives, no new utility classes | code review |
| Adding a new workbench mode | composes existing shell, no copy-paste from another mode | code review |

---

## 9. Out of scope (deferred)

- Full visual rebrand (illustrations, custom icons, mascot).
- Density toggle (compact / cozy / comfortable).
- Mobile responsive refactor.
- i18n beyond ko/en.
- White-label theming product UI.
- Manual color-mode switch (currently OS-driven).

---

## 10. Phase-PR template

Every phase PR must include:

1. **Visual diff**: before/after screenshots of at least 3 representative pages.
2. **Token diff**: list of new/changed CSS variables.
3. **Codemod summary**: number of files touched, regex used, any manual fixes.
4. **Deletion list**: every item deleted (no compat exports allowed).
5. **Lint baseline**: confirmation that `pnpm lint --max-warnings=0` passes.
6. **Risk note**: 1 sentence on the worst plausible regression.
7. **Rollback plan**: a single `git revert <sha>` reverts cleanly (since no migration shims exist, this is true by construction).

If a PR touches > 60 files, split it. Reviewers should read the whole diff in 15 minutes.

---

## 11. Same-session shipped (already merged in master)

- `<WorkbenchPageShell>` primitive.
- Vocabulary / Recipes / Glossary migrated to shell.
- Violet UI-tabs → emerald (4 files).
- `NARROW_SETTINGS_PAGES` opt-out (providers / prompts lifted to wide).
- `text-emerald-600` → `text-emerald-700` for paired foreground (~16 files).
- `routingMatrix` i18n key.
- `<div id="main">` hydration mismatch.
- next/font variable scope on `<html>`.
- `/api/healthz` flat probe + dev.sh envelope-defensive.
- Prompts settings empty state migrated to `<EmptyState>`.

These are starting state; they are NOT to be re-done in Phase 1. They establish the baseline this plan extends.

---

## 12. Decision log

| Decision | Rationale |
|---|---|
| Hard cutover, no compat | User directive ("처음부터 이렇게 설계된것처럼 바로 레거시도 제거"). Compat shims rot; design as if fresh. |
| Single root for hooks (`hooks/`) | 12 hooks in `lib/use-*` violate DRY locality. Move all under `hooks/`. |
| Domain folders for components, not type-folders | `components/recipes/` not `components/lists/recipe-list.tsx`. Locality > taxonomy. |
| One shell per surface family (workbench / settings / bootstrap / auth) | Five shells × ~25 pages prevents shell sprawl while keeping each shell focused. |
| Token-only color references in feature code | Brand color change = 1 token edit, not 200 component edits. |
| `danger` over `error` for buttons | Action-tone semantics. `error` is reserved for state in Alert. |
| `open`/`disabled`/`loading`/`selected` over `isOpen`/`isDisabled`/etc. | React + DOM convention; lint-enforceable. |
| `motion/react` only inside motion primitives | Feature code consumes `<PageTransition>`/`<Modal>`, not `<motion.div>`. |
| CJK leading 1.6 default | Hangul + Latin mix needs more leading than 1.5. |
| Reduced-motion at token layer | `--duration-X: 0` flip vs scattered `prefers-reduced-motion` rules. |

---

## 13. Approval gates

This plan is the final design. Each phase requires explicit user approval before merge:

- **Phase 1** (tokens) — low risk, no visual change.
- **Phase 2** (primitives + shells) — visual change on settings pages. **Bigger user-visible change.**
- **Phase 3** (naming + folder) — no user-visible change but high diff volume. Reviewable in 30 min if pattern is clear.
- **Phase 4** (token migration) — large diff but mechanical. Codemod + visual diff confirms equivalence.
- **Phase 5** (sweep) — visual change, mostly improvements.
- **Phase 6** (motion) — qualitative judgment; includes screencast in PR.
- **Phase 7** (a11y) — small.
- **Phase 8** (CI) — gates land last; flipping ESLint rules to error catches anything missed.

Recommended start: **Phase 1 + Phase 2.1 (Card) together**, since `<Card>` is the component most-blocking for the rest. Then Phase 2.5 (`<SettingsPageShell>`) for the highest user-visible impact.
