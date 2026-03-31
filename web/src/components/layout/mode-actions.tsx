"use client";

import { useRef, useState } from "react";
import { useAppStore } from "@/lib/store";
import { Button } from "@/components/ui/button";
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";
import { Tooltip } from "@/components/ui/tooltip";
import { Spinner } from "@/components/ui/spinner";
import { HugeiconsIcon } from "@hugeicons/react";
import { Upload04Icon, Download04Icon } from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";
import type { OntologyIR } from "@/types/api";
import { normalizeOntology, importOwl, auditGraph } from "@/lib/api";
import type { GraphAuditReport } from "@/lib/api/ontology";
import { handleSchemaExport } from "@/lib/export-utils";
import type { ExportFormat } from "@/lib/export-utils";

// ---------------------------------------------------------------------------
// ModeActions — mode-specific action buttons on the right side of the header
// ---------------------------------------------------------------------------

export function ModeActions() {
  const workspaceMode = useAppStore((s) => s.workspaceMode);

  if (workspaceMode === "design") {
    return <DesignActions />;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Design mode: Import + Export
// ---------------------------------------------------------------------------

function DesignActions() {
  const ontology = useAppStore((s) => s.ontology);
  const setOntology = useAppStore((s) => s.setOntology);
  const setActiveProject = useAppStore((s) => s.setActiveProject);
  const activeProject = useAppStore((s) => s.activeProject);
  const guardPendingEdits = useGuardPendingEdits();

  const fileRef = useRef<HTMLInputElement>(null);
  const [importing, setImporting] = useState(false);
  const [exportMenuOpen, setExportMenuOpen] = useState(false);
  const [auditing, setAuditing] = useState(false);

  const handleAudit = async () => {
    const ontologyId = activeProject?.saved_ontology_id;
    if (!ontologyId) {
      toast.error("Save the ontology first to run an audit");
      return;
    }
    setAuditing(true);
    try {
      const report: GraphAuditReport = await auditGraph(ontologyId);
      const matched = report.matched_nodes.length + report.matched_edges.length;
      const orphaned = report.orphan_graph_nodes.length + report.orphan_graph_edges.length;
      const missing = report.missing_graph_nodes.length + report.missing_graph_edges.length;
      toast.success(`Graph Audit: ${report.sync_percentage}% synced`, {
        description: `Matched: ${matched} | Orphaned: ${orphaned} | Missing: ${missing}`,
        duration: 8000,
      });
    } catch (err) {
      toast.error("Audit failed", {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setAuditing(false);
    }
  };

  const handleFileImport = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    if (!(await guardPendingEdits("Import Ontology"))) {
      if (fileRef.current) fileRef.current.value = "";
      return;
    }
    setImporting(true);
    try {
      const text = await file.text();
      const ext = file.name.split(".").pop()?.toLowerCase();
      let imported: OntologyIR;

      if (ext === "ttl" || ext === "owl") {
        imported = await importOwl(text);
      } else {
        const parsed = JSON.parse(text) as Record<string, unknown>;
        if (
          !Array.isArray(parsed.node_types) ||
          !Array.isArray(parsed.edge_types)
        ) {
          throw new Error("Invalid ontology: missing node_types/edge_types");
        }
        const result = await normalizeOntology(parsed);
        imported = result.ontology;
        if (result.warnings.length > 0) {
          toast.warning("Import completed with warnings", {
            description: result.warnings.map((w) => w.message).join("; "),
          });
        }
      }

      setActiveProject(null);
      setOntology(imported);
      toast.success("Ontology imported", {
        description: `${imported.node_types.length}N, ${imported.edge_types.length}E`,
      });
    } catch (err) {
      toast.error("Import failed", {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setImporting(false);
    }
    if (fileRef.current) fileRef.current.value = "";
  };

  const handleExportFormat = async (format: ExportFormat) => {
    if (!ontology) return;
    setExportMenuOpen(false);
    await handleSchemaExport(ontology, format);
  };

  return (
    <>
      <input ref={fileRef} type="file" accept=".json,.ttl,.owl" className="hidden" onChange={handleFileImport} />
      <Tooltip content="Import Ontology (JSON, OWL/Turtle)">
        <Button variant="ghost" size="icon-sm" onClick={() => fileRef.current?.click()} disabled={importing} aria-label="Import Ontology">
          {importing ? <Spinner size="xs" /> : <HugeiconsIcon icon={Upload04Icon} className="h-3.5 w-3.5" size="100%" />}
        </Button>
      </Tooltip>
      {ontology && (
        <Tooltip content="Audit Graph">
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={handleAudit}
            disabled={auditing || !activeProject?.saved_ontology_id}
            aria-label="Audit Graph"
          >
            {auditing ? (
              <Spinner size="xs" />
            ) : (
              <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" className="h-3.5 w-3.5">
                <path strokeLinecap="round" strokeLinejoin="round" d="M9 12.75 11.25 15 15 9.75m-3-7.036A11.959 11.959 0 0 1 3.598 6 11.99 11.99 0 0 0 3 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285Z" />
              </svg>
            )}
          </Button>
        </Tooltip>
      )}
      {ontology && (
        <Popover open={exportMenuOpen} onOpenChange={setExportMenuOpen}>
          <PopoverTrigger aria-label="Export Ontology" className="inline-flex h-7 w-7 items-center justify-center rounded-md text-zinc-500 transition-colors hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-300">
            <HugeiconsIcon icon={Download04Icon} className="h-3.5 w-3.5" size="100%" />
          </PopoverTrigger>
          <PopoverContent className="z-50 w-48 rounded-lg border border-zinc-200 bg-white p-1 shadow-lg data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0 transition-all dark:border-zinc-700 dark:bg-zinc-900">
            <button
              onClick={() => handleExportFormat("json")}
              className="flex w-full items-center rounded-md px-3 py-1.5 text-left text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              JSON
            </button>
            <button
              onClick={() => handleExportFormat("cypher")}
              className="flex w-full items-center rounded-md px-3 py-1.5 text-left text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              Cypher DDL
            </button>
            <button
              onClick={() => handleExportFormat("mermaid")}
              className="flex w-full items-center rounded-md px-3 py-1.5 text-left text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              Mermaid Diagram
            </button>
            <button
              onClick={() => handleExportFormat("graphql")}
              className="flex w-full items-center rounded-md px-3 py-1.5 text-left text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              GraphQL Schema
            </button>
            <button
              onClick={() => handleExportFormat("owl")}
              className="flex w-full items-center rounded-md px-3 py-1.5 text-left text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              OWL/Turtle
            </button>
            <button
              onClick={() => handleExportFormat("shacl")}
              className="flex w-full items-center rounded-md px-3 py-1.5 text-left text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              SHACL Shapes
            </button>
            <div className="my-1 border-t border-zinc-100 dark:border-zinc-800" />
            <button
              onClick={() => handleExportFormat("typescript")}
              className="flex w-full items-center rounded-md px-3 py-1.5 text-left text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              TypeScript Types
            </button>
            <button
              onClick={() => handleExportFormat("python")}
              className="flex w-full items-center rounded-md px-3 py-1.5 text-left text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              Python Dataclasses
            </button>
          </PopoverContent>
        </Popover>
      )}
    </>
  );
}
