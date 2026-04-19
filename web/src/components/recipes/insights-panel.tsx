"use client";

import { useCallback, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import type { AnalysisRecipe } from "@/types/api";
import { useRecipes } from "@/hooks/api/use-recipes";
import { Spinner } from "@/components/ui/spinner";
import { RecipeCard } from "./recipe-card";
import { RecipeRunner } from "./recipe-runner";

// ---------------------------------------------------------------------------
// Insights panel — displayed inside the Analyze right tab area
// ---------------------------------------------------------------------------

export function InsightsPanel() {
  const t = useTranslations("settings.recipes.insightsPanel");
  const tCard = useTranslations("settings.recipes.card");
  const [runningRecipe, setRunningRecipe] = useState<AnalysisRecipe | null>(
    null,
  );

  const { data, isLoading, isError } = useRecipes({ limit: 50 });

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
      <div className="flex items-center justify-center py-12">
        <Spinner size="lg" />
      </div>
    );
  }

  if (isError) {
    return (
      <div className="px-4 py-8 text-center text-sm text-muted-foreground">
        {t("loadError")}
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="px-4 py-4">
        <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">
          {t("heading")}
        </h2>
        <p className="mt-0.5 text-xs text-muted-foreground">
          {t("description")}
        </p>

        {recipes.length === 0 ? (
          <p className="mt-8 text-center text-sm text-muted-foreground">
            {t("emptyApproved")}
          </p>
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
