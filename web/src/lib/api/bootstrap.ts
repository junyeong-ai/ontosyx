// Bootstrap wizard parser.
//
// The module's sole export is `parseGlossaryDraft`: a tolerant
// textarea-to-rows parser for the wizard's free-form `glossaryDraft`
// field. `6-validate` consumes the parsed rows and maps each into an
// `OntologyEditOp::CreateGlossaryTerm` op before POSTing to the
// unified `/api/ontology` creation endpoint, so no network call
// lives here — network I/O is concentrated in `api/ontology.ts`.

/**
 * One parsed glossary row from the wizard's free-form textarea.
 * Decoupled from the backend `GlossaryTermDef` shape so the parser
 * has a minimal surface — the `6-validate` step widens this into a
 * full `CreateGlossaryTerm` op before POSTing.
 */
export interface GlossaryTermDraft {
  term: string;
  description?: string;
  aliases: string[];
}

/**
 * Parse a free-form textarea into glossary rows.
 *
 * Supported line shapes (tried in order):
 *   - `term: description | alias1, alias2`
 *   - `term: description`
 *   - `term | alias1, alias2`
 *   - `term`
 *
 * Empty lines and lines whose `term` portion is whitespace-only are
 * dropped. Duplicates are resolved server-side (case-insensitive);
 * the client parser stays minimal so it's easy to reason about in a
 * test.
 */
export function parseGlossaryDraft(raw: string): GlossaryTermDraft[] {
  const out: GlossaryTermDraft[] = [];
  for (const rawLine of raw.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line) continue;

    // `term | aliases` or `term: description | aliases` — split on
    // the first `|` so descriptions may contain pipes if followed by
    // an escape-like context (they won't, in practice).
    let pipePart: string | null = null;
    let mainPart = line;
    const pipeIdx = line.indexOf("|");
    if (pipeIdx !== -1) {
      mainPart = line.slice(0, pipeIdx).trim();
      pipePart = line.slice(pipeIdx + 1).trim();
    }

    let term = mainPart;
    let description: string | undefined;
    const colonIdx = mainPart.indexOf(":");
    if (colonIdx !== -1) {
      term = mainPart.slice(0, colonIdx).trim();
      const maybeDescription = mainPart.slice(colonIdx + 1).trim();
      if (maybeDescription) description = maybeDescription;
    }
    if (!term) continue;

    const aliases = pipePart
      ? pipePart
          .split(",")
          .map((a) => a.trim())
          .filter((a) => a.length > 0)
      : [];

    out.push({ term, description, aliases });
  }
  return out;
}
