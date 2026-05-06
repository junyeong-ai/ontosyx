use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::AnalysisRecipe;

use super::{CursorPage, CursorParams};

#[async_trait]
pub trait RecipeStore: Send + Sync {
    async fn upsert_recipe(&self, recipe: &AnalysisRecipe) -> OxResult<()>;
    async fn get_recipe(&self, id: Uuid) -> OxResult<Option<AnalysisRecipe>>;
    async fn list_recipes(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<AnalysisRecipe>>;
    async fn delete_recipe(&self, id: Uuid) -> OxResult<bool>;
    async fn update_recipe_status(&self, id: Uuid, status: &str) -> OxResult<()>;
    async fn create_recipe_version(&self, recipe: &AnalysisRecipe) -> OxResult<()>;
    async fn list_recipe_versions(&self, parent_id: Uuid) -> OxResult<Vec<AnalysisRecipe>>;
    /// Batch upsert multiple recipes in a single transaction.
    async fn upsert_recipes_batch(&self, recipes: &[AnalysisRecipe]) -> OxResult<()>;
}
