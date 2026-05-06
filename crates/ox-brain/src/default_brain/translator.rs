//! `QueryTranslator` impl for [`DefaultBrain`].

use std::collections::HashMap;

use async_trait::async_trait;
use tracing::info;

use ox_core::error::{OxError, OxResult};
use ox_core::source_schema::SourceSchema;
use ox_ontology::ir::OntologyIR;
use ox_ontology::load_plan::LoadPlan;
use ox_query_ir::query::QueryIR;

use crate::*;

#[async_trait]
impl QueryTranslator for DefaultBrain {
    async fn translate_query(
        &self,
        question: &str,
        ontology: &OntologyIR,
        retrieved_context: Option<&str>,
        ctx: &branchforge::ExecutionContext,
    ) -> OxResult<QueryIR> {
        // Schema discovery
        ctx.progress("schema_discovery").started();
        let t_schema = std::time::Instant::now();

        // RAG is only used when (a) the ontology is large enough to need
        // schema selection and (b) a vector memory backend is available.
        // Binding `memory` here removes the redundant `.unwrap()`.
        let rag_memory = self
            .memory
            .as_ref()
            .filter(|_| ontology.node_types().len() > schema_rag::FULL_SCHEMA_NODE_THRESHOLD);

        let (ontology_json, discovered_labels) = if let Some(memory) = rag_memory {
            let oid = self.ontology_lineage_id.as_deref().unwrap_or(&ontology.id);
            schema_rag::discover_schema(memory, ontology, question, oid).await
        } else {
            let all_node_labels: Vec<&str> = ontology
                .node_types()
                .iter()
                .map(|n| n.label.as_str())
                .collect();
            let all_label_strings: Vec<String> = ontology
                .node_types()
                .iter()
                .map(|n| n.label.to_string())
                .chain(ontology.edge_types().iter().map(|e| e.label.to_string()))
                .collect();
            let schema = schema_rag::build_progressive_schema(ontology, &all_node_labels);
            (schema, all_label_strings)
        };

        ctx.progress("schema_discovery")
            .completed(t_schema.elapsed().as_millis() as u64);

        // Knowledge RAG
        ctx.progress("knowledge_lookup").started();
        let t_knowledge = std::time::Instant::now();
        let label_refs: Vec<&str> = discovered_labels.iter().map(|s| s.as_str()).collect();
        let knowledge_context = if let Some(kb) = &self.knowledge_store {
            knowledge_rag::discover_knowledge(
                kb.as_ref(),
                &label_refs,
                self.ontology_lineage_id
                    .as_deref()
                    .unwrap_or(&ontology.name),
                ontology.version.number as i32,
                8,
            )
            .await
        } else {
            String::new()
        };

        ctx.progress("knowledge_lookup")
            .completed(t_knowledge.elapsed().as_millis() as u64);

        let glossary_section =
            crate::design::render_glossary_section(ontology.glossary());

        let mut vars = HashMap::new();
        vars.insert("question", question);
        vars.insert("ontology", ontology_json.as_str());
        vars.insert("glossary_section", glossary_section.as_str());
        vars.insert("knowledge", knowledge_context.as_str());
        // Empty placeholder by default; the label-retry path below
        // overrides it with the offending labels. Keeps `{{correction}}`
        // from leaking into the LLM prompt as a literal token on the
        // happy path (PromptTemplate::render_user does plain string
        // replacement — unmatched placeholders survive).
        vars.insert("correction", "");
        // GraphRAG-rendered subgraph slice. When the agent walks
        // `OntologyNavigationStore::search_entry_points → expand_neighbors
        // → render_subgraph_for_llm` and threads the markdown in,
        // the prompt template surfaces it as a "## Retrieved
        // subgraph" block alongside the schema RAG snippet —
        // gives the LLM anchor-expanded ontology context the
        // schema RAG alone misses (Postgres-backed Level-3 indexes
        // beat the in-memory `discover_schema` heuristic on
        // ontologies past a few hundred node types).
        vars.insert(
            "ontology_subgraph_md",
            retrieved_context.unwrap_or(""),
        );

        // Primary LLM call (StructuredMatchQuery structured output)
        ctx.progress("llm_primary").started();
        let t_llm = std::time::Instant::now();
        let query_ir = match self
            .call_structured::<ox_query_ir::StructuredMatchQuery>(
                "translate_match_query",
                Some("1.0.0"),
                "translate_match_query",
                &vars,
                "Translating to StructuredMatchQuery (structured output)",
            )
            .await
            .and_then(|match_ir| match_ir.into_query_ir())
        {
            Ok(qir) => {
                ctx.progress("llm_primary")
                    .completed(t_llm.elapsed().as_millis() as u64);
                info!("StructuredMatchQuery structured output succeeded");
                qir
            }
            Err(match_err) => {
                ctx.progress("llm_primary").failed_with(t_llm.elapsed().as_millis() as u64,
                    serde_json::json!({ "error": format!("{match_err}").chars().take(200).collect::<String>() }));

                // Fallback LLM call (full QueryIR JSON mode)
                ctx.progress("llm_fallback").started();
                let t_fallback = std::time::Instant::now();
                info!(
                    error = %match_err,
                    "StructuredMatchQuery path failed, falling back to full QueryIR"
                );
                let result: OxResult<QueryIR> = self
                    .call_structured(
                        "translate_query",
                        Some("1.0.0"),
                        "translate_query",
                        &vars,
                        "Translating to QueryIR (JSON mode fallback)",
                    )
                    .await;

                match result {
                    Ok(qir) => {
                        ctx.progress("llm_fallback")
                            .completed(t_fallback.elapsed().as_millis() as u64);
                        qir
                    }
                    Err(first_err) => {
                        ctx.progress("llm_fallback")
                            .failed(t_fallback.elapsed().as_millis() as u64);
                        // Final retry
                        ctx.progress("llm_retry").started();
                        let t_retry = std::time::Instant::now();
                        info!(
                            error = %first_err,
                            "QueryIR translation failed, retrying once"
                        );
                        let retry_result = self
                            .call_structured::<QueryIR>(
                                "translate_query",
                                Some("1.0.0"),
                                "translate_query",
                                &vars,
                                "Retrying query translation",
                            )
                            .await
                            .map_err(|retry_err| {
                                ctx.progress("llm_retry")
                                    .failed(t_retry.elapsed().as_millis() as u64);
                                info!(
                                    first_error = %first_err,
                                    retry_error = %retry_err,
                                    "Query translation retry also failed"
                                );
                                retry_err
                            });
                        if retry_result.is_ok() {
                            ctx.progress("llm_retry")
                                .completed(t_retry.elapsed().as_millis() as u64);
                        }
                        retry_result?
                    }
                }
            }
        };

        // Pre-flight label validation. The runtime's OntologyValidator is
        // the final authority (operates on the AST, catches inline
        // property keys too), but a cheap QueryIR-level check here
        // short-circuits the agent-level retry when the LLM hallucinates
        // a label. If we spot unknown labels, retry once with the
        // offending labels listed in the prompt context. If the retry
        // still produces unknowns, surface them to the runtime — it will
        // reject consistently, so the agent can still learn via the
        // tool-error path.
        let query_ir = match ox_query_ir::unknown_labels_in_query(ontology, &query_ir) {
            unknown if unknown.is_empty() => query_ir,
            unknown => {
                ctx.progress("llm_label_retry").started();
                let t_label_retry = std::time::Instant::now();
                info!(
                    unknown_labels = ?unknown,
                    "LLM returned unknown labels; retrying translate with explicit schema context",
                );
                // Enrich the prompt variables with the specific unknown
                // labels the last attempt produced — the LLM tends to
                // correct itself when given the exact violation.
                let correction = format!(
                    "Previous attempt referenced labels that do not exist in the ontology: \
                     {}. Use only labels listed in the schema above.",
                    unknown.join(", "),
                );
                let mut retry_vars = vars.clone();
                retry_vars.insert("correction", correction.as_str());
                let retry: OxResult<QueryIR> = self
                    .call_structured(
                        "translate_query",
                        Some("1.0.0"),
                        "translate_query",
                        &retry_vars,
                        "Retrying query translation with label correction",
                    )
                    .await;
                match retry {
                    Ok(qir) => {
                        let still_unknown =
                            ox_query_ir::unknown_labels_in_query(ontology, &qir);
                        if !still_unknown.is_empty() {
                            ctx.progress("llm_label_retry")
                                .failed(t_label_retry.elapsed().as_millis() as u64);
                            let available: Vec<&str> = ontology
                                .node_types()
                                .iter()
                                .map(|n| n.label.as_str())
                                .chain(ontology.edge_types().iter().map(|e| e.label.as_str()))
                                .collect();
                            return Err(OxError::Validation {
                                field: "labels".to_string(),
                                message: format!(
                                    "Query references labels that do not exist in the ontology \
                                     even after correction: {}. Available labels: {}.",
                                    still_unknown.join(", "),
                                    available.join(", ")
                                ),
                            });
                        }
                        ctx.progress("llm_label_retry")
                            .completed(t_label_retry.elapsed().as_millis() as u64);
                        qir
                    }
                    Err(_) => {
                        ctx.progress("llm_label_retry")
                            .failed(t_label_retry.elapsed().as_millis() as u64);
                        return Err(OxError::Validation {
                            field: "labels".to_string(),
                            message: format!(
                                "Query references labels that do not exist in the ontology \
                                 ({}); the retry pass also failed. Re-state the question using \
                                 the actual entities from the schema.",
                                unknown.join(", ")
                            ),
                        });
                    }
                }
            }
        };

        Ok(query_ir)
    }

