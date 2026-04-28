/**
 * `DiagnosticMessage` rendering — the FE-side counterpart to the
 * Rust `ox_core::DiagnosticMessage` (RFC 7807 / gRPC `Status` shape).
 *
 * The platform emits structured diagnostics carrying:
 *
 * - `code` — stable dotted identifier (`<crate>.<surface>.<kind>`).
 *   The catalogue key the FE renders translations against.
 * - `message` — English fallback rendering. Always present; used
 *   when the catalogue has no entry for `code` (new diagnostic
 *   landed before the i18n team translated it).
 * - `params` — placeholder values for ICU MessageFormat
 *   substitution. Content-bearing params (rule names, glossary
 *   terms) ride the wire as `LocalizedText` shapes
 *   (`{default, translations}`); the resolver auto-detects this
 *   structure and resolves to the active workspace locale chain
 *   *before* ICU substitution, so a Korean admin sees a Korean
 *   `{rule_name}` and the LLM context sees an English one — every
 *   consumer renders in the language it prefers.
 *
 * `useDiagnosticResolver()` returns a memoised function that walks
 * the `next-intl` message tree to detect the catalogue presence
 * deterministically — instead of relying on `next-intl`'s
 * missing-key behaviour (echoing the key) which differs between
 * dev and prod and couples to library internals. Catalogue hit →
 * resolve LocalizedText params, then render via `t(code, params)`.
 * Catalogue miss → return the structured English `message`.
 */

import { useCallback } from "react";
import { useTranslations, useMessages } from "next-intl";

import type { DiagnosticMessage, LocalizedText } from "@/types/api";
import { localize } from "@/lib/locale/localize";
import { useLocaleChain, type LocaleSurface } from "@/lib/use-locale-chain";

/** The top-level namespace under which diagnostic catalogues live. */
const DIAGNOSTICS_NAMESPACE = "diagnostics";

/**
 * Resolver function returned by [`useDiagnosticResolver`]. Pure
 * given the (locale chain, messages) context it captured at hook
 * time.
 */
export type DiagnosticResolver = (diagnostic: DiagnosticMessage) => string;

/**
 * Returns a memoised resolver that renders any
 * [`DiagnosticMessage`] through the active `next-intl` catalogue,
 * resolving any [`LocalizedText`]-shaped params to the user's
 * locale before ICU substitution and falling back to the
 * diagnostic's English `message` when no catalogue entry exists
 * for `code`.
 *
 * `surface` selects which workspace locale chain drives content
 * resolution — admin (default) for operator UIs, llm for surfaces
 * that mirror the agent's tool-result context.
 *
 * Multi-instance safe — `useTranslations`, `useMessages`, and
 * `useLocaleChain` all read shared provider / TanStack-cached
 * state, so N components calling this hook produce identical
 * resolvers (no fan-out fetches, no divergent state).
 */
export function useDiagnosticResolver(
  surface: LocaleSurface = "admin",
): DiagnosticResolver {
  const t = useTranslations(DIAGNOSTICS_NAMESPACE);
  const messages = useMessages();
  const localeChain = useLocaleChain(surface);

  return useCallback<DiagnosticResolver>(
    (diagnostic) => {
      if (
        !catalogueHasKey(messages, [
          DIAGNOSTICS_NAMESPACE,
          ...diagnostic.code.split("."),
        ])
      ) {
        return diagnostic.message;
      }
      const resolvedParams = resolveLocalizedParams(
        diagnostic.params ?? {},
        localeChain,
      );
      // `next-intl` types the key as a literal union derived from
      // the catalogue tree; diagnostic codes are stringly-typed by
      // design (open extension point for new BE diagnostics) so we
      // widen here. The `catalogueHasKey` check above guarantees
      // the lookup will succeed.
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      return t(diagnostic.code as any, resolvedParams as any);
    },
    [t, messages, localeChain],
  );
}

/**
 * Walk every value in `params`; for each one whose shape matches
 * the canonical [`LocalizedText`] wire form, resolve to a single
 * string against `chain`. Other values pass through untouched.
 *
 * The returned map is suitable for ICU MessageFormat substitution:
 * every entry is either the original scalar or the resolved
 * locale-specific string. Allocation is `O(params.size)` plus the
 * one new object — locale-aware emit sites are diagnostic emit
 * sites, not hot path.
 */
function resolveLocalizedParams(
  params: Record<string, unknown>,
  chain: readonly string[],
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(params)) {
    out[key] = isLocalizedText(value)
      ? localize(value, chain)
      : value;
  }
  return out;
}

/**
 * Structural check for the canonical [`LocalizedText`] wire shape:
 *
 * - non-null object,
 * - `default: string`,
 * - optional `translations` is a string-valued object.
 *
 * Used to discriminate locale-aware diagnostic params from plain
 * scalars without requiring a per-param schema declaration. The
 * shape is unique enough that collision risk with user-supplied
 * shapes is effectively zero — all platform-emitted diagnostics
 * route through the BE's `LocalizedText: Into<serde_json::Value>`
 * helper, which produces exactly this form.
 */
function isLocalizedText(value: unknown): value is LocalizedText {
  if (value === null || typeof value !== "object") {
    return false;
  }
  const obj = value as Record<string, unknown>;
  if (typeof obj.default !== "string") {
    return false;
  }
  if ("translations" in obj) {
    const tx = obj.translations;
    if (tx === null || typeof tx !== "object" || Array.isArray(tx)) {
      return false;
    }
    for (const v of Object.values(tx as Record<string, unknown>)) {
      if (typeof v !== "string") {
        return false;
      }
    }
  }
  return true;
}

/**
 * Walk a nested message dictionary for the existence of a leaf
 * string at `path`. Returns `false` when any intermediate node is
 * missing or the leaf is not a string. Pure / no allocations
 * beyond the path traversal.
 */
function catalogueHasKey(
  messages: unknown,
  path: readonly string[],
): boolean {
  let cursor: unknown = messages;
  for (const segment of path) {
    if (cursor == null || typeof cursor !== "object") {
      return false;
    }
    cursor = (cursor as Record<string, unknown>)[segment];
  }
  return typeof cursor === "string";
}
