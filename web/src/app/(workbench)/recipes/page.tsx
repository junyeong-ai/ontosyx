import { RecipesWorkbench } from "@/components/workbench/recipes/recipes-workbench";

/**
 * Recipes workbench mode (seventh — alongside Design / Analyze /
 * Explore / Dashboard / Glossary / Vocabulary). Lifted from
 * `/settings/recipes` so analysts run recipes from a workbench
 * surface rather than the admin sidebar; the underlying gallery +
 * detail + runner shapes are unchanged.
 */
export default function RecipesPage() {
  return <RecipesWorkbench />;
}
