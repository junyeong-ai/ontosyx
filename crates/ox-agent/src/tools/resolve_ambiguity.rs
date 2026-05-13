//! `resolve_ambiguity` agent tool — closes the detector-resolver loop.
//!
//! The source analyzer publishes [`ox_ontology::ambiguity::AmbiguityContext`]
//! rows into `ambiguity_contexts`. When the agent (or the admin via UI)
//! decides what the codes mean, it calls this tool with a structured
//! [`ResolveAmbiguityInput`] that names either a value-map, an existing
//! CodeSystem, or a Concept. The resulting
//! [`ox_ontology::ambiguity::AmbiguityResolution`] becomes the active
//! interpretation on the next query-path lookup.

use std::sync::Arc;

use async_trait::async_trait;
use entelix::tools::ToolEffect;
use entelix::{AgentContext, SchemaTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_ontology::ambiguity::{AmbiguityId, AmbiguityMapping, AmbiguityResolution, ValueMapEntry};
use ox_ontology::code_system::CodeSystemId;
use ox_ontology::concept::ConceptId;
use ox_store::AmbiguityStore;

/// Input DTO the LLM produces. One of `value_map`, `code_system_id`,
/// or `concept_id` must be set — serde enforces the shape via
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
    ValueMap {
        entries: Vec<ResolveAmbiguityValueEntry>,
    },
    /// Promote the column to an existing CodeSystem — the column's
    /// raw values are codes in that system.
    CodeSystemRef { code_system_id: String },
    /// Pin a canonical Concept on this source specifically.
    ConceptRef { concept_id: String },
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResolveAmbiguityValueEntry {
    pub value: String,
    pub display: String,
    #[serde(default)]
    pub definition: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResolveAmbiguityOutput {
    active: bool,
}

pub struct ResolveAmbiguityTool {
    pub ambiguity_store: Arc<dyn AmbiguityStore>,
    /// Shared "thread → last-resolve-ts" map. A successful resolution
    /// stamps the caller's `ctx.thread_id()`; the next `query_graph`
    /// invocation in the same thread reads the stamp within a ~10
    /// minute window to flip the `ambiguity_was_clarified` signal
    /// that drives the `clarification_success_rate` tile.
    pub clarification_tracker: crate::clarification_tracker::SharedClarificationTracker,
}

#[async_trait]
impl SchemaTool for ResolveAmbiguityTool {
    type Input = ResolveAmbiguityInput;
    type Output = ResolveAmbiguityOutput;
    const NAME: &'static str = super::RESOLVE_AMBIGUITY;

    fn description(&self) -> &str {
        "Record an interpretation for an ambiguous source column the detector flagged. \
         Provide the context_id (from the source analysis report or from an \
         `ambiguity` QueryDiagnostic) plus a mapping — either a `value_map` with \
         explicit value→display entries, a `code_system_ref` pointing at an \
         existing CodeSystem, or a `concept_ref` pointing at a canonical Concept. \
         Creating a new resolution revokes any previously active one on the same \
         context; the chain is preserved for audit."
    }

    fn effect(&self) -> ToolEffect {
        ToolEffect::Mutating
    }

    async fn execute(
        &self,
        input: Self::Input,
        ctx: &AgentContext<()>,
    ) -> entelix::Result<Self::Output> {
        let context_id = AmbiguityId::new(input.context_id.clone());
        // Verify context exists up-front so we give the caller a clear
        // "no such context" error instead of a FK violation on insert.
        let ambig_ctx = self
            .ambiguity_store
            .get_ambiguity_context(&context_id)
            .await
            .map_err(|e| {
                entelix::Error::invalid_request(format!(
                    "Failed to look up ambiguity context: {e:?}"
                ))
            })?
            .ok_or_else(|| {
                entelix::Error::invalid_request(format!(
                    "No ambiguity context found for id `{}`. Call \
                     `introspect_source` or consult the analysis report to find \
                     the correct context_id.",
                    input.context_id
                ))
            })?;

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
            ResolveAmbiguityMapping::ConceptRef { concept_id } => AmbiguityMapping::ConceptRef {
                concept_id: ConceptId::new(concept_id),
            },
        };

        let resolution = AmbiguityResolution::new(
            context_id.clone(),
            ambig_ctx.detection_source_hash.clone(),
            mapping,
        );

        let saved = self
            .ambiguity_store
            .create_ambiguity_resolution(resolution)
            .await
            .map_err(|e| {
                entelix::Error::invalid_request(format!("Failed to save resolution: {e:?}"))
            })?;

        // Stamp "this thread just clarified" so the next `query_graph`
        // in the same conversation counts toward the
        // clarification_success_rate tile. Falls back to the run id
        // when no thread is bound (single-shot dispatch).
        let scope_id = ctx.thread_id().or_else(|| ctx.run_id());
        self.clarification_tracker.record(scope_id);

        Ok(ResolveAmbiguityOutput {
            active: saved.revoked_at.is_none(),
        })
    }
}
