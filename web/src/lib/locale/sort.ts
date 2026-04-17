/**
 * Korean-aware sorting via `Intl.Collator('ko-KR')`.
 *
 * Native `String.prototype.localeCompare()` ordering varies by engine for
 * Hangul (한글) — specifically around jamo ordering vs. syllable blocks and
 * ASCII vs. Korean mixing. Using an explicit collator gives deterministic,
 * dictionary-style ordering regardless of host locale.
 *
 * Variant sensitivity keeps accents/jamo distinctions intact (e.g. "가" vs
 * "각"), which is the default users expect when scanning lists.
 *
 * Example:
 * ```ts
 * sortKorean(["홍길동", "김철수", "이영희"], (x) => x);
 * // → ["김철수", "이영희", "홍길동"]
 * ```
 */

const collator = new Intl.Collator("ko-KR", {
  sensitivity: "variant",
  usage: "sort",
  numeric: true,
});

export function sortKorean<T>(items: readonly T[], accessor: (item: T) => string): T[] {
  return [...items].sort((a, b) => collator.compare(accessor(a), accessor(b)));
}

/**
 * Compare two strings using the shared Korean collator.
 * Prefer this over `String.prototype.localeCompare` when sorting user-facing
 * labels, names, or any Hangul-containing strings.
 */
export function compareKorean(a: string, b: string): number {
  return collator.compare(a, b);
}
