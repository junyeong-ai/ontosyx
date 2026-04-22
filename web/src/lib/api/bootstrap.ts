// Phase 4.1 — Bootstrap wizard backend calls.
//
// The wizard persists free-form `glossaryDraft` text in localStorage.
// On Finish, this helper parses each non-empty line into a
// `{ term, description?, aliases }` row and forwards the batch to
// `POST /api/bootstrap/seed-glossary`, which commits a fresh
// ontology containing those terms. The flow intentionally skips
// the usual source-analysis pipeline — the bootstrap ontology is a
// "here's what the domain experts already wrote down" artefact the
// workbench later refines.

import { request } from "./client";

export interface SeedGlossaryTerm {
  term: string;
  description?: string;
  aliases: string[];
}

export interface SeedGlossaryRequest {
  name: string;
  description?: string;
  terms: SeedGlossaryTerm[];
}

export interface SeedGlossaryResponse {
  ontology_id: string;
  version_id: string;
  committed_terms: number;
}

export async function seedBootstrapGlossary(
  body: SeedGlossaryRequest,
): Promise<SeedGlossaryResponse> {
  return request<SeedGlossaryResponse>("/bootstrap/seed-glossary", {
    method: "POST",
    body: JSON.stringify(body),
  });
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
 * we keep the client-side parser minimal so the flow is easy to
 * reason about in a test.
 */
export function parseGlossaryDraft(raw: string): SeedGlossaryTerm[] {
  const out: SeedGlossaryTerm[] = [];
  for (const rawLine of raw.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line) continue;

    // `term | aliases` or `term: description | aliases` — split on
    // the first `|` so descriptions may contain pipes if followed
    // by an escape-like context (they won't, in practice).
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
