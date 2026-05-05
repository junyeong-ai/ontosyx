import "@testing-library/jest-dom/vitest";
import { expect } from "vitest";
import * as axeMatchers from "vitest-axe/matchers";
import type { AxeMatchers } from "vitest-axe/matchers";

// Register vitest-axe matchers so tests can call `expect(...).toHaveNoViolations()`.
// Importing from `vitest-axe/matchers` (not `vitest-axe`) keeps the CJS/ESM
// interop clean under vitest 4 + node ESM.
expect.extend(axeMatchers);

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
