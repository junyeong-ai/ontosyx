"use client";

import type { OntologyIR } from "@/types/api";
import { exportOntology, exportCypher, exportMermaid, exportGraphql, exportOwl, exportShacl, exportTypescript, exportPython } from "@/lib/api";
import { toast } from "sonner";

export type ExportFormat = "json" | "cypher" | "mermaid" | "graphql" | "owl" | "shacl" | "typescript" | "python";

/**
 * Toast copy injected by the caller. Keeping the strings out of this
 * module preserves the React-context-free guarantee — `lib/` runs
 * outside `useTranslations`, so the caller resolves the i18n in
 * its own component scope and threads them in.
 */
export interface SchemaExportToastCopy {
  failedTitle: string;
}

export function downloadAsFile(content: string, filename: string, mimeType: string) {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export async function handleSchemaExport(
  ontology: OntologyIR,
  format: ExportFormat,
  toastCopy: SchemaExportToastCopy,
) {
  const baseName = ontology.name || "ontology";
  try {
    switch (format) {
      case "json": {
        const exported = await exportOntology(ontology);
        downloadAsFile(JSON.stringify(exported, null, 2), `${baseName}.ontology.json`, "application/json");
        break;
      }
      case "cypher": {
        const text = await exportCypher(ontology);
        downloadAsFile(text, `${baseName}.cypher`, "text/plain");
        break;
      }
      case "mermaid": {
        const text = await exportMermaid(ontology);
        downloadAsFile(text, `${baseName}.mermaid.md`, "text/markdown");
        break;
      }
      case "graphql": {
        const text = await exportGraphql(ontology);
        downloadAsFile(text, `${baseName}.graphql`, "text/plain");
        break;
      }
      case "owl": {
        const text = await exportOwl(ontology);
        downloadAsFile(text, `${baseName}.owl.ttl`, "text/turtle");
        break;
      }
      case "shacl": {
        const text = await exportShacl(ontology);
        downloadAsFile(text, `${baseName}.shacl.ttl`, "text/turtle");
        break;
      }
      case "typescript": {
        const text = await exportTypescript(ontology);
        downloadAsFile(text, `${baseName}.types.ts`, "text/typescript");
        break;
      }
      case "python": {
        const text = await exportPython(ontology);
        downloadAsFile(text, `${baseName}_types.py`, "text/x-python");
        break;
      }
    }
  } catch (err) {
    toast.error(toastCopy.failedTitle, {
      description: err instanceof Error ? err.message : String(err),
    });
  }
}
