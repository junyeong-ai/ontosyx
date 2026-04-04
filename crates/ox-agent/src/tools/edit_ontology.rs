use std::sync::Arc;

use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::DomainContext;

// ---------------------------------------------------------------------------
// EditOntologyTool — generate atomic OntologyCommand operations
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditOntologyInput {
    /// Natural language description of the desired ontology change.
    pub request: String,
}

#[derive(Debug, Serialize)]
struct EditOntologyOutput {
    commands: serde_json::Value,
    explanation: String,
    command_count: usize,
}

/// Generates surgical OntologyCommand operations from a natural language edit request.
/// Returns a preview of commands with explanations; the user decides whether to apply.
pub struct EditOntologyTool {
    pub domain: Arc<DomainContext>,
    pub brain: Arc<dyn ox_brain::Brain>,
}

#[async_trait]
impl SchemaTool for EditOntologyTool {
    type Input = EditOntologyInput;
    const NAME: &'static str = super::EDIT_ONTOLOGY;
    const DESCRIPTION: &'static str = "Generate atomic edit commands to modify the ontology structure. \
         Supports adding/removing/renaming nodes, edges, properties, constraints, and indexes. \
         Returns a preview of commands — the user must approve before applying.";

    async fn handle(&self, input: Self::Input, _ctx: &ExecutionContext) -> ToolResult {
        let ontology = match self.domain.ontology.as_ref() {
            Some(o) => o,
            None => return ToolResult::error(
                "No ontology loaded. Create a project from a data source first."
            ),
        };

        let output = match self
            .brain
            .generate_edit_commands(ontology, &input.request)
            .await
        {
            Ok(o) => o,
            Err(e) => return ToolResult::error(format!("Edit generation failed: {e}")),
        };

        info!(
            request = %input.request,
            commands = output.commands.len(),
            "Edit commands generated"
        );

        let result = EditOntologyOutput {
            command_count: output.commands.len(),
            commands: serde_json::to_value(&output.commands).unwrap_or_default(),
            explanation: output.explanation,
        };

        ToolResult::success(serde_json::to_string_pretty(&result).unwrap_or_default())
    }
}
