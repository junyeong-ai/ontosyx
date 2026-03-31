"use client";

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
            placeholder="postgres://user:password@host:5432/dbname"
            value={connectionString}
            onChange={(e) => setConnectionString(e.target.value)}
            className="font-mono"
          />
          <FormInput
            type="text"
            placeholder="Schema (default: public)"
            value={schemaName}
            onChange={(e) => setSchemaName(e.target.value)}
          />
        </>
      ) : sourceType === "code_repository" ? (
        <FormInput
          type="text"
          placeholder="https://github.com/org/repo.git"
          value={repoUrl}
          onChange={(e) => setRepoUrl(e.target.value)}
          className="font-mono"
        />
      ) : (
        <FormTextarea
          rows={4}
          placeholder="Paste your data here..."
          value={sampleData}
          onChange={(e) => setSampleData(e.target.value)}
          className="font-mono text-xs"
        />
      )}
      {sourceType !== "text" && sourceType !== "code_repository" && (
        <FormInput
          type="text"
          placeholder="Repo path or Git URL (optional)"
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
        Reanalyze
      </Button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Extend source form
// ---------------------------------------------------------------------------

export const SOURCE_TYPE_OPTIONS: { value: DesignSource["type"]; label: string }[] = [
  { value: "text", label: "Text" },
  { value: "csv", label: "CSV" },
  { value: "json", label: "JSON" },
  { value: "code_repository", label: "Code Repo" },
  { value: "postgresql", label: "PostgreSQL" },
  { value: "mysql", label: "MySQL" },
  { value: "mongodb", label: "MongoDB" },
];

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
  loading: boolean;
  onSubmit: () => void;
}) {
  return (
    <div className="space-y-2 rounded-lg border border-blue-200 bg-blue-50/50 p-3 dark:border-blue-900 dark:bg-blue-950/20">
      <h4 className="text-xs font-semibold text-blue-800 dark:text-blue-200">
        New Source
      </h4>

      {/* Source type selector */}
      <div className="flex gap-1">
        {SOURCE_TYPE_OPTIONS.map((opt) => (
          <button
            key={opt.value}
            onClick={() => setSourceType(opt.value)}
            className={cn(
              "rounded px-2 py-0.5 text-[10px] font-medium transition-colors",
              sourceType === opt.value
                ? "bg-blue-600 text-white dark:bg-blue-500"
                : "bg-zinc-100 text-zinc-600 hover:bg-zinc-200 dark:bg-zinc-800 dark:text-zinc-400 dark:hover:bg-zinc-700",
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
            placeholder={sourceType === "postgresql"
              ? "postgres://user:password@host:5432/dbname"
              : "mysql://user:password@host:3306/dbname"}
            value={connectionString}
            onChange={(e) => setConnectionString(e.target.value)}
            className="font-mono"
          />
          <FormInput
            type="text"
            placeholder={sourceType === "postgresql" ? "Schema (default: public)" : "Database name"}
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
            placeholder="mongodb://user:password@host:27017"
            value={connectionString}
            onChange={(e) => setConnectionString(e.target.value)}
            className="font-mono"
          />
          <FormInput
            type="text"
            placeholder="Database name"
            value={database}
            onChange={(e) => setDatabase(e.target.value)}
          />
        </>
      ) : sourceType === "code_repository" ? (
        <FormInput
          type="text"
          placeholder="https://github.com/org/repo.git"
          value={repoUrl}
          onChange={(e) => setRepoUrl(e.target.value)}
          className="font-mono"
        />
      ) : (
        <FormTextarea
          rows={4}
          placeholder="Paste your data here..."
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
        Extend Ontology
      </Button>
    </div>
  );
}
