/**
 * `LocalizedText` resolver — the only sanctioned way to read a
 * displayable string out of `LocalizedText { default, translations }`.
 *
 * Rust counterpart: `ox_core::i18n::LocalizedText::resolve`. Both
 * sides walk the chain in order; the first translation that exists
 * *and* is non-empty wins. If no translation matches, the canonical
 * `default` is returned (which itself may be empty for optional
 * fields — callers wanting "present or null" use {@link localizePresent}).
 *
 * The chain is `workspaces.locale_fallback` (BCP 47 tags, e.g.
 * `["ko", "en"]`). Caller threads the workspace's chain through
 * from `useWorkspace()` or the request context.
 *
 * @example
 *   const label = localize(node.display_name, ["ko", "en"]);
 */
import type { LocalizedText } from "@/types/ontology";

/**
 * Read the canonical default text without localising. Use for
 * cases where a downstream API takes a plain string and the
 * caller has no locale context (test fixtures, low-level
 * comparisons, etc). Render code should prefer {@link localize}.
 */
export function defaultText(text: LocalizedText | null | undefined): string {
  return text?.default ?? "";
}

/**
 * Static fallback when a workspace's `locale_fallback` chain isn't
 * available at the call-site. Mirrors the `workspaces.locale_fallback`
 * column default. Surfaces that gain a workspace context should
 * thread the actual chain in instead of importing this constant.
 */
export const DEFAULT_LOCALE_CHAIN: readonly string[] = ["ko", "en"];

export function localize(text: LocalizedText, chain: readonly string[]): string {
  for (const tag of chain) {
    const candidate = text.translations?.[tag];
    if (candidate && candidate.length > 0) {
      return candidate;
    }
  }
  return text.default;
}

/**
 * Same walk as {@link localize} but returns `null` instead of an
 * empty string when nothing was found. Mirrors the Rust
 * `LocalizedText::present` accessor.
 */
export function localizePresent(
  text: LocalizedText,
  chain: readonly string[],
): string | null {
  const resolved = localize(text, chain);
  return resolved.length > 0 ? resolved : null;
}

/**
 * Pick the displayable label for a node/edge/property where the
 * graph identifier acts as a guaranteed-present fallback (Cypher
 * labels are always populated). Avoids `<UNNAMED>` placeholders at
 * the rendering layer.
 */
export function localizeWithFallback(
  text: LocalizedText,
  chain: readonly string[],
  fallback: string,
): string {
  const resolved = localize(text, chain);
  return resolved.length > 0 ? resolved : fallback;
}
