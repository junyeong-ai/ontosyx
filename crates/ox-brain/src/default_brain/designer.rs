//! `OntologyDesigner` impl for [`DefaultBrain`].

use std::collections::HashMap;

use async_trait::async_trait;
use tracing::info;

use ox_core::error::{OxError, OxResult};
use ox_ontology::ir::OntologyIR;
use ox_ontology::mapping::SourceId;

use crate::model_resolver::operation;
use crate::provider::structured_completion;
use crate::*;

#[async_trait]
impl OntologyDesigner for DefaultBrain {
    async fn design_ontology(
        &self,
        input: &DesignOntologyInput<'_>,
    ) -> OxResult<DesignOntologyOutput> {
        // Render every domain-context slice into a compact prompt
        // section. Empty slices produce empty strings so the prompt
        // collapses without conditional template syntax — no
        // leftover headers in the rendered prompt.
        let glossary_section = design::render_glossary_section(input.glossary_terms);
        let code_systems_section = design::render_code_systems_section(input.code_systems);
        let ambiguity_section = design::render_ambiguity_section(input.ambiguity_hints);
        let existing_ontology_section =
            design::render_existing_ontology_section(input.existing_ontology);

        let mut vars: HashMap<&str, &str> = HashMap::new();
        vars.insert("sample_data", input.sample_data);
        vars.insert("context", input.context);
        vars.insert("glossary_section", glossary_section.as_str());
        vars.insert("code_systems_section", code_systems_section.as_str());
        vars.insert("ambiguity_section", ambiguity_section.as_str());
        vars.insert(
            "existing_ontology_section",
            existing_ontology_section.as_str(),
        );

        info!(
            has_domain_context = input.has_domain_context(),
            glossary_terms = input.glossary_terms.len(),
            code_systems = input.code_systems.len(),
            ambiguity_hints = input.ambiguity_hints.len(),
            extending = input.existing_ontology.is_some(),
            "Designing ontology from sample data + domain context",
        );

        // `call_structured_traced` returns the parsed output plus
        // the call's `CallProvenance` — single registry + resolver
        // round-trip, no post-hoc lookup that could drift behind a
        // concurrent admin update. The provenance folds straight
        // into the artifact-side envelope.
        let (llm_output, call): (design::LlmDesignOutput, _) = self
            .call_structured_traced(
                operation::DESIGN_ONTOLOGY,
                Some("1.0.0"),
                operation::DESIGN_ONTOLOGY,
                &vars,
                "Designing ontology from sample data",
            )
            .await?;
        let provenance = call.into_artifact_provenance();

        let raw_input = design::into_input_ontology(llm_output);

        let norm_result =
            ox_ontology::input::normalize(raw_input, input.source_id).map_err(|errors| {
                OxError::Ontology {
                    message: format!(
                        "LLM-generated ontology normalization failed: {}",
                        ox_core::join_messages(&errors, "; ")
                    ),
                }
            })?;
        let ontology = norm_result.ontology;

        // validate() is already called inside normalize(), but keep explicit validation
        // as a safety net
        let errors = ontology.validate();
        if !errors.is_empty() {
            return Err(OxError::Ontology {
                message: format!(
                    "LLM-generated ontology has validation errors: {}",
                    ox_core::join_messages(&errors, "; ")
                ),
            });
        }

        Ok(DesignOntologyOutput {
            ontology,
            provenance,
        })
    }

    async fn resolve_design_provenance(
        &self,
        prompt_name: &str,
        operation: &str,
    ) -> OxResult<ox_ontology::source_mapping::ArtifactProvenance> {
        let tmpl = self.prompts.get(prompt_name)?;
        let resolved = self.model_resolver.resolve(operation).await?;
        let max_tokens = resolved.max_tokens.unwrap_or(tmpl.max_tokens);
        let temperature = resolved.temperature.or(tmpl.temperature);
        // No render hash on this path — the caller resolves
        // provenance ahead of (or independent of) the actual LLM
        // call, so there is no rendered prompt body to hash. The
        // batch path that *does* emit prompts records its render
        // hash through `call_structured` like the other LLM-driven
        // operations.
        Ok(CallProvenance {
            prompt_id: prompt_name.to_string(),
            prompt_version: tmpl.version.clone(),
            provider: resolved.provider,
            model_id: resolved.model_id,
            max_tokens,
            temperature,
            prompt_render_hash: String::new(),
        }
        .into_artifact_provenance())
    }

