import "@testing-library/jest-dom/vitest";
import { expect } from "vitest";
import * as axeMatchers from "vitest-axe/matchers";
import type { AxeMatchers } from "vitest-axe/matchers";

// Register vitest-axe matchers so tests can call `expect(...).toHaveNoViolations()`.
// Importing from `vitest-axe/matchers` (not `vitest-axe`) keeps the CJS/ESM
// interop clean under vitest 4 + node ESM.
expect.extend(axeMatchers);

declare module "vitest" {
  // Vitest's own `Assertion<T>` has no default type parameter, so we must
  // mirror that shape verbatim when augmenting (TypeScript requires
  // identical type parameters across declaration merging).
  interface Assertion<T> extends AxeMatchers {
    __phantom?: T;
  }
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  interface AsymmetricMatchersContaining extends AxeMatchers {}
}
