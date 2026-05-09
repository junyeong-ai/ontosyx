/**
 * RFC 4180 CSV serialiser.
 *
 * Header + body rows; every field is RFC 4180-quoted when it
 * contains any of `, " \r \n` so the output round-trips through
 * stdlib CSV parsers (Excel, LibreOffice, pandas, csvkit).
 *
 * Pure function — no DOM access, no Blob materialisation. The
 * caller wraps the returned string in a Blob and triggers
 * download via the standard anchor pattern (see
 * `triggerCsvDownload`).
 */
export type CsvCell = string | number;

export function toCsv(
  header: readonly string[],
  rows: readonly (readonly CsvCell[])[],
): string {
  const lines: string[] = [];
  lines.push(header.map(escapeField).join(","));
  for (const row of rows) {
    lines.push(row.map((cell) => escapeField(String(cell))).join(","));
  }
  // RFC 4180 §2.4 — records are terminated by CRLF. Excel
  // tolerates LF-only on macOS but Windows Excel renders one
  // long line; CRLF is the safer default.
  return lines.join("\r\n") + "\r\n";
}

function escapeField(field: string): string {
  if (/[",\r\n]/.test(field)) {
    return `"${field.replace(/"/g, '""')}"`;
  }
  return field;
}

/**
 * Trigger a browser download for the given CSV string. Browser-
 * only; callers in Node tests should use `toCsv` directly and
 * stub this function.
 */
export function triggerCsvDownload(filename: string, csv: string): void {
  if (typeof window === "undefined") return;
  const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.rel = "noopener";
  document.body.appendChild(anchor);
  anchor.click();
  // Defer revoke so the click finishes before the URL goes
  // away. `setTimeout(0)` is enough across every browser
  // engine that ships with a synchronous click handler.
  setTimeout(() => {
    URL.revokeObjectURL(url);
    anchor.remove();
  }, 0);
}
