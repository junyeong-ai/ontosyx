use std::sync::Arc;

use async_trait::async_trait;
use entelix::tools::ToolEffect;
use entelix::{AgentContext, SchemaTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::DomainContext;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditOntologyInput {
    /// Natural language description of the desired ontology change.
    pub request: String,
}

#[derive(Debug, Serialize)]
pub struct EditOntologyOutput {
    commands: serde_json::Value,
    command_count: usize,
    explanation: String,
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
    type Output = EditOntologyOutput;
    const NAME: &'static str = super::EDIT_ONTOLOGY;

    fn description(&self) -> &str {
        "Generate atomic edit commands (add/remove/rename nodes, edges, properties, constraints, indexes). \
         Returns a preview; the user must approve before apply_ontology runs."
    }

    // Edit generation does not mutate the store — it returns a preview;
    // `apply_ontology` is the dedicated mutator. The edit step is
    // therefore read-only with respect to ontology state.
    fn effect(&self) -> ToolEffect {
        ToolEffect::ReadOnly
    }

    async fn execute(
        &self,
        input: Self::Input,
        ctx: &AgentContext<()>,
    ) -> entelix::Result<Self::Output> {
        let ontology = self.domain.current_ontology().ok_or_else(|| {
            entelix::Error::invalid_request(
                "No ontology loaded. Create an ontology draft from a data source first.",
            )
        })?;

        let output = self
            .brain
            .generate_edit_commands(&ontology, &input.request, ctx.core())
            .await
            .map_err(|e| entelix::Error::invalid_request(format!("Edit generation failed: {e}")))?;

        let command_count = output.commands.len();
        info!(
            request = %input.request,
            commands = command_count,
            "Edit commands generated"
        );

        Ok(EditOntologyOutput {
            commands: serde_json::to_value(&output.commands).unwrap_or_default(),
            command_count,
            explanation: output.explanation,
        })
    }
}
