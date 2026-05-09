//! `OntologyEditor` impl for [`DefaultBrain`] + the
//! `EditCommandsResponse` LLM structured-output payload.

use std::collections::HashMap;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use ox_core::error::OxResult;
use ox_ontology::command::OntologyCommand;
use ox_ontology::ir::OntologyIR;

use crate::model_resolver::operation;
use crate::*;

// ---------------------------------------------------------------------------
// EditCommandsResponse — internal struct for LLM structured output
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
struct EditCommandsResponse {
    commands: Vec<OntologyCommand>,
    explanation: String,
}

#[async_trait]
impl OntologyEditor for DefaultBrain {
    async fn generate_edit_commands(
        &self,
        ontology: &OntologyIR,
        user_request: &str,
    ) -> OxResult<EditCommandsOutput> {
        let ontology_json = serialize_pretty(
            &ontology.to_agent_view(ox_core::llm_locale_fallback_default_tags()),
            "ontology",
        )?;

        let mut vars = HashMap::new();
        vars.insert("ontology", ontology_json.as_str());
        vars.insert("user_request", user_request);

        // `call_structured_traced` already captures the resolved
        // model id alongside the parsed output — no second
        // `model_resolver.resolve` round-trip that could drift behind
        // a concurrent admin update.
        let (response, call): (EditCommandsResponse, _) = self
            .call_structured_traced(
                operation::EDIT_ONTOLOGY,
                Some("1.0.0"),
                operation::EDIT_ONTOLOGY,
                &vars,
                "Generating ontology edit commands",
            )
            .await?;
        Ok(EditCommandsOutput {
            commands: response.commands,
            explanation: response.explanation,
            provider: call.provider,
            model: call.model_id,
        })
    }
}
