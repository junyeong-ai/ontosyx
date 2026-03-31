use std::sync::Arc;

use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::info;

use crate::DomainContext;

// ---------------------------------------------------------------------------
// ApplyOntologyTool — generate + execute ontology edits in one step
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApplyOntologyInput {
    /// The edit request description — what changes to make.
    pub edit_request: String,
}

/// Applies ontology edits by delegating to Brain.generate_edit_commands + Store.save.
/// This is the "execute" counterpart to EditOntologyTool's "preview" mode.
pub struct ApplyOntologyTool {
    pub domain: Arc<DomainContext>,
    pub brain: Arc<dyn ox_brain::Brain>,
}

#[async_trait]
impl SchemaTool for ApplyOntologyTool {
    type Input = ApplyOntologyInput;
    const NAME: &'static str = super::APPLY_ONTOLOGY;
    const DESCRIPTION: &'static str =
        "Apply ontology edits directly to the current project. \
         Generates edit commands from the request, validates them, and saves the updated ontology. \
         Use this when the user wants to actually modify the ontology (not just preview changes). \
         Requires 'designer' role. Changes are saved to the project with a new revision.";

    async fn handle(&self, input: Self::Input, _ctx: &ExecutionContext) -> ToolResult {
        let ontology = match &self.domain.ontology {
            Some(o) => o,
            None => return ToolResult::error("No ontology loaded"),
        };
        let project_id = match self.domain.project_id {
            Some(id) => id,
            None => {
                return ToolResult::error(
                    "No project context — save the ontology to a project first",
                )
            }
        };
        let revision = match self.domain.project_revision {
            Some(r) => r,
            None => return ToolResult::error("No project revision"),
        };

        // Generate edit commands via Brain
        let edit_result = match self
            .brain
            .generate_edit_commands(ontology, &input.edit_request)
            .await
        {
            Ok(result) => result,
            Err(e) => return ToolResult::error(format!("Failed to generate edit commands: {e}")),
        };

        if edit_result.commands.is_empty() {
            return ToolResult::success(
                serde_json::json!({
                    "status": "no_changes",
                    "explanation": edit_result.explanation,
                })
                .to_string(),
            );
        }

        info!(
            project_id = %project_id,
            command_count = edit_result.commands.len(),
            "Applying ontology edit commands"
        );

        // Apply commands sequentially — each command validates against the current state
        let mut updated = ontology.clone();
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
            return ToolResult::error(format!(
                "All {} commands failed: {}",
                edit_result.commands.len(),
                errors.join("; ")
            ));
        }

        // Save to project store
        let ontology_json = match serde_json::to_value(&updated) {
            Ok(v) => v,
            Err(e) => return ToolResult::error(format!("Failed to serialize ontology: {e}")),
        };

        match self
            .domain
            .store
            .update_design_result(
                project_id,
                &ontology_json,
                None, // source_mapping unchanged
                None, // quality_report will be recomputed
                revision,
            )
            .await
        {
            Ok(()) => {
                info!(
                    project_id = %project_id,
                    applied = applied_count,
                    errors = errors.len(),
                    "Ontology edit applied and saved"
                );

                let output = serde_json::json!({
                    "status": "applied",
                    "commands_applied": applied_count,
                    "errors": errors,
                    "explanation": edit_result.explanation,
                    "new_node_count": updated.node_types.len(),
                    "new_edge_count": updated.edge_types.len(),
                });
                ToolResult::success(
                    serde_json::to_string_pretty(&output).unwrap_or_default(),
                )
            }
            Err(e) => ToolResult::error(format!("Failed to save ontology: {e}")),
        }
    }
}
