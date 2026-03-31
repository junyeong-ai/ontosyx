"use client";

import { useEffect, useState, useCallback } from "react";
import type { AnalysisRecipe } from "@/types/api";
import { listRecipes } from "@/lib/api";
import { Spinner } from "@/components/ui/spinner";
import { RecipeCard } from "./recipe-card";
import { RecipeRunner } from "./recipe-runner";

// ---------------------------------------------------------------------------
// Insights panel — displayed inside the Analyze right tab area
// ---------------------------------------------------------------------------

export function InsightsPanel() {
  const [recipes, setRecipes] = useState<AnalysisRecipe[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [runningRecipe, setRunningRecipe] = useState<AnalysisRecipe | null>(
    null,
  );

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listRecipes({ limit: 50 })
      .then((page) => {
        if (!cancelled) {
          // Show only approved recipes in insights
          setRecipes(page.items.filter((r) => r.status === "approved"));
        }
      })
      .catch(() => {
        if (!cancelled) setError("Failed to load analysis recipes.");
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const handleRun = useCallback((recipe: AnalysisRecipe) => {
    setRunningRecipe(recipe);
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Spinner size="lg" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="px-4 py-8 text-center text-sm text-zinc-500">
        {error}
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="px-4 py-4">
        <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">
          Available Analyses
        </h2>
        <p className="mt-0.5 text-xs text-zinc-500">
          Pre-built analysis recipes you can run against your ontology data.
        </p>

        {recipes.length === 0 ? (
          <p className="mt-8 text-center text-sm text-zinc-400">
            No approved recipes available yet.
          </p>
        ) : (
          <div className="mt-4 grid grid-cols-1 gap-3">
            {recipes.map((recipe) => (
              <RecipeCard
                key={recipe.id}
                recipe={recipe}
                compact
                onRun={handleRun}
                actionLabel="Apply to current ontology"
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
