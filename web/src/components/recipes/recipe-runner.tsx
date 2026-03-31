"use client";

import { useState, useRef, useCallback } from "react";
import type { AnalysisRecipe } from "@/types/api";
import { chatStream } from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { Spinner } from "@/components/ui/spinner";

// ---------------------------------------------------------------------------
// Parameter type → form field
// ---------------------------------------------------------------------------

interface ParamDef {
  type: string;
  default: unknown;
  description?: string;
}

function ParamField({
  name,
  def,
  value,
  onChange,
}: {
  name: string;
  def: ParamDef;
  value: string;
  onChange: (v: string) => void;
}) {
  const inputCls =
    "w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200";

  return (
    <div>
      <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
        {name}
        {def.description && (
          <span className="ml-1 font-normal normal-case text-zinc-400">
            — {def.description}
          </span>
        )}
      </label>
      {def.type === "int" ? (
        <input
          type="number"
          step={1}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className={inputCls}
        />
      ) : def.type === "float" ? (
        <input
          type="number"
          step={0.01}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className={inputCls}
        />
      ) : (
        <input
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className={inputCls}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// RecipeRunner modal
// ---------------------------------------------------------------------------

interface RecipeRunnerProps {
  recipe: AnalysisRecipe;
  onClose: () => void;
}

export function RecipeRunner({ recipe, onClose }: RecipeRunnerProps) {
  const params = recipe.parameters as Record<string, ParamDef>;
  const paramEntries = Object.entries(params);

  // Pre-fill parameter defaults
  const [values, setValues] = useState<Record<string, string>>(() => {
    const init: Record<string, string> = {};
    for (const [k, v] of paramEntries) {
      init[k] = String(v.default ?? "");
    }
    return init;
  });

  const [cypherQuery, setCypherQuery] = useState("");
  const [useLastResult, setUseLastResult] = useState(false);
  const [resultText, setResultText] = useState("");
  const [isRunning, setIsRunning] = useState(false);
  const abortRef = useRef<AbortController | null>(null);

  const ontology = useAppStore((s) => s.ontology);
  const sessionId = useAppStore((s) => s.sessionId);
  const savedOntologyId = useAppStore((s) => s.savedOntologyId);

  const handleParamChange = useCallback((name: string, val: string) => {
    setValues((prev) => ({ ...prev, [name]: val }));
  }, []);

  const handleRun = useCallback(async () => {
    if (!ontology) return;
    setIsRunning(true);
    setResultText("");

    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    // Build the prompt that the agent will interpret
    const paramStr = paramEntries
      .map(([k]) => `${k}=${values[k]}`)
      .join(", ");
    const dataSource = useLastResult
      ? "Use the last query result as data source."
      : cypherQuery.trim()
        ? `Data source query: ${cypherQuery.trim()}`
        : "";

    const message = `Run analysis recipe "${recipe.name}" with parameters: ${paramStr}. ${dataSource}`.trim();

    try {
      await chatStream(
        {
          message,
          ontology,
          saved_ontology_id: savedOntologyId ?? undefined,
          session_id: sessionId ?? undefined,
        },
        {
          onText(delta) {
            setResultText((prev) => prev + delta);
          },
          onError(error) {
            setResultText((prev) => prev + `\n[Error] ${error}`);
          },
        },
        controller.signal,
      );
    } catch {
      // aborted or network error handled by onError
    } finally {
      setIsRunning(false);
    }
  }, [ontology, sessionId, savedOntologyId, recipe.name, paramEntries, values, useLastResult, cypherQuery]);

  const handleCancel = useCallback(() => {
    abortRef.current?.abort();
    setIsRunning(false);
  }, []);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="mx-4 flex max-h-[85vh] w-full max-w-lg flex-col rounded-xl border border-zinc-200 bg-white shadow-xl dark:border-zinc-700 dark:bg-zinc-900">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-zinc-200 px-5 py-3 dark:border-zinc-700">
          <div>
            <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">
              {recipe.name}
            </h2>
            <p className="mt-0.5 text-xs text-zinc-500 line-clamp-1">
              {recipe.description}
            </p>
          </div>
          <button
            onClick={onClose}
            className="rounded-md p-1 text-zinc-400 hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800"
          >
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" className="h-4 w-4">
              <path d="M6.28 5.22a.75.75 0 0 0-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 1 0 1.06 1.06L10 11.06l3.72 3.72a.75.75 0 1 0 1.06-1.06L11.06 10l3.72-3.72a.75.75 0 0 0-1.06-1.06L10 8.94 6.28 5.22Z" />
            </svg>
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 space-y-4 overflow-y-auto px-5 py-4">
          {/* Parameters */}
          {paramEntries.length > 0 && (
            <div className="space-y-2">
              <h3 className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                Parameters
              </h3>
              {paramEntries.map(([name, def]) => (
                <ParamField
                  key={name}
                  name={name}
                  def={def}
                  value={values[name] ?? ""}
                  onChange={(v) => handleParamChange(name, v)}
                />
              ))}
            </div>
          )}

          {/* Data Source */}
          <div className="space-y-2">
            <h3 className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Data Source
            </h3>
            <label className="flex items-center gap-2 text-xs text-zinc-600 dark:text-zinc-400">
              <input
                type="checkbox"
                checked={useLastResult}
                onChange={(e) => setUseLastResult(e.target.checked)}
                className="rounded border-zinc-300 text-emerald-600 focus:ring-emerald-500"
              />
              Use last query result
            </label>
            {!useLastResult && (
              <textarea
                value={cypherQuery}
                onChange={(e) => setCypherQuery(e.target.value)}
                placeholder="MATCH (n)-[r]->(m) RETURN n, r, m LIMIT 100"
                rows={3}
                className="w-full rounded-md border border-zinc-200 bg-white px-3 py-2 font-mono text-xs dark:border-zinc-700 dark:bg-zinc-950 dark:text-zinc-200"
              />
            )}
          </div>

          {/* Result */}
          {resultText && (
            <div>
              <h3 className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                Result
              </h3>
              <pre className="mt-1 max-h-60 overflow-auto rounded-md bg-zinc-50 p-3 text-xs text-zinc-700 dark:bg-zinc-950 dark:text-zinc-300">
                {resultText}
              </pre>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-2 border-t border-zinc-200 px-5 py-3 dark:border-zinc-700">
          {isRunning ? (
            <>
              <Spinner size="sm" />
              <button
                onClick={handleCancel}
                className="rounded-md px-3 py-1.5 text-xs font-medium text-red-600 hover:bg-red-50 dark:hover:bg-red-950"
              >
                Cancel
              </button>
            </>
          ) : (
            <>
              <button
                onClick={onClose}
                className="rounded-md px-3 py-1.5 text-xs font-medium text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
              >
                Close
              </button>
              <button
                onClick={handleRun}
                disabled={!ontology}
                className="rounded-md bg-emerald-600 px-4 py-1.5 text-xs font-medium text-white hover:bg-emerald-700 disabled:opacity-50"
              >
                Run Analysis
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