    async fn plan_load(
        &self,
        ontology: &OntologyIR,
        source_description: &str,
    ) -> OxResult<LoadPlan> {
        let ontology_json = serialize_pretty(
            &ontology.to_agent_view(ox_core::llm_locale_fallback_default_tags()),
            "ontology",
        )?;

        let mut vars = HashMap::new();
        vars.insert("source_description", source_description);
        vars.insert("ontology", ontology_json.as_str());

        self.call_structured("plan_load", None, "plan_load", &vars, "Planning data load")
            .await
    }

    async fn generate_load_plan(
        &self,
        ontology: &OntologyIR,
        source_schema: &SourceSchema,
    ) -> OxResult<LoadPlan> {
        // The agent view carries the logical schema the load plan
        // targets; the ObjectMappingDef slice is serialised
        // separately so the prompt still sees the canonical wire
        // shape the planner will consume at runtime.
        let ontology_json = serialize_pretty(
            &ontology.to_agent_view(ox_core::llm_locale_fallback_default_tags()),
            "ontology",
        )?;
        let mapping_json =
            serialize_pretty(&ontology.object_mappings(), "object_mappings")?;
        let schema_json = serialize_pretty(source_schema, "source_schema")?;
        let source_description =
            format!("Object Mappings:\n{mapping_json}\n\nSource Schema:\n{schema_json}");

        let mut vars = HashMap::new();
        vars.insert("source_description", source_description.as_str());
        vars.insert("ontology", ontology_json.as_str());

        self.call_structured(
            "plan_load",
            None,
            "plan_load",
            &vars,
            "Generating load plan from project data",
        )
        .await
    }

    async fn select_widget(&self, query: &QueryIR, result_sample: &str) -> OxResult<WidgetHint> {
        let query_json = serialize_pretty(query, "query")?;

        let mut vars = HashMap::new();
        vars.insert("query", query_json.as_str());
        vars.insert("result_sample", result_sample);

        self.call_structured(
            "select_widget",
            None,
            "select_widget",
            &vars,
            "Selecting widget for query results",
        )
        .await
    }
}
