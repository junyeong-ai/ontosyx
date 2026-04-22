//! `resolve_ambiguity` agent tool — closes the detector-resolver loop
//! the patent 1-pager promises.
//!
//! The source analyzer publishes [`ox_ontology::ambiguity::AmbiguityContext`]
//! rows into `ambiguity_contexts`. When the agent (or the admin via UI)
//! decides what the codes mean, it calls this tool with a structured
//! [`ResolveAmbiguityInput`] that names either a value-map, an existing
//! CodeSystem, or a Glossary term. The resulting
//! [`ox_ontology::ambiguity::AmbiguityResolution`] becomes the active
//! interpretation on the next query-path lookup.

use std::sync::Arc;

use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_ontology::ambiguity::{
    AmbiguityId, AmbiguityMapping, AmbiguityResolution, ValueMapEntry,
};
use ox_ontology::code_system::CodeSystemId;
use ox_ontology::glossary::GlossaryTermId;
use ox_store::AmbiguityStore;

/// Input DTO the LLM produces. One of `value_map`, `code_system_id`,
/// or `glossary_term_id` must be set — serde enforces the shape via
/// the typed `mapping` enum.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResolveAmbiguityInput {
    /// The `AmbiguityContext.id` (UUID) this resolution binds to.
    /// The agent reads the id from the source-analysis report or from
    /// an earlier `QueryDiagnostic { validator: "ambiguity" }`.
    pub context_id: String,
    /// Chosen interpretation. Discriminated union — see
    /// [`ResolveAmbiguityMapping`].
    pub mapping: ResolveAmbiguityMapping,
}

/// Wire form of [`AmbiguityMapping`]. Re-declared here so the
/// JsonSchema the LLM sees stays stable even if the internal IR enum
/// gains variants (only the variants we want the agent to author
/// appear here).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolveAmbiguityMapping {
    /// Enumerate each raw value with a display + optional definition.
    ValueMap { entries: Vec<ResolveAmbiguityValueEntry> },
    /// Promote the column to an existing CodeSystem — the column's
    /// raw values are codes in that system.
    CodeSystemRef { code_system_id: String },
    /// Pin a Glossary term on this source specifically.
    GlossaryRef { term_id: String },
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResolveAmbiguityValueEntry {
    pub value: String,
    pub display: String,
    #[serde(default)]
    pub definition: Option<String>,
}

#[derive(Debug, Serialize)]
struct ResolveAmbiguityOutput {
    resolution_id: String,
    context_id: String,
    supersedes: Option<String>,
    active: bool,
}

pub struct ResolveAmbiguityTool {
    pub ambiguity_store: Arc<dyn AmbiguityStore>,
}

#[async_trait]
impl SchemaTool for ResolveAmbiguityTool {
    type Input = ResolveAmbiguityInput;
    const NAME: &'static str = super::RESOLVE_AMBIGUITY;
    const DESCRIPTION: &'static str =
        "Record an interpretation for an ambiguous source column the detector flagged. \
         Provide the context_id (from the source analysis report or from an \
         `ambiguity` QueryDiagnostic) plus a mapping — either a `value_map` with \
         explicit value→display entries, a `code_system_ref` pointing at an \
         existing CodeSystem, or a `glossary_ref` pointing at a Glossary term. \
         Creating a new resolution revokes any previously active one on the same \
         context; the chain is preserved for audit.";
    const READ_ONLY: bool = false;

    async fn handle(&self, input: Self::Input, _ctx: &ExecutionContext) -> ToolResult {
        let context_id = AmbiguityId::new(input.context_id.clone());
        // Verify context exists up-front so we give the caller a clear
        // "no such context" error instead of a FK violation on insert.
        let ctx = match self.ambiguity_store.get_ambiguity_context(&context_id).await {
            Ok(Some(c)) => c,
            Ok(None) => {
                return ToolResult::error(format!(
                    "No ambiguity context found for id `{}`. Call \
                     `introspect_source` or consult the analysis report to find \
                     the correct context_id.",
                    input.context_id
                ));
            }
            Err(e) => {
                return ToolResult::error(format!(
                    "Failed to look up ambiguity context: {e:?}"
                ));
            }
        };

        let mapping = match input.mapping {
            ResolveAmbiguityMapping::ValueMap { entries } => AmbiguityMapping::ValueMap {
                entries: entries
                    .into_iter()
                    .map(|e| ValueMapEntry {
                        value: e.value,
                        display: e.display,
                        definition: e.definition,
                    })
                    .collect(),
            },
            ResolveAmbiguityMapping::CodeSystemRef { code_system_id } => {
                AmbiguityMapping::CodeSystemRef {
                    code_system_id: CodeSystemId::new(code_system_id),
                }
            }
            ResolveAmbiguityMapping::GlossaryRef { term_id } => AmbiguityMapping::GlossaryRef {
                term_id: GlossaryTermId::new(term_id),
            },
        };

        let resolution =
            AmbiguityResolution::new(context_id.clone(), ctx.detection_source_hash.clone(), mapping);

        match self
            .ambiguity_store
            .create_ambiguity_resolution(resolution)
            .await
        {
            Ok(saved) => ToolResult::success(
                serde_json::to_string_pretty(&ResolveAmbiguityOutput {
                    resolution_id: saved.id.as_str().to_string(),
                    context_id: saved.context_id.as_str().to_string(),
                    supersedes: saved.supersedes.map(|s| s.as_str().to_string()),
                    active: saved.revoked_at.is_none(),
                })
                .unwrap_or_default(),
            ),
            Err(e) => ToolResult::error(format!("Failed to save resolution: {e:?}")),
        }
    }
}
