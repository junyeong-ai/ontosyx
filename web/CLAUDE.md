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
