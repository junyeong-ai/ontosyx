"use client";

import { useMemo, useState, useCallback } from "react";
import { useTranslations } from "next-intl";
import { z } from "zod";
import { Spinner } from "@/components/ui/spinner";
import { Heading } from "@/components/ui/heading";
import { FormField } from "@/components/ui/form-field";
import { FormInput, FormTextarea, SettingsSelect } from "@/components/ui/form-input";
import { Button } from "@/components/ui/button";
import { useFormWithSchema } from "@/hooks/use-form-with-schema";
import { Card } from "@/components/ui/card";
import { SkeletonCard } from "@/components/ui/skeleton";
import { WorkbenchPageShell } from "@/components/workbench/workbench-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { toast } from "@/components/ui/toast";
import { useConfirm } from "@/components/providers/confirm-provider";
import type { AnalysisRecipe, RecipeStatus } from "@/types/api";
import {
  type CreateRecipeRequest,
  listRecipeVersions,
} from "@/lib/api";
import {
  useRecipes,
  useCreateRecipe,
  useDeleteRecipe,
  useUpdateRecipeStatus,
} from "@/hooks/api/use-recipes";
import { useAuth } from "@/hooks/use-auth";
import { RecipeCard } from "@/components/recipes/recipe-card";
import { RecipeRunner } from "@/components/recipes/recipe-runner";
import { Analytics01Icon, ArrowDown01Icon, Search01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
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
  const tCommon = useTranslations("common");
  const recipesQuery = useRecipes();
  const createMutation = useCreateRecipe();
  const deleteMutation = useDeleteRecipe();
  const statusMutation = useUpdateRecipeStatus();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [runningRecipe, setRunningRecipe] = useState<AnalysisRecipe | null>(null);
  const [search, setSearch] = useState("");
  const [createOpen, setCreateOpen] = useState(false);
  const { isAdmin } = useAuth();
  const confirm = useConfirm();

  const recipes = useMemo(
    () => recipesQuery.data?.items ?? [],
    [recipesQuery.data],
  );

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return recipes;
    return recipes.filter(
      (r) =>
        r.name.toLowerCase().includes(q) ||
        r.description.toLowerCase().includes(q),
    );
  }, [recipes, search]);

  const selected = recipes.find((r) => r.id === selectedId);

  const handleDelete = useCallback(
    async (id: string) => {
      const recipe = recipes.find((r) => r.id === id);
      const ok = await confirm({
        title: t("deleteConfirm.title", { name: recipe?.name ?? id }),
        description: t("deleteConfirm.description"),
        variant: "danger",
      });
      if (!ok) return;
      try {
        await deleteMutation.mutateAsync(id);
        if (selectedId === id) setSelectedId(null);
        toast.success(t("toast.deleted"));
      } catch {
        toast.error(t("toast.deleteFailed"));
      }
    },
    [recipes, confirm, t, deleteMutation, selectedId],
  );

  const handleCreate = useCallback(
    async (values: CreateRecipeRequest) => {
      await createMutation.mutateAsync(values);
      toast.success(t("toast.created"));
    },
    [createMutation, t],
  );

  const handleStatusChange = useCallback(
    async (recipeId: string, status: RecipeStatus) => {
      try {
        await statusMutation.mutateAsync({ id: recipeId, status });
        toast.success(
          t("toast.statusChanged", { status: t(`status.${status}`) }),
        );
      } catch {
        toast.error(t("toast.statusChangeFailed"));
      }
    },
    [statusMutation, t],
  );

  const pageState: PageState = recipesQuery.isLoading
    ? { kind: "loading" }
    : recipesQuery.isError
      ? { kind: "error", onRetry: () => void recipesQuery.refetch() }
      : recipes.length === 0
        ? { kind: "empty" }
        : filtered.length === 0
          ? { kind: "filtered-empty", onClearFilters: () => setSearch("") }
          : { kind: "data" };

  return (
    <WorkbenchPageShell
      title={t("title")}
      subtitle={t("description")}
      count={recipes.length}
      pageState={pageState}
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
      filters={
        <div className="max-w-xs flex-1">
          <FormInput
            placeholder={t("searchPlaceholder")}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
      }
    >
      <div>
        {createOpen && (
          <RecipeCreateForm
            onSubmit={handleCreate}
            onClose={() => setCreateOpen(false)}
          />
        )}

        <PageStateView
          state={pageState}
          skeleton={
            <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
              {Array.from({ length: 6 }, (_, i) => (
                <SkeletonCard key={i} />
              ))}
            </div>
          }
          error={{
            title: tCommon("loadError.title"),
            description: tCommon("loadError.description"),
            retryLabel: tCommon("retry"),
          }}
          empty={{
            icon: Analytics01Icon,
            title: t("empty.title"),
            description: t("empty.description"),
            action: {
              label: t("empty.cta"),
              onClick: () => setCreateOpen(true),
            },
          }}
          filteredEmpty={{
            icon: Search01Icon,
            title: t("filteredEmpty.title"),
            description: t("filteredEmpty.description"),
            clearLabel: t("filteredEmpty.clearFilters"),
          }}
        >
          <div className="stagger-fade-in grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
            {filtered.map((r) => (
              <div
                key={r.id}
                onClick={() => setSelectedId(r.id === selectedId ? null : r.id)}
                className={`cursor-pointer rounded-lg ring-2 transition-shadow duration-[var(--duration-base)] ease-[var(--ease-out)] ${
                  r.id === selectedId
                    ? "ring-brand-foreground"
                    : "ring-transparent hover:ring-divider"
                }`}
              >
                <RecipeCard
                  recipe={r}
                  onRun={(recipe) => {
                    setRunningRecipe(recipe);
                  }}
                />
              </div>
            ))}
          </div>
        </PageStateView>

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

  // Reset version-history disclosure when the recipe identity
  // changes — tracked-key idiom (conditional setState during render)
  // is the React-19-blessed alternative to a setState-in-effect
  // reset. The first render after a recipe.id change sees the
  // mismatch, updates state in one pass, and subsequent renders
  // skip the branch.
  const [trackedRecipeId, setTrackedRecipeId] = useState(recipe.id);
  if (trackedRecipeId !== recipe.id) {
    setTrackedRecipeId(recipe.id);
    setIsVersionsOpen(false);
    setVersions(null);
  }

  const algoLabel = isKnownAlgorithmType(recipe.algorithm_type)
    ? t(`algorithmType.${recipe.algorithm_type}`)
    : recipe.algorithm_type;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="flex items-center gap-2">
            <Heading level={2} size={6}>
              {recipe.name}
            </Heading>
            <RecipeStatusBadge status={recipe.status} />
          </div>
          <p className="text-xs text-foreground-muted">
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
          <button type="button"
            onClick={loadVersions}
            disabled={isVersionsLoading}
            className="rounded-md px-3 py-1.5 text-xs font-medium text-foreground hover:bg-surface-inset"
          >
            {isVersionsLoading ? (
              <Spinner size="sm" />
            ) : isVersionsOpen ? (
              t("detail.hideHistory")
            ) : (
              t("detail.versions")
            )}
          </button>
          <button type="button"
            onClick={() => onDelete(recipe.id)}
            className="rounded-md px-3 py-1.5 text-xs font-medium text-danger-foreground hover:bg-danger-surface"
          >
            {t("detail.delete")}
          </button>
        </div>
      </div>

      {isVersionsOpen && versions && (
        <VersionHistory versions={versions} currentRecipe={recipe} />
      )}

      <div>
        <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("detail.description")}
        </label>
        <p className="mt-0.5 text-sm text-foreground">
          {recipe.description}
        </p>
      </div>

      <div>
        <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("detail.requiredColumns")}
        </label>
        <div className="mt-0.5 flex flex-wrap gap-1">
          {recipe.required_columns.map((col) => (
            <span
              key={col}
              className="rounded bg-surface-inset px-1.5 py-0.5 text-xs text-foreground"
            >
              {col}
            </span>
          ))}
        </div>
      </div>

      {recipe.output_description && (
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("detail.output")}
          </label>
          <p className="mt-0.5 text-sm text-foreground">
            {recipe.output_description}
          </p>
        </div>
      )}

      <div>
        <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("detail.codeTemplate")}
        </label>
        <pre className="mt-1 max-h-80 overflow-auto rounded-md bg-surface-base p-3 text-xs text-brand-foreground">
          {recipe.code_template}
        </pre>
      </div>

      {recipe.parameters && Object.keys(recipe.parameters).length > 0 && (
        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("detail.parameters")}
          </label>
          <pre className="mt-1 rounded-md bg-surface-raised p-2 text-xs text-foreground">
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
      <button type="button"
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center gap-3 px-3 py-2 text-start text-xs hover:bg-surface-inset"
      >
        <span className="font-medium text-foreground">
          {t("versions.versionPrefix", { version: version.version })}
        </span>
        <RecipeStatusBadge status={version.status} />
        <span className="flex-1 text-foreground-muted">
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
        <HugeiconsIcon
          icon={ArrowDown01Icon}
          className={`h-3 w-3 text-foreground-muted transition-transform duration-[var(--duration-quick)] ease-[var(--ease-out)] ${isExpanded ? "rotate-180" : ""}`}
          size="100%"
        />
      </button>
      {isExpanded && (
        <div className="border-t border-divider bg-surface-base px-3 py-3">
          <div className="space-y-2">
            <div>
              <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                {t("versions.description")}
              </label>
              <p className="text-xs text-foreground">
                {version.description || t("versions.noDescription")}
              </p>
            </div>
            <div>
              <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                {t("versions.algorithm")}
              </label>
              <p className="text-xs text-foreground">
                {algoLabel}
              </p>
            </div>
            <div>
              <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
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

const RECIPE_CREATE_SCHEMA = z.object({
  name: z.string().trim().min(1, { message: "form.errors.nameRequired" }),
  codeTemplate: z
    .string()
    .trim()
    .min(1, { message: "form.errors.codeTemplateRequired" }),
});

type RecipeCreateFormInput = z.input<typeof RECIPE_CREATE_SCHEMA>;

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

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [algorithmType, setAlgorithmType] = useState<string>(ALGORITHM_TYPES[0]);
  const [codeTemplate, setCodeTemplate] = useState("");
  const [requiredColumns, setRequiredColumns] = useState("");
  const [outputDescription, setOutputDescription] = useState("");

  const reset = useCallback(() => {
    setName("");
    setDescription("");
    setAlgorithmType(ALGORITHM_TYPES[0]);
    setCodeTemplate("");
    setRequiredColumns("");
    setOutputDescription("");
  }, []);

  const onValid = useCallback(
    async (validated: RecipeCreateFormInput) => {
      try {
        await onSubmit({
          name: validated.name,
          description: description.trim(),
          algorithm_type: algorithmType,
          code_template: validated.codeTemplate,
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
      }
    },
    [
      onSubmit, 
      onClose, 
      description, 
      algorithmType, 
      requiredColumns, 
      outputDescription, 
      t, reset
    ],
  );

  const { errors, submit, clearErrors, pending } = useFormWithSchema({
    schema: RECIPE_CREATE_SCHEMA,
    onValid,
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    void submit({ name, codeTemplate });
  };

  const nameError = errors.name ? t(errors.name) : undefined;
  const codeError = errors.codeTemplate ? t(errors.codeTemplate) : undefined;

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
          className="text-xs text-foreground-muted hover:text-foreground"
        >
          {t("form.cancel")}
        </button>
      </div>

      <div className="space-y-3">
        <FormField label={t("form.name")} error={nameError}>
          <FormInput
            value={name}
            onChange={(e) => {
              setName(e.target.value);
              clearErrors("name");
            }}
            placeholder={t("form.namePlaceholder")}
            error={!!nameError}
          />
        </FormField>

        <FormField label={t("form.description")}>
          <FormTextarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={t("form.descriptionPlaceholder")}
            rows={2}
          />
        </FormField>

        <SettingsSelect
          label={t("form.algorithmType")}
          value={algorithmType}
          onChange={(e) => setAlgorithmType(e.target.value)}
        >
          {ALGORITHM_TYPES.map((value) => (
            <option key={value} value={value}>
              {t(`algorithmType.${value}`)}
            </option>
          ))}
        </SettingsSelect>

        <FormField label={t("form.codeTemplate")} error={codeError}>
          <FormTextarea
            value={codeTemplate}
            onChange={(e) => {
              setCodeTemplate(e.target.value);
              clearErrors("codeTemplate");
            }}
            placeholder={t("form.codeTemplatePlaceholder")}
            rows={12}
            className="font-mono"
            error={!!codeError}
          />
        </FormField>

        <FormField label={t("form.requiredColumns")}>
          <FormInput
            value={requiredColumns}
            onChange={(e) => setRequiredColumns(e.target.value)}
            placeholder={t("form.requiredColumnsPlaceholder")}
          />
        </FormField>

        <FormField label={t("form.outputDescription")}>
          <FormInput
            value={outputDescription}
            onChange={(e) => setOutputDescription(e.target.value)}
            placeholder={t("form.outputDescriptionPlaceholder")}
          />
        </FormField>

        <Button
          type="submit"
          variant="primary"
          size="sm"
          loading={pending}
        >
          {t("form.create")}
        </Button>
      </div>
    </form>
  );
}
