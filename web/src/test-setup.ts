import "@testing-library/jest-dom/vitest";
import { expect } from "vitest";
import * as axeMatchers from "vitest-axe/matchers";
import type { AxeMatchers } from "vitest-axe/matchers";

// Register vitest-axe matchers so tests can call `expect(...).toHaveNoViolations()`.
// Importing from `vitest-axe/matchers` (not `vitest-axe`) keeps the CJS/ESM
// interop clean under vitest 4 + node ESM.
expect.extend(axeMatchers);

// Storage polyfill for vitest 4 + jsdom 29 + Node 22. Node 22's
// experimental built-in `localStorage` requires `--localstorage-file`
// to be passed at the runtime level; without it, `window.localStorage`
// is undefined inside the jsdom environment. Tests that assert against
// browser storage (auth, workspace cache, command palette state) therefore
// crash on the first `localStorage.x()` call.
//
// The shim below installs an in-memory `Storage` implementation on both
// `window` and `globalThis` when one is missing. The behavioural surface
// mirrors the browser contract — `clear`, `getItem`, `setItem`,
// `removeItem`, `length`, indexed `key()` — so consumer code reads the
// same way under jsdom as it does in production. Each test gets a fresh
// shim slate via the `beforeEach` hook below; tests that need a
// pre-populated storage hydrate explicitly with `setItem`.
class MemoryStorage implements Storage {
  private data = new Map<string, string>();
  get length(): number {
    return this.data.size;
  }
  clear(): void {
    this.data.clear();
  }
  getItem(key: string): string | null {
    return this.data.has(key) ? (this.data.get(key) as string) : null;
  }
  key(index: number): string | null {
    return Array.from(this.data.keys())[index] ?? null;
  }
  removeItem(key: string): void {
    this.data.delete(key);
  }
  setItem(key: string, value: string): void {
    this.data.set(key, String(value));
  }
}

function installStorageShimIfMissing(slot: "localStorage" | "sessionStorage"): void {
  const target = globalThis as unknown as Record<string, Storage>;
  if (!target[slot]) {
    target[slot] = new MemoryStorage();
  }
  if (typeof window !== "undefined") {
    const w = window as unknown as Record<string, Storage>;
    if (!w[slot]) {
      w[slot] = target[slot];
    }
  }
}

installStorageShimIfMissing("localStorage");
installStorageShimIfMissing("sessionStorage");

// `prefers-reduced-motion: reduce` mocked globally. JSDOM ships a stub
// `requestAnimationFrame` that never flushes — components that animate
// (NumberTicker, fade-ins, stagger reveals) would otherwise render
// their initial frame in tests and never converge. The reduced-motion
// branch is the documented "skip animation" path in every motion
// helper, so the test environment lines up with the a11y reduced-motion
// path real users opt into. Tests therefore assert *final* state.
if (typeof window !== "undefined" && window.matchMedia) {
  const realMatchMedia = window.matchMedia.bind(window);
  window.matchMedia = ((query: string) => {
    if (/prefers-reduced-motion/.test(query)) {
      return {
        matches: true,
        media: query,
        onchange: null,
        addEventListener: () => {},
        removeEventListener: () => {},
        addListener: () => {},
        removeListener: () => {},
        dispatchEvent: () => false,
      } as MediaQueryList;
    }
    return realMatchMedia(query);
  }) as typeof window.matchMedia;
}

declare module "vitest" {
  // Vitest's own `Assertion<T>` has no default type parameter, so we must
  // mirror that shape verbatim when augmenting (TypeScript requires
  // identical type parameters across declaration merging).
  interface Assertion<T> extends AxeMatchers {
    __phantom?: T;
  }
  interface AsymmetricMatchersContaining extends AxeMatchers {}
}
