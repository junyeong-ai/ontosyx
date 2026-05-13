use std::sync::Arc;

use async_trait::async_trait;
use entelix::tools::ToolEffect;
use entelix::{AgentContext, SchemaTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::DomainContext;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApplyOntologyInput {
    /// The edit request description — what changes to make.
    pub edit_request: String,
}

/// Outcome of an `apply_ontology` invocation. The `status` discriminator
/// drives the LLM's next decision: `"applied"` reports the new node /
/// edge counts so the model can answer follow-up questions against the
/// updated schema; `"no_changes"` lets the model continue without a
/// pointless retry.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ApplyOntologyOutput {
    /// The edit produced zero commands — the request was either a
    /// no-op or the brain refused to generate commands. The
    /// explanation gives the LLM enough signal to decide whether to
    /// retry with a sharper request.
    NoChanges { explanation: String },
    /// One or more commands applied successfully. `commands_applied`
    /// is the count that landed; `errors` carries the per-command
    /// rejection messages for the ones that did not.
    Applied {
        commands_applied: usize,
        errors: Vec<String>,
        explanation: String,
        new_node_count: usize,
        new_edge_count: usize,
    },
}

/// Applies ontology edits by delegating to `Brain::generate_edit_commands`
/// and persisting the result. The "execute" counterpart to
/// `EditOntologyTool`'s "preview" mode — `EditOntologyTool` returns
/// commands without saving; this tool runs them and saves.
pub struct ApplyOntologyTool {
    pub domain: Arc<DomainContext>,
    pub brain: Arc<dyn ox_brain::Brain>,
}

#[async_trait]
impl SchemaTool for ApplyOntologyTool {
    type Input = ApplyOntologyInput;
    type Output = ApplyOntologyOutput;
    const NAME: &'static str = super::APPLY_ONTOLOGY;

    fn description(&self) -> &str {
        "Apply ontology edits directly to the current ontology draft (not a preview). \
         Creates a new revision; requires designer role."
    }

    fn effect(&self) -> ToolEffect {
        ToolEffect::Mutating
    }

    async fn execute(
        &self,
        input: Self::Input,
        ctx: &AgentContext<()>,
    ) -> entelix::Result<Self::Output> {
        let ontology = self
            .domain
            .current_ontology()
            .ok_or_else(|| entelix::Error::invalid_request("No ontology loaded"))?;
        let ontology_draft_id = self.domain.ontology_draft_id.ok_or_else(|| {
            entelix::Error::invalid_request(
                "No ontology draft context — save the ontology to a draft first",
            )
        })?;
        let revision = self
            .domain
            .ontology_draft_revision
            .ok_or_else(|| entelix::Error::invalid_request("No ontology draft revision"))?;

        let edit_result = self
            .brain
            .generate_edit_commands(&ontology, &input.edit_request, ctx.core())
            .await
            .map_err(|e| {
                entelix::Error::invalid_request(format!("Failed to generate edit commands: {e}"))
            })?;

        if edit_result.commands.is_empty() {
            return Ok(ApplyOntologyOutput::NoChanges {
                explanation: edit_result.explanation,
            });
        }

        info!(
            ontology_draft_id = %ontology_draft_id,
            command_count = edit_result.commands.len(),
            "Applying ontology edit commands"
        );

        // Apply commands sequentially — each command validates against
        // the current state before mutating; per-command rejections
        // accumulate so the LLM sees the full failure surface.
        let total_commands = edit_result.commands.len();
        let mut updated = (*ontology).clone();
        let mut applied_count = 0;
        let mut errors = Vec::new();
        for (i, cmd) in edit_result.commands.iter().enumerate() {
            match cmd.execute(&updated) {
                Ok(result) => {
                    updated = result.new_ontology;
                    applied_count += 1;
                }
                Err(e) => errors.push(format!("Command {} failed: {e}", i + 1)),
            }
        }

        if applied_count == 0 {
            return Err(entelix::Error::invalid_request(format!(
                "All {} commands failed: {}",
                total_commands,
                errors.join("; ")
            )));
        }

        let ontology_json = serde_json::to_value(&updated).map_err(|e| {
            entelix::Error::invalid_request(format!("Failed to serialize ontology: {e}"))
        })?;

        self.domain
            .store
            .update_design_result(ontology_draft_id, &ontology_json, None, revision)
            .await
            .map_err(|e| {
                entelix::Error::invalid_request(format!("Failed to save ontology: {e}"))
            })?;

        // Publish the new ontology into the shared `DomainContext` slot
        // so downstream tools (query_graph, explain, …) see the edits
        // without a session restart. The `GRAPH_ONTOLOGY` task-local
        // picks up the fresh snapshot on the next `runtime.execute_query`
        // wrap.
        self.domain.replace_ontology(updated.clone());
        info!(
            ontology_draft_id = %ontology_draft_id,
            applied = applied_count,
            errors = errors.len(),
            "Ontology edit applied and saved"
        );

        let new_node_count = updated.node_types().len();
        let new_edge_count = updated.edge_types().len();
        Ok(ApplyOntologyOutput::Applied {
            commands_applied: applied_count,
            errors,
            explanation: edit_result.explanation,
            new_node_count,
            new_edge_count,
        })
    }
}
