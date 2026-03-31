"use client";

import { useState, useCallback } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import { PlusSignIcon } from "@hugeicons/core-free-icons";
import { createProject } from "@/lib/api";
import { isGitUrl } from "@/lib/git-url";
import { Button } from "@/components/ui/button";
import { FormInput } from "@/components/ui/form-input";
import { FormTextarea } from "@/components/ui/form-textarea";
import { FormField } from "@/components/ui/form-field";
import { Spinner } from "@/components/ui/spinner";
import { toast } from "sonner";
import type { DesignProject, DesignSource } from "@/types/api";
import { type GenerateSourceType, selectClassName } from "./design-panel-shared";

export function CreateProjectForm({
  guardBeforeCreate,
  onCreated,
}: {
  guardBeforeCreate: (actionName: string) => Promise<boolean>;
  onCreated: (p: DesignProject) => void;
}) {
  const [sourceType, setSourceType] = useState<GenerateSourceType>("postgresql");
  const [sampleData, setSampleData] = useState("");
  const [connectionString, setConnectionString] = useState("");
  const [schemaName, setSchemaName] = useState("public");
  const [repoPath, setRepoPath] = useState("");
  const [repoUrl, setRepoUrl] = useState("");
  const [title, setTitle] = useState("");
  const [loading, setLoading] = useState(false);

  // Ontology mode state
  // Inline validation — track which fields have been touched
  const [touched, setTouched] = useState<Record<string, boolean>>({});
  const markTouched = useCallback(
    (field: string) => setTouched((prev) => ({ ...prev, [field]: true })),
    [],
  );

  // Validation errors (only shown after field is touched)
  const [mysqlDatabase, setMysqlDatabase] = useState("");
  const [mongoDatabase, setMongoDatabase] = useState("");

  const isDbSource = sourceType === "postgresql" || sourceType === "mysql" || sourceType === "mongodb";

  const connectionError =
    touched.connectionString && isDbSource && !connectionString.trim()
      ? "Connection string is required"
      : undefined;
  const mysqlDatabaseError =
    touched.mysqlDatabase && sourceType === "mysql" && !mysqlDatabase.trim()
      ? "Database name is required"
      : undefined;
  const mongoDatabaseError =
    touched.mongoDatabase && sourceType === "mongodb" && !mongoDatabase.trim()
      ? "Database name is required"
      : undefined;
  const repoUrlError =
    touched.repoUrl && sourceType === "code_repository" && !repoUrl.trim()
      ? "Repository URL is required"
      : undefined;
  const sampleDataError =
    touched.sampleData &&
    !isDbSource &&
    sourceType !== "code_repository" &&
    !sampleData.trim()
      ? `${sourceType === "text" ? "Sample data" : "Source data"} is required`
      : undefined;

  function buildSource(): DesignSource | null {
    if (sourceType === "postgresql") {
      if (!connectionString.trim()) return null;
      return {
        type: "postgresql",
        connection_string: connectionString.trim(),
        schema: schemaName.trim() || "public",
      };
    }
    if (sourceType === "mysql") {
      if (!connectionString.trim() || !mysqlDatabase.trim()) return null;
      return {
        type: "mysql",
        connection_string: connectionString.trim(),
        schema: mysqlDatabase.trim(),
      };
    }
    if (sourceType === "mongodb") {
      if (!connectionString.trim() || !mongoDatabase.trim()) return null;
      return {
        type: "mongodb",
        connection_string: connectionString.trim(),
        database: mongoDatabase.trim(),
      };
    }
    if (sourceType === "code_repository") {
      if (!repoUrl.trim()) return null;
      return { type: "code_repository", url: repoUrl.trim() };
    }
    if (!sampleData.trim()) return null;
    return sourceType === "text"
      ? { type: "text", data: sampleData }
      : sourceType === "csv"
        ? { type: "csv", data: sampleData }
        : { type: "json", data: sampleData };
  }

  async function handleCreate() {
    setTouched({ connectionString: true, sampleData: true, repoUrl: true, mysqlDatabase: true, mongoDatabase: true });
    const source = buildSource();
    if (!source) return;
    if (!(await guardBeforeCreate("Create Project"))) return;

    setLoading(true);
    try {
      const project = await createProject({
        title: title.trim() || undefined,
        origin_type: "source",
        source,
        repo_source: repoPath.trim()
          ? isGitUrl(repoPath.trim())
            ? { type: "git_url" as const, url: repoPath.trim() }
            : { type: "local" as const, path: repoPath.trim() }
          : undefined,
      });
      onCreated(project);
      toast.success("Project created", {
        description: `Status: ${project.status} (rev ${project.revision})`,
      });
    } catch (err) {
      toast.error("Failed to create project", {
        description: err instanceof Error ? err.message : "Unknown error",
      });
    } finally {
      setLoading(false);
    }
  }

  const canSubmitSource =
    !loading &&
    (sourceType === "postgresql"
      ? !!connectionString.trim()
      : sourceType === "mysql"
        ? !!connectionString.trim() && !!mysqlDatabase.trim()
        : sourceType === "mongodb"
          ? !!connectionString.trim() && !!mongoDatabase.trim()
          : sourceType === "code_repository"
            ? !!repoUrl.trim()
            : !!sampleData.trim());

  const canSubmit = canSubmitSource;

  const dataPlaceholder =
    sourceType === "csv"
      ? "name,age,company\nAlice,30,Acme Corp\nBob,25,Globex"
      : sourceType === "json"
        ? '[{"name":"Alice","team":"Platform"},{"name":"Bob","team":"Data"}]'
        : "Describe your entities, relationships, and example records";

  return (
    <div>
      <h3 className="mb-1 text-xs font-semibold uppercase tracking-wider text-zinc-500">
        New Design Project
      </h3>
      <p className="mb-3 text-xs text-zinc-400">
        Create a project to analyze your data source and design an ontology.
        Fields marked with <span className="text-red-500">*</span> are required.
      </p>

      {/* Mode toggle removed — "From Existing Ontology" is now a Fork action on completed projects in the header dropdown */}

      <div className="grid max-w-2xl grid-cols-2 gap-3">
        <FormField label="Title" hint="Optional — auto-generated if empty">
          <FormInput
            type="text"
            placeholder="e.g. Olive Young DB Ontology"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </FormField>

        <FormField label="Source Type" required>
              <select
                value={sourceType}
                onChange={(e) => setSourceType(e.target.value as GenerateSourceType)}
                className={selectClassName}
              >
                <option value="postgresql">PostgreSQL</option>
                <option value="mysql">MySQL</option>
                <option value="mongodb">MongoDB</option>
                <option value="csv">CSV</option>
                <option value="json">JSON</option>
                <option value="code_repository">Code Repository</option>
                <option value="text">Plain Text (manual)</option>
              </select>
            </FormField>

            {sourceType === "postgresql" ? (
              <>
                <FormField
                  label="Connection String"
                  required
                  error={connectionError}
                  hint="postgres://user:password@host:5432/dbname"
                >
                  <FormInput
                    type="text"
                    placeholder="postgres://user:password@host:5432/dbname"
                    value={connectionString}
                    onChange={(e) => setConnectionString(e.target.value)}
                    onBlur={() => markTouched("connectionString")}
                    error={!!connectionError}
                    className="font-mono"
                  />
                </FormField>
                <FormField label="Schema" hint="Defaults to 'public'">
                  <FormInput
                    type="text"
                    placeholder="public"
                    value={schemaName}
                    onChange={(e) => setSchemaName(e.target.value)}
                  />
                </FormField>
              </>
            ) : sourceType === "mysql" ? (
              <>
                <FormField
                  label="Connection String"
                  required
                  error={connectionError}
                  hint="mysql://user:password@host:3306/dbname"
                >
                  <FormInput
                    type="text"
                    placeholder="mysql://user:password@host:3306/dbname"
                    value={connectionString}
                    onChange={(e) => setConnectionString(e.target.value)}
                    onBlur={() => markTouched("connectionString")}
                    error={!!connectionError}
                    className="font-mono"
                  />
                </FormField>
                <FormField label="Database" required error={mysqlDatabaseError}>
                  <FormInput
                    type="text"
                    placeholder="my_database"
                    value={mysqlDatabase}
                    onChange={(e) => setMysqlDatabase(e.target.value)}
                    onBlur={() => markTouched("mysqlDatabase")}
                    error={!!mysqlDatabaseError}
                  />
                </FormField>
              </>
            ) : sourceType === "mongodb" ? (
              <>
                <FormField
                  label="Connection String"
                  required
                  error={connectionError}
                  hint="mongodb://user:password@host:27017 or mongodb+srv://..."
                >
                  <FormInput
                    type="text"
                    placeholder="mongodb://user:password@host:27017"
                    value={connectionString}
                    onChange={(e) => setConnectionString(e.target.value)}
                    onBlur={() => markTouched("connectionString")}
                    error={!!connectionError}
                    className="font-mono"
                  />
                </FormField>
                <FormField label="Database" required error={mongoDatabaseError}>
                  <FormInput
                    type="text"
                    placeholder="my_database"
                    value={mongoDatabase}
                    onChange={(e) => setMongoDatabase(e.target.value)}
                    onBlur={() => markTouched("mongoDatabase")}
                    error={!!mongoDatabaseError}
                  />
                </FormField>
              </>
            ) : sourceType === "code_repository" ? (
              <div className="col-span-2">
                <FormField
                  label="Repository URL"
                  required
                  error={repoUrlError}
                  hint="Analyzes the default branch"
                >
                  <FormInput
                    type="text"
                    placeholder="https://github.com/org/repo"
                    value={repoUrl}
                    onChange={(e) => setRepoUrl(e.target.value)}
                    onBlur={() => markTouched("repoUrl")}
                    error={!!repoUrlError}
                    className="font-mono"
                  />
                </FormField>
              </div>
            ) : (
              <div className="col-span-2">
                <FormField
                  label={sourceType === "text" ? "Sample Data" : "Source Data"}
                  required
                  error={sampleDataError}
                >
                  <FormTextarea
                    rows={5}
                    placeholder={dataPlaceholder}
                    value={sampleData}
                    onChange={(e) => setSampleData(e.target.value)}
                    onBlur={() => markTouched("sampleData")}
                    error={!!sampleDataError}
                    className="font-mono"
                  />
                </FormField>
              </div>
            )}

            {!["text", "code_repository"].includes(sourceType) && (
              <div className="col-span-2">
                <FormField label="Repo Path or Git URL" hint="Optional — used for code analysis">
                  <FormInput
                    type="text"
                    placeholder="/path/to/repo or https://github.com/org/repo"
                    value={repoPath}
                    onChange={(e) => setRepoPath(e.target.value)}
                  />
                </FormField>
              </div>
            )}

        <div className="col-span-2">
          <Button onClick={handleCreate} disabled={!canSubmit} className="w-full">
            {loading ? (
              <Spinner size="xs" className="mr-2" />
            ) : (
              <HugeiconsIcon icon={PlusSignIcon} className="mr-2 h-4 w-4" size="100%" />
            )}
            {loading ? "Creating..." : "Create Project"}
          </Button>
        </div>
      </div>
    </div>
  );
}
