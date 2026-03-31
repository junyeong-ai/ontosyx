use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use ox_store::UsageSummary;

use crate::error::AppError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /api/usage — aggregated usage summary for a time range
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct UsageQuery {
    /// ISO 8601 datetime (defaults to 30 days ago)
    pub from: Option<String>,
    /// ISO 8601 datetime (defaults to now)
    pub to: Option<String>,
}

pub(crate) async fn get_usage_summary(
    State(state): State<AppState>,
    Query(params): Query<UsageQuery>,
) -> Result<Json<Vec<UsageSummary>>, AppError> {
    let from = params
        .from
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(30));
    let to = params
        .to
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(chrono::Utc::now);
    let summary = state
        .store
        .usage_summary(from, to)
        .await
        .map_err(AppError::from)?;
    Ok(Json(summary))
}
