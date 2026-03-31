use std::collections::HashMap;

use async_trait::async_trait;

use ox_core::error::OxResult;

// ---------------------------------------------------------------------------
// ModelResolver — per-operation model resolution abstraction
// ---------------------------------------------------------------------------

/// Resolves which LLM model to use for a given operation.
///
/// Implementations may resolve from:
/// - Static config (tests, simple deployments)
/// - Database routing rules (production, per-workspace)
///
/// workspace_id is NOT exposed in the trait — implementations read it
/// from task-locals or other context, keeping Brain decoupled from tenancy.
#[async_trait]
pub trait ModelResolver: Send + Sync {
    async fn resolve(&self, operation: &str) -> OxResult<ResolvedModel>;
}

/// Result of model resolution.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub provider: String,
    pub model_id: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

// ---------------------------------------------------------------------------
// StaticModelResolver — fixed model mapping (tests, simple deployments)
// ---------------------------------------------------------------------------

/// Maps operations to models using a static map.
///
/// Operations not in the map fall through to either the "fast" model
/// (for known-cheap operations) or the "primary" model.
pub struct StaticModelResolver {
    primary: ResolvedModel,
    fast: ResolvedModel,
    overrides: HashMap<String, ResolvedModel>,
}

/// Operations that default to the "fast" (cheap) model tier.
const FAST_OPERATIONS: &[&str] = &[
    "plan_load",
    "select_widget",
    "explain",
    "suggest_insights",
    "repo_navigate",
];

impl StaticModelResolver {
    pub fn new(primary: ResolvedModel, fast: ResolvedModel) -> Self {
        Self {
            primary,
            fast,
            overrides: HashMap::new(),
        }
    }

    /// Override the model for a specific operation.
    pub fn with_operation(mut self, operation: &str, model: ResolvedModel) -> Self {
        self.overrides.insert(operation.to_string(), model);
        self
    }

    /// Create from LlmProviderConfig (primary + optional fast).
    pub fn from_configs(
        primary: &crate::auth::LlmProviderConfig,
        fast: Option<&crate::auth::LlmProviderConfig>,
    ) -> Self {
        let primary_resolved = ResolvedModel {
            provider: primary.provider.clone(),
            model_id: primary.model.clone(),
            max_tokens: None,
            temperature: None,
        };
        let fast_resolved = fast
            .map(|f| ResolvedModel {
                provider: f.provider.clone(),
                model_id: f.model.clone(),
                max_tokens: None,
                temperature: None,
            })
            .unwrap_or_else(|| primary_resolved.clone());

        Self::new(primary_resolved, fast_resolved)
    }
}

#[async_trait]
impl ModelResolver for StaticModelResolver {
    async fn resolve(&self, operation: &str) -> OxResult<ResolvedModel> {
        // Explicit override first
        if let Some(model) = self.overrides.get(operation) {
            return Ok(model.clone());
        }

        // Fast tier for known-cheap operations
        if FAST_OPERATIONS.contains(&operation) {
            return Ok(self.fast.clone());
        }

        // Everything else uses primary
        Ok(self.primary.clone())
    }
}
