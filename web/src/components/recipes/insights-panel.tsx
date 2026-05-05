"use client";

import { useCallback, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import type { AnalysisRecipe } from "@/types/api";
import { useRecipes } from "@/hooks/api/use-recipes";
import { ErrorState } from "@/components/ui/error-state";
import { Heading } from "@/components/ui/heading";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonCard } from "@/components/ui/skeleton";
import { RecipeCard } from "./recipe-card";
import { RecipeRunner } from "./recipe-runner";

// ---------------------------------------------------------------------------
// Insights panel — displayed inside the Analyze right tab area
// ---------------------------------------------------------------------------

export function InsightsPanel() {
  const t = useTranslations("settings.recipes.insightsPanel");
  const tCard = useTranslations("settings.recipes.card");
  const tCommon = useTranslations("common");
  const [runningRecipe, setRunningRecipe] = useState<AnalysisRecipe | null>(
    null,
  );

  const recipesQuery = useRecipes({ limit: 50 });
  const { data, isLoading, isError } = recipesQuery;

  // Show only approved recipes in insights
  const recipes = useMemo(
    () => data?.items.filter((r) => r.status === "approved") ?? [],
    [data],
  );

  const handleRun = useCallback((recipe: AnalysisRecipe) => {
    setRunningRecipe(recipe);
  }, []);

  if (isLoading) {
    return (
      <div className="px-4 py-4">
        <div className="grid grid-cols-1 gap-3">
          {Array.from({ length: 3 }, (_, i) => (
            <SkeletonCard key={i} />
          ))}
        </div>
      </div>
    );
  }

  if (isError) {
    return (
      <div className="px-4 py-4">
        <ErrorState
          title={tCommon("loadError.title")}
          description={t("toast.loadFailed")}
          onRetry={() => recipesQuery.refetch()}
          retryLabel={tCommon("retry")}
        />
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="px-4 py-4">
        <Heading level={2} size={6}>
          {t("heading")}
        </Heading>
        <p className="mt-0.5 text-xs text-foreground-muted">
          {t("description")}
        </p>

        {recipes.length === 0 ? (
          <EmptyState variant="compact" title={t("emptyApproved")} />
        ) : (
          <div className="mt-4 grid grid-cols-1 gap-3">
            {recipes.map((recipe) => (
              <RecipeCard
                key={recipe.id}
                recipe={recipe}
                compact
                onRun={handleRun}
                actionLabel={tCard("applyAction")}
              />
            ))}
          </div>
        )}
      </div>

      {/* Runner modal */}
      {runningRecipe && (
        <RecipeRunner
          recipe={runningRecipe}
          onClose={() => setRunningRecipe(null)}
        />
      )}
    </div>
  );
}
