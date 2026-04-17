# `hooks/api/` — TanStack Query hooks (Phase 5.1)

Every data-fetching hook in the app lives here, one file per feature. The goal is that components never call `listX()` / `fetch` inside `useEffect` — they import a typed hook and let TanStack Query handle lifecycle, cache, retries, and invalidation.

## Rules

1. **One file per feature** named `use-<feature>.ts` (e.g. `use-dashboards.ts`). Matches the structure of `src/lib/api/<feature>.ts`.
2. **Always export a `<feature>Keys` factory** with hierarchical query keys:
   ```ts
   export const dashboardsKeys = {
     all: ["dashboards"] as const,
     lists: () => [...dashboardsKeys.all, "list"] as const,
     list: (params?) => [...dashboardsKeys.lists(), params ?? {}] as const,
     details: () => [...dashboardsKeys.all, "detail"] as const,
     detail: (id) => [...dashboardsKeys.details(), id] as const,
   };
   ```
   Rationale: `invalidateQueries({ queryKey: dashboardsKeys.lists() })` invalidates every list variant regardless of params. Hand-rolled arrays drift.
3. **Reuse the API layer.** Fetching logic belongs in `src/lib/api/*.ts`. Hooks only wrap those calls. Never inline `fetch` calls in a hook.
4. **Plain `useQuery` is the default.** Reach for `useInfiniteQuery` only when the UI actually accumulates pages (e.g. a "Load more" button). For selectors / filters that replace the whole page, plain `useQuery` is simpler and cheaper.
5. **Mutations must invalidate or update caches.** Use `invalidateQueries` at minimum. For single-entity updates, `setQueryData(detail(id), result)` avoids an extra round-trip. For list-mutating operations where lag is visible (delete, status toggle), prefer optimistic updates with rollback in `onError`.
6. **Keep `request<T>` envelope unwrapping untouched.** `client.ts::request<T>` already unwraps `{ data, pagination, meta }`. Hooks receive the same shape the legacy call sites did (`CursorPage<T>`, single resources, etc.).
7. **Do not guess default options per-hook.** Global defaults live in `src/lib/query/client.ts` (`staleTime: 30s`, `retry: 2` with 4xx short-circuit, `refetchOnWindowFocus: false`, `mutations.retry: false`). Override only when you have a concrete reason — document it in a `Why:` comment above the override.

## Migration pattern

Before:

```tsx
const [items, setItems] = useState<Foo[]>([]);
const [loading, setLoading] = useState(true);

useEffect(() => {
  listFoos({ limit: 50 })
    .then((p) => setItems(p.items))
    .catch(() => toast.error("..."))
    .finally(() => setLoading(false));
}, []);
```

After:

```tsx
const { data, isLoading, isError } = useFoos({ limit: 50 });
const items = data?.items ?? [];

useEffect(() => {
  if (isError) toast.error("...");
}, [isError]);
```

For mutations:

```tsx
const { mutate: createFoo, isPending } = useCreateFoo();
// then: createFoo(payload, { onSuccess: () => toast.success("Created") });
```

## `enabled` for dependent fetches

When an id is nullable, gate with `enabled`:

```ts
useFoo(id, { enabled: !!id });
```

Hooks in this folder already do this internally when a nullable id is a parameter (see `useDashboard(id)`), so call sites stay clean.

## Query key dictionary

- `ontologies` — `listOntologies`
- `dashboards` — `listDashboards`, `getDashboard`
- `projects` — `listProjects`, `getProject`
- `knowledge` — `listKnowledge`, `knowledgeStats`

Add new keys to the table as hooks land.
