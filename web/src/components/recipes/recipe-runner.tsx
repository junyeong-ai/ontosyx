"use client";

import { useState, useRef, useCallback } from "react";
import { useTranslations } from "next-intl";
import { X } from "lucide-react";
import type { AnalysisRecipe } from "@/types/api";
import { chatStream } from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { Spinner } from "@/components/ui/spinner";
import { Eyebrow } from "@/components/ui/eyebrow";
import { Heading } from "@/components/ui/heading";
import { FormInput, FormTextarea } from "@/components/ui/form-input";
import { Checkbox } from "@/components/ui/checkbox";

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
  return (
    <label className="block">
      <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
        {name}
        {def.description && (
          <span className="ms-1 font-normal normal-case text-foreground-muted">
            — {def.description}
          </span>
        )}
      </span>
      <FormInput
        type={def.type === "int" || def.type === "float" ? "number" : "text"}
        step={def.type === "int" ? 1 : def.type === "float" ? 0.01 : undefined}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="mt-0.5"
      />
    </label>
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
  const t = useTranslations("settings.recipes.runner");
  const tCommon = useTranslations("common");
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
  const ontologyId = useAppStore((s) => s.ontologyId);

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
      ? t("dataSourceLast")
      : cypherQuery.trim()
        ? t("dataSourceQuery", { query: cypherQuery.trim() })
        : "";

    const message = t("prompt", {
      name: recipe.name,
      params: paramStr,
      dataSource,
    }).trim();

    try {
      await chatStream(
        {
          message,
          ontology,
          ontology_id: ontologyId ?? undefined,
          session_id: sessionId ?? undefined,
        },
        {
          onText(delta) {
            setResultText((prev) => prev + delta);
          },
          onError(error) {
            setResultText((prev) => prev + "\n" + t("errorPrefix", { error }));
          },
        },
        controller.signal,
      );
    } catch {
      // aborted or network error handled by onError
    } finally {
      setIsRunning(false);
    }
  }, [ontology, sessionId, ontologyId, recipe.name, paramEntries, values, useLastResult, cypherQuery, t]);

  const handleCancel = useCallback(() => {
    abortRef.current?.abort();
    setIsRunning(false);
  }, []);

  return (
    <div className="fixed inset-0 z-modal flex items-center justify-center bg-surface-scrim-strong">
      <div className="mx-4 flex max-h-[85vh] w-full max-w-lg flex-col rounded-xl border border-divider bg-surface-base shadow-4">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-divider px-5 py-3">
          <div>
            <Heading level={2} size={6}>
              {recipe.name}
            </Heading>
            <p className="mt-0.5 text-xs text-foreground-muted line-clamp-1">
              {recipe.description}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-foreground-muted hover:bg-surface-inset hover:text-foreground"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 space-y-4 overflow-y-auto px-5 py-4">
          {/* Parameters */}
          {paramEntries.length > 0 && (
            <div className="space-y-2">
              <Eyebrow level={3}>{t("parameters")}</Eyebrow>
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
            <Eyebrow level={3}>{t("dataSource")}</Eyebrow>
            <Checkbox
              checked={useLastResult}
              onChange={(e) => setUseLastResult(e.target.checked)}
              label={t("useLastResult")}
            />
            {!useLastResult && (
              <FormTextarea
                value={cypherQuery}
                onChange={(e) => setCypherQuery(e.target.value)}
                placeholder={t("cypherPlaceholder")}
                rows={3}
                className="font-mono text-xs"
              />
            )}
          </div>

          {/* Result */}
          {resultText && (
            <div>
              <Eyebrow level={3}>{t("result")}</Eyebrow>
              <pre className="mt-1 max-h-60 overflow-auto rounded-md bg-surface-raised p-3 text-xs text-foreground-muted">
                {resultText}
              </pre>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-2 border-t border-divider px-5 py-3">
          {isRunning ? (
            <>
              <Spinner size="sm" />
              <button
                type="button"
                onClick={handleCancel}
                className="rounded-md px-3 py-1.5 text-xs font-medium text-danger-foreground hover:bg-danger-surface"
              >
                {tCommon("cancel")}
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                onClick={onClose}
                className="rounded-md px-3 py-1.5 text-xs font-medium text-foreground hover:bg-surface-inset"
              >
                {tCommon("close")}
              </button>
              <button type="button"
                onClick={handleRun}
                disabled={!ontology}
                className="rounded-md bg-brand-solid px-4 py-1.5 text-xs font-medium text-foreground-onbrand hover:bg-brand-solid disabled:opacity-50"
              >
                {t("run")}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
