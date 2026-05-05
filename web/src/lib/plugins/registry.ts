// Generic plugin registry — module-singleton state + subscriber list
// + cached snapshot, parameterised over the item shape. Three current
// surfaces share this pattern:
//
//   - `lib/command-registry.ts`         (CommandSource)
//   - `components/workbench/inspector/facets/registry.tsx` (InspectorFacet)
//   - future: i18n message catalogues, validator/edit-op contributions
//
// Centralising the bookkeeping here means each call-site exposes only
// its typed wrapper (registerCommandSource / registerInspectorFacet)
// and inherits the snapshot caching that `useSyncExternalStore`
// requires for stable referential identity.
//
// Re-registering an existing id replaces the prior entry — idempotent
// under HMR / React StrictMode double-mount, so plugin loaders don't
// need their own dedupe logic.

export interface PluginItem {
  /** Unique identifier within the registry. Re-using an id replaces
   *  the prior entry. */
  id: string;
}

export interface PluginRegistryOptions<T extends PluginItem> {
  /** Optional comparator for sorted snapshots. Defaults to insertion
   *  order (registration time). Use this for surfaces where the
   *  iteration order is meaningful (e.g. command palette source
   *  groups, inspector facet tab strip). */
  compare?: (a: T, b: T) => number;
}

type Listener = () => void;

export class PluginRegistry<T extends PluginItem> {
  private items = new Map<string, T>();
  private order: string[] = [];
  private listeners = new Set<Listener>();
  // Cached snapshot — `list()` is read by `useSyncExternalStore`,
  // which compares snapshots by referential identity. Returning a
  // fresh array each call would loop the React scheduler.
  private snapshot: T[] | null = null;
  private compare: ((a: T, b: T) => number) | undefined;

  constructor(options: PluginRegistryOptions<T> = {}) {
    this.compare = options.compare;
  }

  /**
   * Register or replace an item. Returns an unregister thunk that
   * removes the item if (and only if) it's still the active entry
   * for that id — re-registering a fresh payload over the same id
   * invalidates older callers' unregister thunks gracefully.
   *
   * `position` controls placement when the id is fresh:
   * - `{ before: id }` — insert ahead of an existing entry. Falls
   *   back to append if the named id isn't registered.
   * - `{ after:  id }` — insert behind an existing entry. Same
   *   fallback.
   * - omitted — append.
   *
   * Re-registering an existing id preserves its current position
   * regardless of `position` (idempotent under HMR / StrictMode).
   */
  register(
    item: T,
    position?: { before?: string; after?: string },
  ): () => void {
    const existing = this.items.get(item.id);
    this.items.set(item.id, item);
    if (!existing) {
      if (position?.before && this.order.includes(position.before)) {
        const idx = this.order.indexOf(position.before);
        this.order.splice(idx, 0, item.id);
      } else if (position?.after && this.order.includes(position.after)) {
        const idx = this.order.indexOf(position.after);
        this.order.splice(idx + 1, 0, item.id);
      } else {
        this.order.push(item.id);
      }
    }
    this.invalidate();
    return () => {
      if (this.items.get(item.id) === item) {
        this.unregister(item.id);
      }
    };
  }

  /** Remove a previously-registered item. Idempotent. */
  unregister(id: string): void {
    if (!this.items.has(id)) return;
    this.items.delete(id);
    const idx = this.order.indexOf(id);
    if (idx >= 0) this.order.splice(idx, 1);
    this.invalidate();
  }

  /** Look up an item by id. Returns `undefined` for unregistered ids. */
  get(id: string): T | undefined {
    return this.items.get(id);
  }

  /**
   * Snapshot of all currently-registered items. Memoised across
   * calls — only re-computed after a registry mutation invalidates
   * the cache. The referential stability is required by
   * `useSyncExternalStore` consumers.
   */
  list(): T[] {
    if (this.snapshot === null) {
      const items = this.order
        .map((id) => this.items.get(id))
        .filter((it): it is T => Boolean(it));
      this.snapshot = this.compare ? [...items].sort(this.compare) : items;
    }
    return this.snapshot;
  }

  /**
   * Subscribe to registry mutations. Returns the unsubscribe thunk.
   * Use through `useSyncExternalStore` so React stays in lockstep
   * with non-React registrations.
   */
  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /** Reset the registry to empty. Test-only escape hatch. */
  clearForTests(): void {
    this.items.clear();
    this.order.length = 0;
    this.invalidate();
  }

  private invalidate(): void {
    this.snapshot = null;
    for (const listener of this.listeners) listener();
  }
}
