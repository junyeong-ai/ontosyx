"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Refresh01Icon, Add01Icon } from "@hugeicons/core-free-icons";
import { Spinner } from "@/components/ui/spinner";
import { Button } from "@/components/ui/button";
import { FormInput } from "@/components/ui/form-input";
import { FormTextarea } from "@/components/ui/form-textarea";
import { SourceImportPanel } from "@/components/workbench/source-import-panel";
import { cn } from "@/lib/cn";
import type { DesignSource, ProjectSource } from "@/types/api";

// ---------------------------------------------------------------------------
// Reanalyze form
// ---------------------------------------------------------------------------

export function ReanalyzeForm({
  sourceType,
  connectionString,
  setConnectionString,
  schemaName,
  setSchemaName,
  sampleData,
  setSampleData,
  repoPath,
  setRepoPath,
  loading,
  repoUrl,
  setRepoUrl,
  onSubmit,
  modeledOnly,
  setModeledOnly,
  modeledTablesAvailable,
}: {
  sourceType: string;
  connectionString: string;
  setConnectionString: (v: string) => void;
  schemaName: string;
  setSchemaName: (v: string) => void;
  sampleData: string;
  setSampleData: (v: string) => void;
  repoPath: string;
  setRepoPath: (v: string) => void;
  repoUrl: string;
  setRepoUrl: (v: string) => void;
  loading: boolean;
  onSubmit: () => void;
  modeledOnly: boolean;
  setModeledOnly: (v: boolean) => void;
  /** Number of tables in `analysis_scope.included` — when 0 the
   * "modeled only" checkbox is hidden (the action would 400). */
  modeledTablesAvailable: number;
}) {
  const t = useTranslations("workbench.bottomPanel.workflowForms");
  const isDisabled = loading || (() => {
    if (sourceType === "postgresql") return !connectionString.trim();
    if (sourceType === "code_repository") return !repoUrl.trim();
    return !sampleData.trim();
  })();

  return (
    <div className="space-y-2 rounded-lg border border-divider bg-surface-raised p-3">
      {sourceType === "postgresql" ? (
        <>
          <FormInput
            type="text"
            placeholder={t("postgresPlaceholder")}
            value={connectionString}
            onChange={(e) => setConnectionString(e.target.value)}
            className="font-mono"
          />
          <FormInput
            type="text"
            placeholder={t("schemaPlaceholder")}
            value={schemaName}
            onChange={(e) => setSchemaName(e.target.value)}
          />
        </>
      ) : sourceType === "code_repository" ? (
        <FormInput
          type="text"
          placeholder={t("repoUrlPlaceholder")}
          value={repoUrl}
          onChange={(e) => setRepoUrl(e.target.value)}
          className="font-mono"
        />
      ) : (
        <FormTextarea
          rows={4}
          placeholder={t("dataPlaceholder")}
          value={sampleData}
          onChange={(e) => setSampleData(e.target.value)}
          className="font-mono text-xs"
        />
      )}
      {sourceType !== "text" && sourceType !== "code_repository" && (
        <FormInput
          type="text"
          placeholder={t("repoPathPlaceholder")}
          value={repoPath}
          onChange={(e) => setRepoPath(e.target.value)}
        />
      )}
      {modeledTablesAvailable > 0 && sourceType !== "code_repository" && (
        <label className="flex cursor-pointer items-center gap-2 rounded border border-divider bg-surface-base px-2 py-1.5 text-[11px] hover:bg-surface-raised dark:hover:bg-surface-base/50">
          <input
            type="checkbox"
            checked={modeledOnly}
            onChange={(e) => setModeledOnly(e.target.checked)}
            className="h-3 w-3"
          />
          <span className="flex-1">
            {t("modeledOnlyLabel", { count: modeledTablesAvailable })}
          </span>
        </label>
      )}
      <Button
        size="sm"
        onClick={onSubmit}
        disabled={isDisabled}
        className="w-full text-xs"
      >
        {loading ? (
          <Spinner size="xs" className="mr-1.5" />
        ) : (
          <HugeiconsIcon icon={Refresh01Icon} className="mr-1.5 h-3 w-3" size="100%" />
        )}
        {modeledOnly ? t("reanalyzeModeled") : t("reanalyze")}
      </Button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Extend source form
// ---------------------------------------------------------------------------

// `as const` is load-bearing here: without it TypeScript widens
// this to `DesignSource["type"][]` and the i18n auditor / type
// guards below can't prove `snowflake` / `bigquery` can't show up
// at runtime. The SSoT for which sources the extend form actually
// renders is this tuple — keep it narrow.
export const SOURCE_TYPE_OPTIONS = [
  "text",
  "csv",
  "json",
  "duckdb",
  "code_repository",
  "postgresql",
  "mysql",
  "mongodb",
] as const satisfies readonly DesignSource["type"][];

// `ExtendSourceType` is now a precise 8-variant union. The type
// guard narrows `DesignSource["type"]` values that flow in from
// the wider wire schema onto this subset for label lookup.
type ExtendSourceType = (typeof SOURCE_TYPE_OPTIONS)[number];
function isExtendSourceType(s: DesignSource["type"]): s is ExtendSourceType {
  return (SOURCE_TYPE_OPTIONS as readonly string[]).includes(s);
}

interface ExtendFormSnapshot {
  sourceType: DesignSource["type"];
  connectionString: string;
  schemaName: string;
  database: string;
  sampleData: string;
  repoUrl: string;
  duckdbFilePath?: string;
}

/**
 * Translate the extend form's flat fields into the `ProjectSource`
 * wire shape, or `null` when the inputs aren't yet sufficient to
 * call the source-preview endpoint. Single source of truth for both
 * the panel preview (workflow-forms) and the submit path
 * (enhance-actions).
 */
export function extendSourceFromForm(
  s: ExtendFormSnapshot,
): ProjectSource | null {
  const conn = s.connectionString.trim();
  switch (s.sourceType) {
    case "postgresql":
      if (!conn) return null;
      return {
        type: "postgresql",
        connection_string: conn,
        schema: s.schemaName.trim() || "public",
      };
    case "mysql": {
      const db = s.database.trim();
      if (!conn || !db) return null;
      return { type: "mysql", connection_string: conn, schema: db };
    }
    case "mongodb": {
      const db = s.database.trim();
      if (!conn || !db) return null;
      return { type: "mongodb", connection_string: conn, database: db };
    }
    case "duckdb": {
      const fp = (s.duckdbFilePath ?? "").trim();
      if (!fp) return null;
      return { type: "duckdb", file_path: fp };
    }
    case "csv":
    case "json": {
      const data = s.sampleData.trim();
      if (!data) return null;
      return { type: s.sourceType, data };
    }
    default:
      // Text / CodeRepository / Snowflake / BigQuery — preview is
      // either trivial (Text), unsupported, or needs structured
      // input the form doesn't collect.
      return null;
  }
}

export function ExtendSourceForm({
  sourceType,
  setSourceType,
  connectionString,
  setConnectionString,
  schemaName,
  setSchemaName,
  database,
  setDatabase,
  sampleData,
  setSampleData,
  repoUrl,
  setRepoUrl,
  duckdbFilePath,
  setDuckdbFilePath,
  importValue,
  setImportValue,
  loading,
  onSubmit,
}: {
  sourceType: DesignSource["type"];
  setSourceType: (v: DesignSource["type"]) => void;
  connectionString: string;
  setConnectionString: (v: string) => void;
  schemaName: string;
  setSchemaName: (v: string) => void;
  database: string;
  setDatabase: (v: string) => void;
  sampleData: string;
  setSampleData: (v: string) => void;
  repoUrl: string;
  setRepoUrl: (v: string) => void;
  duckdbFilePath?: string;
  setDuckdbFilePath?: (v: string) => void;
  importValue: import("@/components/workbench/source-import-panel").SourceImportValue;
  setImportValue: (v: import("@/components/workbench/source-import-panel").SourceImportValue) => void;
  loading: boolean;
  onSubmit: () => void;
}) {
  const t = useTranslations("workbench.bottomPanel.workflowForms");
  const sourceTypes = useMemo(
    () =>
      SOURCE_TYPE_OPTIONS.map((value) => ({
        value,
        label: isExtendSourceType(value)
          ? t(`sourceTypeLabels.${value}`)
          : value,
      })),
    [t],
  );

  return (
    <div className="space-y-2 rounded-lg border border-info-border bg-info-surface/50 p-3 dark:border-info-border">
      <h4 className="text-xs font-semibold text-info-foreground">
        {t("newSource")}
      </h4>

      {/* Source type selector */}
      <div className="flex gap-1">
        {sourceTypes.map((opt) => (
          <button
            key={opt.value}
            onClick={() => setSourceType(opt.value)}
            className={cn(
              "rounded px-2 py-0.5 text-2xs font-medium transition-colors",
              sourceType === opt.value
                ? "bg-info-foreground text-white dark:bg-info-foreground"
                : "bg-surface-inset text-foreground hover:bg-surface-inset dark:text-muted-foreground dark:hover:bg-surface-base",
            )}
          >
            {opt.label}
          </button>
        ))}
      </div>
      {sourceType === "postgresql" || sourceType === "mysql" ? (
        <>
          <FormInput
            type="text"
            placeholder={sourceType === "postgresql" ? t("postgresPlaceholder") : t("mysqlPlaceholder")}
            value={connectionString}
            onChange={(e) => setConnectionString(e.target.value)}
            className="font-mono"
          />
          <FormInput
            type="text"
            placeholder={sourceType === "postgresql" ? t("schemaPlaceholder") : t("dbNamePlaceholder")}
            value={sourceType === "postgresql" ? schemaName : database}
            onChange={(e) => sourceType === "postgresql"
              ? setSchemaName(e.target.value)
              : setDatabase(e.target.value)}
          />
          <SourceImportPanel
            source={extendSourceFromForm({
              sourceType,
              connectionString,
              schemaName,
              database,
              sampleData,
              repoUrl,
              duckdbFilePath,
            })}
            value={importValue}
            onChange={setImportValue}
          />
        </>
      ) : sourceType === "mongodb" ? (
        <>
          <FormInput
            type="text"
            placeholder={t("mongoPlaceholder")}
            value={connectionString}
            onChange={(e) => setConnectionString(e.target.value)}
            className="font-mono"
          />
          <FormInput
            type="text"
            placeholder={t("dbNamePlaceholder")}
            value={database}
            onChange={(e) => setDatabase(e.target.value)}
          />
          <SourceImportPanel
            source={extendSourceFromForm({
              sourceType,
              connectionString,
              schemaName,
              database,
              sampleData,
              repoUrl,
              duckdbFilePath,
            })}
            value={importValue}
            onChange={setImportValue}
          />
        </>
      ) : sourceType === "duckdb" ? (
        <FormInput
          type="text"
          placeholder={t("duckdbFilePlaceholder")}
          value={duckdbFilePath ?? ""}
          onChange={(e) => setDuckdbFilePath?.(e.target.value)}
          className="font-mono"
        />
      ) : sourceType === "code_repository" ? (
        <FormInput
          type="text"
          placeholder={t("repoUrlPlaceholder")}
          value={repoUrl}
          onChange={(e) => setRepoUrl(e.target.value)}
          className="font-mono"
        />
      ) : (
        <FormTextarea
          rows={4}
          placeholder={t("dataPlaceholder")}
          value={sampleData}
          onChange={(e) => setSampleData(e.target.value)}
          className="font-mono text-xs"
        />
      )}

      <Button
        size="sm"
        onClick={onSubmit}
        disabled={
          loading ||
          (sourceType === "postgresql"
            ? !connectionString.trim()
            : sourceType === "mysql"
              ? !connectionString.trim() || !database.trim()
              : sourceType === "mongodb"
                ? !connectionString.trim() || !database.trim()
                : sourceType === "duckdb"
                  ? !(duckdbFilePath ?? "").trim()
                  : sourceType === "code_repository"
                    ? !repoUrl.trim()
                    : !sampleData.trim())
        }
        className="w-full text-xs"
      >
        {loading ? (
          <Spinner size="xs" className="mr-1.5" />
        ) : (
          <HugeiconsIcon icon={Add01Icon} className="mr-1.5 h-3 w-3" size="100%" />
        )}
        {t("extendOntology")}
      </Button>
    </div>
  );
}
