import { describe, expect, it } from "vitest";
import { renderHook } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { useDiagnosticResolver } from "./diagnostic";

const messages = {
  diagnostics: {
    runtime: {
      cypher: {
        complexity: {
          unbounded_var_length:
            "Variable-length relationship `{relationship}` has no upper bound",
        },
      },
    },
  },
} as const;

/** Test harness — provides next-intl + TanStack Query context.
 *  `useDiagnosticResolver` chains through `useLocaleChain` which
 *  uses `useQuery`, so a QueryClient must be in scope; without a
 *  workspace cookie the locale chain falls through to
 *  `DEFAULT_LOCALE_CHAIN = ["ko", "en"]`. */
function withProvider(
  node: React.ReactNode,
  catalogue: Record<string, unknown> = messages,
) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return (
    <QueryClientProvider client={qc}>
      <NextIntlClientProvider locale="en" messages={catalogue}>
        {node}
      </NextIntlClientProvider>
    </QueryClientProvider>
  );
}

describe("useDiagnosticResolver", () => {
  it("renders a catalogued code via ICU MessageFormat substitution", () => {
    const { result } = renderHook(() => useDiagnosticResolver(), {
      wrapper: ({ children }) => withProvider(children),
    });
    const rendered = result.current({
      code: "runtime.cypher.complexity.unbounded_var_length",
      message: "fallback ignored when catalogue hits",
      params: { relationship: "r" },
    });
    expect(rendered).toBe("Variable-length relationship `r` has no upper bound");
  });

  it("falls back to the structured English `message` on catalogue miss", () => {
    const { result } = renderHook(() => useDiagnosticResolver(), {
      wrapper: ({ children }) => withProvider(children),
    });
    const rendered = result.current({
      code: "runtime.cypher.complexity.never_catalogued",
      message: "english fallback wins",
      params: {},
    });
    expect(rendered).toBe("english fallback wins");
  });

  it("falls back when an intermediate namespace segment is missing", () => {
    const { result } = renderHook(() => useDiagnosticResolver(), {
      wrapper: ({ children }) => withProvider(children),
    });
    const rendered = result.current({
      code: "runtime.cypher.shacl.min_count_missing", // shacl namespace missing
      message: "fallback for missing-branch",
    });
    expect(rendered).toBe("fallback for missing-branch");
  });

  it("treats a non-string leaf as a catalogue miss", () => {
    const { result } = renderHook(() => useDiagnosticResolver(), {
      // shacl namespace is an object, not a leaf string — should miss
      wrapper: ({ children }) => withProvider(children),
    });
    const rendered = result.current({
      code: "runtime.cypher.complexity", // points at the object, not a leaf
      message: "fallback for object leaf",
    });
    expect(rendered).toBe("fallback for object leaf");
  });

  // LocalizedText param resolution — the cross-cutting concern that
  // separates Wave 9.3 (single-locale params) from Wave 9.4
  // (locale-aware params). Test environments without a workspace
  // context fall back to the static `DEFAULT_LOCALE_CHAIN = ["ko", "en"]`,
  // so a Korean-first param resolves to its Korean form.

  it("resolves LocalizedText params via the active locale chain before ICU substitution", () => {
    const { result } = renderHook(() => useDiagnosticResolver(), {
      wrapper: ({ children }) => withProvider(children),
    });
    const rendered = result.current({
      code: "runtime.cypher.complexity.unbounded_var_length",
      message: "english fallback",
      params: {
        relationship: {
          default: "관계",
          translations: { ko: "관계", en: "relationship" },
        },
      },
    });
    // chain ["ko", "en"] → ko wins, even though the catalogue locale is en
    expect(rendered).toBe(
      "Variable-length relationship `관계` has no upper bound",
    );
  });

  it("falls back to LocalizedText.default when no chain entry matches", () => {
    const { result } = renderHook(() => useDiagnosticResolver(), {
      wrapper: ({ children }) => withProvider(children),
    });
    const rendered = result.current({
      code: "runtime.cypher.complexity.unbounded_var_length",
      message: "english fallback",
      params: {
        relationship: {
          default: "デフォルト",
          translations: { ja: "片道" }, // no ko/en — chain misses, fall to default
        },
      },
    });
    expect(rendered).toBe(
      "Variable-length relationship `デフォルト` has no upper bound",
    );
  });

  it("passes scalar params through unchanged alongside LocalizedText params", () => {
    const messages2 = {
      diagnostics: {
        test: { mixed: "{name} authored at depth {depth}" },
      },
    } as const;
    const { result } = renderHook(() => useDiagnosticResolver(), {
      wrapper: ({ children }) => withProvider(children, messages2),
    });
    const rendered = result.current({
      code: "test.mixed",
      message: "english fallback",
      params: {
        name: { default: "이름", translations: { ko: "이름", en: "name" } },
        depth: 3,
      },
    });
    expect(rendered).toBe("이름 authored at depth 3");
  });

  it("does not misclassify a plain object that happens to have a `default` key", () => {
    const messages2 = {
      diagnostics: {
        test: { plain_object: "{value}" },
      },
    } as const;
    const { result } = renderHook(() => useDiagnosticResolver(), {
      wrapper: ({ children }) => withProvider(children, messages2),
    });
    // `translations` present but with a non-string value → not
    // LocalizedText; the object passes through to ICU as-is.
    const rendered = result.current({
      code: "test.plain_object",
      message: "english fallback",
      params: {
        value: { default: "x", translations: { ko: 42 as unknown as string } },
      },
    });
    // Catalogue substitution stringifies the object via its
    // default `toString` (`[object Object]`). The point of the
    // test is that the resolver did NOT mistake it for a
    // LocalizedText and resolve to "x" — the `translations` shape
    // disqualified it.
    expect(rendered).toContain("[object Object]");
  });
});
