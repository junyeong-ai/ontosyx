"use client";

import { useEffect, useState, useCallback } from "react";
import { useTranslations } from "next-intl";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSelect } from "@/components/ui/form-input";
import { FormInput } from "@/components/ui/form-input";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { WorkbenchPageShell } from "@/components/workbench/workbench-page-shell";
import { toast } from "sonner";
import { useConfirm } from "@/components/providers/confirm-provider";
import type { AnalysisRecipe, RecipeStatus } from "@/types/api";
import {
  type CreateRecipeRequest,
  listRecipes,
  createRecipe,
  deleteRecipe,
  listRecipeVersions,
  updateRecipeStatus,
} from "@/lib/api";
import { useAuth } from "@/hooks/use-auth";
import { RecipeCard } from "@/components/recipes/recipe-card";
import { RecipeRunner } from "@/components/recipes/recipe-runner";
import { Analytics01Icon } from "@hugeicons/core-free-icons";
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge";

const ALGORITHM_TYPES = [
  "time_series",
  "segmentation",
  "classification",
  "regression",
  "anomaly_detection",
  "statistical_analysis",
  "custom",
] as const;
type KnownAlgorithmType = (typeof ALGORITHM_TYPES)[number];

function isKnownAlgorithmType(s: string): s is KnownAlgorithmType {
  return (
    s === "time_series" ||
    s === "segmentation" ||
    s === "classification" ||
    s === "regression" ||
    s === "anomaly_detection" ||
    s === "statistical_analysis" ||
    s === "custom"
  );
}

const STATUS_TONE: Record<RecipeStatus, StatusTone> = {
  draft: "warning",
  approved: "success",
  deprecated: "neutral",
};

