// ---------------------------------------------------------------------------
// filter-value.ts — shared parse/stringify for property filter values
// ---------------------------------------------------------------------------
//
// Used by both the query-builder's IR builder (canvas → QueryIR) and the
// saved-pattern serializer (canvas ↔ PatternIR wire shape). Keeping the
// two halves in one module prevents the parse/stringify rules from
// drifting when one side adds a literal type the other forgets.
//
// Value grammar (intentionally small):
//   - Integers / floats: `1`, `-5`, `3.14`, `-0.5`     → number
//   - Booleans:          `true` / `false`              → boolean
//   - Quoted strings:    `'x'`, `"x"`                  → string (dequoted)
//   - Anything else:                                   → string (as-is)

/**
 * Parse a raw filter-value string into its typed JSON equivalent.
 *
 * Mirrors the grammar the builder exposes in its filter-editor UI:
 * numeric literals, the two booleans, single/double-quoted strings,
 * and everything else as an unquoted string.
 */
export function parseFilterValue(raw: string): unknown {
  if (/^-?\d+(\.\d+)?$/.test(raw)) return Number(raw);
  if (raw === "true") return true;
  if (raw === "false") return false;
  if (
    (raw.startsWith('"') && raw.endsWith('"')) ||
    (raw.startsWith("'") && raw.endsWith("'"))
  ) {
    return raw.slice(1, -1);
  }
  return raw;
}

/**
 * Render a typed JSON value back to the builder's editor string.
 *
 * The UI uses the same textarea for every literal type, so `true`
 * round-trips as `"true"` (not `"'true'"`), and a numeric value
 * round-trips as its decimal string. Returns `""` for null /
 * undefined so the editor shows an empty field rather than the
 * literal strings `"null"` / `"undefined"`.
 */
export function stringifyFilterValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  return String(value);
}
