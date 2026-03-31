use std::sync::Arc;

use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use ox_store::Store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SearchRecipesTool — find reusable analysis algorithms
// ---------------------------------------------------------------------------

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
struct SearchRecipesOutput {
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
    const NAME: &'static str = super::SEARCH_RECIPES;
    const DESCRIPTION: &'static str =
        "Search for reusable analysis recipes (algorithms) before writing custom code. \
         Recipes include pre-built templates for time series, segmentation, classification, \
         regression, anomaly detection, and statistical analysis. \
         Returns recipe details including required input columns and parameters.";

    async fn handle(&self, input: Self::Input, _ctx: &ExecutionContext) -> ToolResult {
        let params = ox_store::CursorParams {
            limit: 20,
            cursor: None,
        };

        match self.store.list_recipes(&params).await {
            Ok(page) => {
                let query_lower = input.query.to_lowercase();
                let type_filter = input.algorithm_type.as_deref().unwrap_or("");

                let matched: Vec<RecipeEntry> = page
                    .items
                    .into_iter()
                    .filter(|r| {
                        let name_match = r.name.to_lowercase().contains(&query_lower)
                            || r.description.to_lowercase().contains(&query_lower);
                        let type_match =
                            type_filter.is_empty() || r.algorithm_type == type_filter;
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

                let output = SearchRecipesOutput {
                    total: matched.len(),
                    recipes: matched,
                };

                ToolResult::success(
                    serde_json::to_string_pretty(&output).unwrap_or_default(),
                )
            }
            Err(e) => ToolResult::error(format!("Recipe search failed: {e}")),
        }
    }
}
