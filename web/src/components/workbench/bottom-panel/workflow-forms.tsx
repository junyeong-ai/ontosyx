"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Refresh01Icon, Add01Icon } from "@hugeicons/core-free-icons";
import { Spinner } from "@/components/ui/spinner";
import { Button } from "@/components/ui/button";
import { FormInput } from "@/components/ui/form-input";
import { FormTextarea } from "@/components/ui/form-textarea";
import { cn } from "@/lib/cn";
import type { DesignSource } from "@/types/api";

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
}) {
  const t = useTranslations("workbench.bottomPanel.workflowForms");
  const isDisabled = loading || (() => {
    if (sourceType === "postgresql") return !connectionString.trim();
    if (sourceType === "code_repository") return !repoUrl.trim();
    return !sampleData.trim();
  })();

  return (
    <div className="space-y-2 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3 dark:border-zinc-700 dark:bg-zinc-900/50">
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
        {t("reanalyze")}
      </Button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Extend source form
// ---------------------------------------------------------------------------

export const SOURCE_TYPE_OPTIONS: DesignSource["type"][] = [
  "text",
  "csv",
  "json",
  "duckdb",
  "code_repository",
  "postgresql",
  "mysql",
  "mongodb",
];

// Which SOURCE_TYPE_OPTIONS entries have a translation key in
// workbench.bottomPanel.workflowForms.sourceTypeLabels — narrows the
// label lookup to the subset the extend form actually renders.
const EXTEND_SOURCE_TYPE_KEYS = SOURCE_TYPE_OPTIONS;
type ExtendSourceType = (typeof EXTEND_SOURCE_TYPE_KEYS)[number];
function isExtendSourceType(s: DesignSource["type"]): s is ExtendSourceType {
  return (EXTEND_SOURCE_TYPE_KEYS as readonly string[]).includes(s);
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
    <div className="space-y-2 rounded-lg border border-blue-200 bg-blue-50/50 p-3 dark:border-blue-900 dark:bg-blue-950/20">
      <h4 className="text-xs font-semibold text-blue-800 dark:text-blue-200">
        {t("newSource")}
      </h4>

      {/* Source type selector */}
      <div className="flex gap-1">
        {sourceTypes.map((opt) => (
          <button
            key={opt.value}
            onClick={() => setSourceType(opt.value)}
            className={cn(
              "rounded px-2 py-0.5 text-[10px] font-medium transition-colors",
              sourceType === opt.value
                ? "bg-blue-600 text-white dark:bg-blue-500"
                : "bg-zinc-100 text-zinc-600 hover:bg-zinc-200 dark:bg-zinc-800 dark:text-muted-foreground dark:hover:bg-zinc-700",
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
