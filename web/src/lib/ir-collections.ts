/**
 * Helpers for the optional `Vec<T>` collections in `OntologyIR` and its
 * sub-types. The wire shape carries `#[serde(default,
 * skip_serializing_if = "Vec::is_empty")]` on most non-core slices, so
 * a deserialised IR may surface `edge_types`, `properties`,
 * `interfaces`, etc. as `undefined` when the backend omitted them.
 *
 * Call-sites that map / filter / iterate these collections funnel
 * through {@link arr} so the optionality is handled in one place
 * rather than scattered `?? []` everywhere.
 *
 * Long-term: when a collection migrates to *always required* in the
 * wire shape, the helper stays correct (the input type just narrows)
 * and the call-sites keep compiling.
 */
export function arr<T>(value: readonly T[] | null | undefined): readonly T[] {
  return value ?? [];
}

/**
 * Mutable variant. Returns the value verbatim when present so callers
 * that mutate (e.g. push into) keep the underlying array; falls back
 * to a fresh empty array only when the input is missing.
 */
export function arrMut<T>(value: T[] | null | undefined): T[] {
  return value ?? [];
}
