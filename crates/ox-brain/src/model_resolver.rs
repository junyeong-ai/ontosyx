use std::collections::HashMap;

use async_trait::async_trait;

use ox_core::error::OxResult;

use crate::auth::LlmProviderConfig;

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
    pub provider_config: Option<LlmProviderConfig>,
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

#[derive(Debug, Clone, Copy)]
pub struct ModelOperationSpec {
    pub key: &'static str,
    pub tier: &'static str,
    pub description: &'static str,
}

pub mod operation {
    pub const CHAT: &str = "chat";
    pub const DESIGN_ONTOLOGY: &str = "design_ontology";
    pub const REFINE_ONTOLOGY: &str = "refine_ontology";
    pub const RESOLVE_CROSS_EDGES: &str = "resolve_cross_edges";
    pub const EDIT_ONTOLOGY: &str = "edit_ontology";
    pub const TRANSLATE_QUERY: &str = "translate_query";
    pub const PLAN_LOAD: &str = "plan_load";
    pub const SELECT_WIDGET: &str = "select_widget";
    pub const EXPLAIN: &str = "explain";
    pub const SUGGEST_INSIGHTS: &str = "suggest_insights";
    pub const REPO_NAVIGATE: &str = "repo_navigate";
    pub const REPO_ANALYZE: &str = "repo_analyze";
    pub const EVALUATION_JUDGE: &str = "evaluation_judge";
    pub const EVALUATION_SAFETY_JUDGE: &str = "evaluation_safety_judge";
    pub const COMMUNITY_SUMMARY: &str = "community_summary";
}

/// Operations that default to the "fast" (cheap) model tier.
pub const FAST_OPERATIONS: &[&str] = &[
    operation::PLAN_LOAD,
    operation::SELECT_WIDGET,
    operation::EXPLAIN,
    operation::SUGGEST_INSIGHTS,
    operation::REPO_NAVIGATE,
    operation::COMMUNITY_SUMMARY,
];

/// First-class LLM operation keys used for routing, metering, evaluation axes,
/// and admin UI suggestions. The routing API still accepts future lowercase
/// stable keys; this registry documents operations the platform emits today.
pub const KNOWN_OPERATIONS: &[ModelOperationSpec] = &[
    ModelOperationSpec {
        key: operation::CHAT,
        tier: "primary",
        description: "Interactive agent conversation and tool orchestration",
    },
    ModelOperationSpec {
        key: operation::DESIGN_ONTOLOGY,
        tier: "primary",
        description: "Ontology draft generation from source analysis",
    },
    ModelOperationSpec {
        key: operation::REFINE_ONTOLOGY,
        tier: "primary",
        description: "Ontology refinement and reconciliation",
    },
    ModelOperationSpec {
        key: operation::RESOLVE_CROSS_EDGES,
        tier: "primary",
        description: "Cross-source edge inference during ontology design",
    },
    ModelOperationSpec {
        key: operation::EDIT_ONTOLOGY,
        tier: "primary",
        description: "Natural-language ontology edit command generation",
    },
    ModelOperationSpec {
        key: operation::TRANSLATE_QUERY,
        tier: "primary",
        description: "Natural language to QueryIR translation",
    },
    ModelOperationSpec {
        key: operation::PLAN_LOAD,
        tier: "fast",
        description: "Load-plan generation from ontology and source schema",
    },
    ModelOperationSpec {
        key: operation::SELECT_WIDGET,
        tier: "fast",
        description: "Result visualization hint selection",
    },
    ModelOperationSpec {
        key: operation::EXPLAIN,
        tier: "fast",
        description: "Short natural-language result explanation",
    },
    ModelOperationSpec {
        key: operation::SUGGEST_INSIGHTS,
        tier: "fast",
        description: "Proactive insight suggestion generation",
    },
    ModelOperationSpec {
        key: operation::REPO_NAVIGATE,
        tier: "fast",
        description: "Repository file-tree selection for ontology design",
    },
    ModelOperationSpec {
        key: operation::REPO_ANALYZE,
        tier: "primary",
        description: "Repository file analysis for domain insights",
    },
    ModelOperationSpec {
        key: operation::EVALUATION_JUDGE,
        tier: "primary",
        description: "LLM-as-judge scoring for evaluation cases",
    },
    ModelOperationSpec {
        key: operation::EVALUATION_SAFETY_JUDGE,
        tier: "primary",
        description: "Safety-axis LLM-as-judge scoring for evaluation cases",
    },
    ModelOperationSpec {
        key: operation::COMMUNITY_SUMMARY,
        tier: "fast",
        description: "GraphRAG community-summary prose for the offline detection cron",
    },
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
            provider_config: Some(primary.clone()),
        };
        let fast_resolved = fast
            .map(|f| ResolvedModel {
                provider: f.provider.clone(),
                model_id: f.model.clone(),
                max_tokens: None,
                temperature: None,
                provider_config: Some(f.clone()),
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

#[cfg(test)]
mod tests {
    use super::{FAST_OPERATIONS, KNOWN_OPERATIONS};
    use std::collections::HashSet;

    #[test]
    fn known_operations_are_unique() {
        let mut seen = HashSet::new();
        for op in KNOWN_OPERATIONS {
            assert!(seen.insert(op.key), "duplicate operation key: {}", op.key);
        }
    }

    #[test]
    fn fast_operation_registry_matches_static_fallback() {
        for fast_key in FAST_OPERATIONS {
            let spec = KNOWN_OPERATIONS
                .iter()
                .find(|op| op.key == *fast_key)
                .unwrap_or_else(|| panic!("missing fast operation in registry: {fast_key}"));
            assert_eq!(spec.tier, "fast");
        }

        for op in KNOWN_OPERATIONS.iter().filter(|op| op.tier == "fast") {
            assert!(
                FAST_OPERATIONS.contains(&op.key),
                "registry marks operation as fast without static fallback: {}",
                op.key
            );
        }
    }
}