    async fn design_ontology_batch(
        &self,
        batch_data: &str,
        context: &str,
        existing_nodes: &str,
        cross_fks: &str,
    ) -> OxResult<ox_ontology::input::InputOntologyDef> {
        let base_prompt = self.prompts.get(operation::DESIGN_ONTOLOGY)?;
        let batch_tmpl = self.prompts.checked_for("design_ontology_batch", "1.0.0")?;

        // Inject full base instructions — token budget is safe after profile compression
        let system = batch_tmpl
            .system
            .replace("{{base_instructions}}", &base_prompt.system);

        let mut vars = HashMap::new();
        vars.insert("existing_nodes", existing_nodes);
        vars.insert("cross_fks", cross_fks);
        vars.insert("sample_data", batch_data);
        vars.insert("context", context);
        let user_prompt = batch_tmpl.render_user(&vars);

        let (client, resolved) = self
            .resolve_for_operation(operation::DESIGN_ONTOLOGY)
            .await?;
        info!(
            model = %resolved.model_id,
            prompt_version = %batch_tmpl.version,
            "Designing ontology batch (divide-and-conquer)"
        );

        let (llm_output, _usage): (design::LlmDesignOutput, _) = structured_completion(
            client.as_ref(),
            &resolved.model_id,
            &system,
            &user_prompt,
            resolved.max_tokens.unwrap_or(batch_tmpl.max_tokens),
            resolved.temperature.or(batch_tmpl.temperature),
        )
        .await?;
        Ok(design::into_input_ontology(llm_output))
    }

    async fn resolve_cross_edges(
        &self,
        node_labels: &str,
        existing_edges: &str,
        uncovered_fks: &str,
    ) -> OxResult<Vec<ox_ontology::InputEdgeTypeDef>> {
        let mut vars = HashMap::new();
        vars.insert("node_labels", node_labels);
        vars.insert("existing_edges", existing_edges);
        vars.insert("uncovered_fks", uncovered_fks);

        self.call_structured(
            operation::RESOLVE_CROSS_EDGES,
            Some("1.0.0"),
            operation::RESOLVE_CROSS_EDGES,
            &vars,
            "Resolving cross-domain edges",
        )
        .await
    }

    async fn refine_ontology(
        &self,
        ontology: &OntologyIR,
        refinement_context: &str,
        source_id: &SourceId,
    ) -> OxResult<OntologyIR> {
        let ontology_json = serialize_pretty(
            &ontology.to_agent_view(ox_core::llm_locale_fallback_default_tags()),
            "ontology",
        )?;

        let mut vars = HashMap::new();
        vars.insert("ontology", ontology_json.as_str());
        vars.insert("refinement_context", refinement_context);

        let llm_output: design::LlmDesignOutput = self
            .call_structured(
                operation::REFINE_ONTOLOGY,
                Some("1.0.0"),
                operation::REFINE_ONTOLOGY,
                &vars,
                "Refining ontology metadata",
            )
            .await?;
        let input = design::into_input_ontology(llm_output);

        let norm_result = ox_ontology::input::normalize(input, source_id).map_err(|errors| {
            OxError::Ontology {
                message: format!(
                    "Refined ontology normalization failed: {}",
                    ox_core::join_messages(&errors, "; ")
                ),
            }
        })?;
        let refined = norm_result.ontology;

        let errors = refined.validate();
        if !errors.is_empty() {
            return Err(OxError::Ontology {
                message: format!(
                    "Refined ontology has validation errors: {}",
                    ox_core::join_messages(&errors, "; ")
                ),
            });
        }

        Ok(refined)
    }
}