export function RecipesWorkbench() {
  const t = useTranslations("settings.recipes");
  const [recipes, setRecipes] = useState<AnalysisRecipe[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [runningRecipe, setRunningRecipe] = useState<AnalysisRecipe | null>(null);
  const [search, setSearch] = useState("");
  const [createOpen, setCreateOpen] = useState(false);
  const { isAdmin } = useAuth();
  const confirm = useConfirm();

  useEffect(() => {
    listRecipes()
      .then((page) => setRecipes(page.items))
      .catch(() => toast.error(t("toast.loadFailed")))
      .finally(() => setLoading(false));
  }, [t]);

  const handleDelete = async (id: string) => {
    const recipe = recipes.find((r) => r.id === id);
    const ok = await confirm({
      title: t("deleteConfirm.title", { name: recipe?.name ?? id }),
      description: t("deleteConfirm.description"),
      variant: "danger",
    });
    if (!ok) return;
    try {
      await deleteRecipe(id);
      setRecipes((prev) => prev.filter((r) => r.id !== id));
      if (selectedId === id) setSelectedId(null);
      toast.success(t("toast.deleted"));
    } catch {
      toast.error(t("toast.deleteFailed"));
    }
  };

  const handleCreate = async (values: CreateRecipeRequest) => {
    const recipe = await createRecipe(values);
    setRecipes((prev) => [recipe, ...prev]);
    toast.success(t("toast.created"));
  };

  const handleStatusChange = useCallback(
    async (recipeId: string, status: RecipeStatus) => {
      try {
        await updateRecipeStatus(recipeId, status);
        setRecipes((prev) =>
          prev.map((r) => (r.id === recipeId ? { ...r, status } : r)),
        );
        toast.success(t("toast.statusChanged", { status: t(`status.${status}`) }));
      } catch {
        toast.error(t("toast.statusChangeFailed"));
      }
    },
    [t],
  );

  if (loading) {
    return (
      <WorkbenchPageShell title={t("title")} subtitle={t("description")}>
        <div className="flex h-full items-center justify-center py-12">
          <Spinner size="lg" />
        </div>
      </WorkbenchPageShell>
    );
  }

  const selected = recipes.find((r) => r.id === selectedId);

  const filtered = recipes.filter(
    (r) =>
      !search ||
      r.name.toLowerCase().includes(search.toLowerCase()) ||
      r.description.toLowerCase().includes(search.toLowerCase()),
  );

  return (
    <WorkbenchPageShell
      title={t("title")}
      subtitle={t("description")}
      actions={
        !createOpen && (
          <Button
            variant="primary"
            size="sm"
            onClick={() => setCreateOpen(true)}
          >
            {t("newRecipe")}
          </Button>
        )
      }
    >
      <div className="px-4 py-4">
        {createOpen && (
          <RecipeCreateForm
            onSubmit={handleCreate}
            onClose={() => setCreateOpen(false)}
          />
        )}

        {/* Search row */}
        <div className="mb-4 max-w-xs">
          <FormInput
            placeholder={t("searchPlaceholder")}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>

        {/* Gallery grid */}
        {filtered.length === 0 ? (
          <EmptyState
            icon={Analytics01Icon}
            title={recipes.length === 0 ? t("emptyAll") : t("emptyFiltered")}
          />
        ) : (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
            {filtered.map((r) => (
              <div
                key={r.id}
                onClick={() => setSelectedId(r.id === selectedId ? null : r.id)}
                className={`cursor-pointer rounded-lg ring-2 transition-shadow ${
                  r.id === selectedId
                    ? "ring-brand-foreground"
                    : "ring-transparent hover:ring-divider dark:hover:ring-divider"
                }`}
              >
                <RecipeCard
                  recipe={r}
                  onRun={(recipe) => {
                    // Stop click from toggling selection
                    setRunningRecipe(recipe);
                  }}
                />
              </div>
            ))}
          </div>
        )}

        {/* Detail panel */}
        {selected && (
          <Card variant="inset" padding="lg" className="mt-6">
            <RecipeDetail
              recipe={selected}
              onDelete={handleDelete}
              onStatusChange={handleStatusChange}
              isAdmin={isAdmin}
            />
          </Card>
        )}
      </div>

      {/* Runner modal */}
      {runningRecipe && (
        <RecipeRunner
          recipe={runningRecipe}
          onClose={() => setRunningRecipe(null)}
        />
      )}
    </WorkbenchPageShell>
  );
}


function RecipeStatusBadge({ status }: { status: RecipeStatus }) {
  const t = useTranslations("settings.recipes");
  return (
    <StatusBadge tone={STATUS_TONE[status]} className="font-semibold uppercase tracking-wider">
      {t(`status.${status}`)}
    </StatusBadge>
  );
}

// ---------------------------------------------------------------------------
// Recipe detail with version history
// ---------------------------------------------------------------------------

function RecipeDetail({
  recipe,
  onDelete,
  onStatusChange,
  isAdmin,
}: {
  recipe: AnalysisRecipe;
  onDelete: (id: string) => void;
  onStatusChange: (id: string, status: RecipeStatus) => void;
  isAdmin: boolean;
}) {
  const t = useTranslations("settings.recipes");
  const [versions, setVersions] = useState<AnalysisRecipe[] | null>(null);
  const [isVersionsOpen, setIsVersionsOpen] = useState(false);
  const [isVersionsLoading, setIsVersionsLoading] = useState(false);

  const loadVersions = useCallback(async () => {
    if (isVersionsOpen) {
      setIsVersionsOpen(false);
      return;
    }
    setIsVersionsLoading(true);
    try {
      const data = await listRecipeVersions(recipe.id);
      setVersions(data);
      setIsVersionsOpen(true);
    } catch {
      toast.error(t("toast.versionsError"));
    } finally {
      setIsVersionsLoading(false);
    }
  }, [recipe.id, isVersionsOpen, t]);

  useEffect(() => {
    setIsVersionsOpen(false);
    setVersions(null);
  }, [recipe.id]);

  const algoLabel = isKnownAlgorithmType(recipe.algorithm_type)
    ? t(`algorithmType.${recipe.algorithm_type}`)
    : recipe.algorithm_type;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-semibold text-foreground-strong">
              {recipe.name}
            </h2>
            <RecipeStatusBadge status={recipe.status} />
          </div>
          <p className="text-xs text-muted-foreground">
            {t("detail.meta", {
              algorithm: algoLabel,
              version: recipe.version,
              user: recipe.created_by,
              date: new Date(recipe.created_at).toLocaleDateString(),
            })}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {isAdmin && (
            <SettingsSelect
              label={t("detail.statusLabel")}
              hideLabel
              value={recipe.status}
              onChange={(e) =>
                onStatusChange(recipe.id, e.target.value as RecipeStatus)
              }
            >
              <option value="draft">{t("status.draft")}</option>
              <option value="approved">{t("status.approved")}</option>
              <option value="deprecated">{t("status.deprecated")}</option>
            </SettingsSelect>
          )}
          <button
            onClick={loadVersions}
            disabled={isVersionsLoading}
            className="rounded-md px-3 py-1.5 text-xs font-medium text-foreground hover:bg-surface-inset dark:text-muted-foreground dark:hover:bg-surface-base"
          >
            {isVersionsLoading ? (
              <Spinner size="sm" />
            ) : isVersionsOpen ? (
              t("detail.hideHistory")
            ) : (
              t("detail.versions")
            )}
          </button>
          <button
            onClick={() => onDelete(recipe.id)}
            className="rounded-md px-3 py-1.5 text-xs font-medium text-danger-foreground hover:bg-danger-surface dark:hover:bg-danger-surface"
          >
            {t("detail.delete")}
          </button>
        </div>
      </div>

      {isVersionsOpen && versions && (
        <VersionHistory versions={versions} currentRecipe={recipe} />
      )}

      <div>
        <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
          {t("detail.description")}
        </label>
        <p className="mt-0.5 text-sm text-foreground">
          {recipe.description}
        </p>
      </div>

      <div>
        <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
          {t("detail.requiredColumns")}
        </label>
        <div className="mt-0.5 flex flex-wrap gap-1">
          {recipe.required_columns.map((col) => (
            <span
              key={col}
              className="rounded bg-surface-inset px-1.5 py-0.5 text-xs text-foreground dark:text-muted-foreground"
            >
              {col}
            </span>
          ))}
        </div>
      </div>

      {recipe.output_description && (
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("detail.output")}
          </label>
          <p className="mt-0.5 text-sm text-foreground">
            {recipe.output_description}
          </p>
        </div>
      )}

      <div>
        <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
          {t("detail.codeTemplate")}
        </label>
        <pre className="mt-1 max-h-80 overflow-auto rounded-md bg-surface-base p-3 text-xs text-brand-foreground">
          {recipe.code_template}
        </pre>
      </div>

      {recipe.parameters && Object.keys(recipe.parameters).length > 0 && (
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("detail.parameters")}
          </label>
          <pre className="mt-1 rounded-md bg-surface-raised p-2 text-xs text-foreground dark:text-muted-foreground">
            {JSON.stringify(recipe.parameters, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Version history
// ---------------------------------------------------------------------------

function VersionHistory({
  versions,
  currentRecipe,
}: {
  versions: AnalysisRecipe[];
  currentRecipe: AnalysisRecipe;
}) {
  const t = useTranslations("settings.recipes");
  return (
    <Card variant="inset" padding="none">
      <Card.Header className="px-3 py-2">
        <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("versions.heading")}
        </span>
      </Card.Header>
      {versions.length === 0 ? (
        <p className="px-3 py-4 text-xs text-foreground-muted">
          {t("versions.empty")}
        </p>
      ) : (
        <div className="divide-y divide-divider">
          {versions.map((v) => (
            <VersionRow
              key={`${v.id}-${v.version}`}
              version={v}
              isCurrent={v.id === currentRecipe.id && v.version === currentRecipe.version}
            />
          ))}
        </div>
      )}
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Version row
// ---------------------------------------------------------------------------

function VersionRow({
  version,
  isCurrent,
}: {
  version: AnalysisRecipe;
  isCurrent: boolean;
}) {
  const t = useTranslations("settings.recipes");
  const [isExpanded, setIsExpanded] = useState(false);

  const algoLabel = isKnownAlgorithmType(version.algorithm_type)
    ? t(`algorithmType.${version.algorithm_type}`)
    : version.algorithm_type.replace(/_/g, " ");

  return (
    <div>
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center gap-3 px-3 py-2 text-left text-xs hover:bg-surface-inset dark:hover:bg-surface-base"
      >
        <span className="font-medium text-foreground">
          {t("versions.versionPrefix", { version: version.version })}
        </span>
        <RecipeStatusBadge status={version.status} />
        <span className="flex-1 text-muted-foreground">
          {t("versions.meta", {
            date: new Date(version.created_at).toLocaleDateString(),
            user: version.created_by,
          })}
        </span>
        {isCurrent && (
          <span className="rounded-full bg-brand-surface-strong px-1.5 py-0.5 text-2xs font-semibold text-brand-foreground-strong">
            {t("versions.current")}
          </span>
        )}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
          strokeWidth={2}
          stroke="currentColor"
          className={`h-3 w-3 text-muted-foreground transition-transform ${isExpanded ? "rotate-180" : ""}`}
        >
          <path strokeLinecap="round" strokeLinejoin="round" d="m19.5 8.25-7.5 7.5-7.5-7.5" />
        </svg>
      </button>
      {isExpanded && (
        <div className="border-t border-divider bg-surface-base px-3 py-3">
          <div className="space-y-2">
            <div>
              <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                {t("versions.description")}
              </label>
              <p className="text-xs text-foreground dark:text-muted-foreground">
                {version.description || t("versions.noDescription")}
              </p>
            </div>
            <div>
              <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                {t("versions.algorithm")}
              </label>
              <p className="text-xs text-foreground dark:text-muted-foreground">
                {algoLabel}
              </p>
            </div>
            <div>
              <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                {t("versions.code")}
              </label>
              <pre className="mt-0.5 max-h-40 overflow-auto rounded-md bg-surface-base p-2 text-2xs text-brand-foreground">
                {version.code_template}
              </pre>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Recipe creation form
// ---------------------------------------------------------------------------

function RecipeCreateForm({
  onSubmit,
  onClose,
}: {
  onSubmit: (values: CreateRecipeRequest) => Promise<void>;
  /** Close the form. Called after a successful submit and from the
   *  inline cancel button. Open state is owned by the parent so the
   *  workbench shell's action button can also drive it. */
  onClose: () => void;
}) {
  const t = useTranslations("settings.recipes");
  const [isSaving, setIsSaving] = useState(false);

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [algorithmType, setAlgorithmType] = useState<string>(ALGORITHM_TYPES[0]);
  const [codeTemplate, setCodeTemplate] = useState("");
  const [requiredColumns, setRequiredColumns] = useState("");
  const [outputDescription, setOutputDescription] = useState("");

  const reset = () => {
    setName("");
    setDescription("");
    setAlgorithmType(ALGORITHM_TYPES[0]);
    setCodeTemplate("");
    setRequiredColumns("");
    setOutputDescription("");
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !codeTemplate.trim()) return;
    setIsSaving(true);
    try {
      await onSubmit({
        name: name.trim(),
        description: description.trim(),
        algorithm_type: algorithmType,
        code_template: codeTemplate,
        parameters: {},
        required_columns: requiredColumns
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean),
        output_description: outputDescription.trim(),
      });
      reset();
      onClose();
    } catch {
      toast.error(t("toast.createFailed"));
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <form
      onSubmit={handleSubmit}
      className="mb-4 rounded-lg border border-brand-border bg-brand-surface p-4"
    >
      <div className="mb-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-brand-foreground">
          {t("form.newTitle")}
        </span>
        <button
          type="button"
          onClick={() => { reset(); onClose(); }}
          className="text-xs text-muted-foreground hover:text-foreground"
        >
          {t("form.cancel")}
        </button>
      </div>

      <div className="space-y-3">
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.name")}
          </label>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("form.namePlaceholder")}
            required
            className="mt-0.5 w-full rounded-md border border-divider bg-surface-base px-3 py-1.5 text-xs"
          />
        </div>

        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.description")}
          </label>
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={t("form.descriptionPlaceholder")}
            rows={2}
            className="mt-0.5 w-full rounded-md border border-divider bg-surface-base px-3 py-1.5 text-xs"
          />
        </div>

        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.algorithmType")}
          </label>
          <SettingsSelect
            label={t("form.algorithmType")}
            hideLabel
            value={algorithmType}
            onChange={(e) => setAlgorithmType(e.target.value)}
          >
            {ALGORITHM_TYPES.map((value) => (
              <option key={value} value={value}>
                {t(`algorithmType.${value}`)}
              </option>
            ))}
          </SettingsSelect>
        </div>

        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.codeTemplate")}
          </label>
          <textarea
            value={codeTemplate}
            onChange={(e) => setCodeTemplate(e.target.value)}
            placeholder={t("form.codeTemplatePlaceholder")}
            rows={12}
            required
            className="mt-0.5 w-full rounded-md border border-divider bg-surface-base px-3 py-1.5 font-mono text-xs"
          />
        </div>

        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.requiredColumns")}
          </label>
          <input
            value={requiredColumns}
            onChange={(e) => setRequiredColumns(e.target.value)}
            placeholder={t("form.requiredColumnsPlaceholder")}
            className="mt-0.5 w-full rounded-md border border-divider bg-surface-base px-3 py-1.5 text-xs"
          />
        </div>

        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {t("form.outputDescription")}
          </label>
          <input
            value={outputDescription}
            onChange={(e) => setOutputDescription(e.target.value)}
            placeholder={t("form.outputDescriptionPlaceholder")}
            className="mt-0.5 w-full rounded-md border border-divider bg-surface-base px-3 py-1.5 text-xs"
          />
        </div>

        <button
          type="submit"
          disabled={!name.trim() || !codeTemplate.trim() || isSaving}
          className="rounded-md bg-brand-solid px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50 hover:bg-brand-solid"
        >
          {isSaving ? t("form.creating") : t("form.create")}
        </button>
      </div>
    </form>
  );
}
