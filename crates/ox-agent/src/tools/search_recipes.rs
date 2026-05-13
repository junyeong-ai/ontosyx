use std::sync::Arc;

use async_trait::async_trait;
use entelix::tools::ToolEffect;
use entelix::{AgentContext, SchemaTool};
use ox_store::Store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchRecipesInput {
    /// Search query: algorithm name, description, or use case.
    pub query: String,
    /// Filter by algorithm type: "time_series", "segmentation", "classification",
    /// "regression", "anomaly_detection", "statistical_analysis", "custom".
    #[serde(default)]
    pub algorithm_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchRecipesOutput {
    recipes: Vec<RecipeEntry>,
    total: usize,
}

#[derive(Debug, Serialize)]
struct RecipeEntry {
    id: String,
    name: String,
    description: String,
    algorithm_type: String,
    required_columns: Vec<String>,
}

/// Searches the recipe registry for reusable analysis algorithms.
/// Call this before writing custom analysis code — a recipe might already exist.
pub struct SearchRecipesTool {
    pub store: Arc<dyn Store>,
}

#[async_trait]
impl SchemaTool for SearchRecipesTool {
    type Input = SearchRecipesInput;
    type Output = SearchRecipesOutput;
    const NAME: &'static str = super::SEARCH_RECIPES;

    fn description(&self) -> &str {
        "Search reusable analysis recipes (time series, segmentation, classification, \
         regression, anomaly detection, statistics) before writing custom code. \
         Returns required input columns and parameters."
    }

    fn effect(&self) -> ToolEffect {
        ToolEffect::ReadOnly
    }

    async fn execute(
        &self,
        input: Self::Input,
        _ctx: &AgentContext<()>,
    ) -> entelix::Result<Self::Output> {
        let params = ox_store::CursorParams {
            limit: 20,
            cursor: None,
        };

        let page =
            self.store.list_recipes(&params).await.map_err(|e| {
                entelix::Error::invalid_request(format!("Recipe search failed: {e}"))
            })?;

        let query_lower = input.query.to_lowercase();
        let type_filter = input.algorithm_type.as_deref().unwrap_or("");

        let matched: Vec<RecipeEntry> = page
            .items
            .into_iter()
            .filter(|r| {
                let name_match = r.name.to_lowercase().contains(&query_lower)
                    || r.description.to_lowercase().contains(&query_lower);
                let type_match = type_filter.is_empty() || r.algorithm_type == type_filter;
                name_match && type_match
            })
            .map(|r| RecipeEntry {
                id: r.id.to_string(),
                name: r.name,
                description: r.description,
                algorithm_type: r.algorithm_type,
                required_columns: serde_json::from_value(
                    serde_json::to_value(&r.required_columns).unwrap_or_default(),
                )
                .unwrap_or_default(),
            })
            .collect();

        Ok(SearchRecipesOutput {
            total: matched.len(),
            recipes: matched,
        })
    }
}
