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

Every server-response handler that returned a `DesignProject` (handleSave, refine, restore, extend, reanalyze, complete, fork, delete) now lands its result through `applyProjectSnapshot`. Drift between `activeProject.ontology` and the slice cache is structurally impossible.

## Header actions: "Extend source" 1st-class

The design canvas top-bar carries an emerald "Extend source" button next to the inspector toggle. Clicking it fires `requestExtendSource()` (a monotonic counter on `ChromeSlice`) which `EnhanceActions` watches and auto-opens the extend sub-form on change. New header / shortcut callers should drive the same store action rather than prop-drilling — keeps the extension flow a one-click discovery without coupling.
