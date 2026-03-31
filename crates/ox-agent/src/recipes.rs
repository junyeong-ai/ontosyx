use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// AnalysisRecipe — reusable data analysis algorithm
// ---------------------------------------------------------------------------

/// A saved analysis algorithm that can be re-executed with different parameters.
///
/// Agent creates recipes when a novel analysis is successful, and searches
/// for existing recipes before designing new algorithms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRecipe {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub algorithm_type: AlgorithmType,
    /// Python code template with parameter placeholders
    pub code_template: String,
    /// Adjustable parameters
    pub parameters: Vec<RecipeParameter>,
    /// Required input columns/fields
    pub required_columns: Vec<String>,
    /// Expected output format description
    pub output_description: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    /// Monotonically increasing version number within a recipe chain
    #[serde(default = "default_version")]
    pub version: i32,
    /// "draft", "approved", "deprecated"
    #[serde(default = "default_status")]
    pub status: String,
    /// Previous version's ID (for version chain)
    #[serde(default)]
    pub parent_id: Option<Uuid>,
}

fn default_version() -> i32 {
    1
}

fn default_status() -> String {
    "draft".to_string()
}

/// Classification of analysis algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlgorithmType {
    /// Time series forecasting (trend, seasonality, prediction)
    TimeSeries,
    /// Customer/data segmentation (clustering, RFM)
    Segmentation,
    /// Binary/multi-class classification (churn, fraud)
    Classification,
    /// Numeric prediction (price, demand)
    Regression,
    /// Statistical outlier detection
    AnomalyDetection,
    /// Correlation, distribution, summary statistics
    StatisticalAnalysis,
    /// Custom/other
    Custom,
}

/// A tunable parameter in a recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeParameter {
    pub name: String,
    pub param_type: ParamType,
    pub default_value: serde_json::Value,
    pub description: String,
}

/// Parameter value type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    Int,
    Float,
    String,
    Bool,
}
