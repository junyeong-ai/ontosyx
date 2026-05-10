//! `QueryTranslator` impl for [`DefaultBrain`].

use std::collections::HashMap;

use async_trait::async_trait;
use tracing::info;

use ox_core::error::{OxError, OxResult};
use ox_core::source_schema::SourceSchema;
use ox_ontology::ir::OntologyIR;
use ox_ontology::load_plan::LoadPlan;
use ox_ontology::{AttemptOutcome, ErrorClassification, PipelineStage};
use ox_query_ir::query::QueryIR;

use crate::model_resolver::operation;
use crate::*;

#[async_trait]
impl QueryTranslator for DefaultBrain {
    async fn translate_query(
        &self,
        question: &str,
        ontology: &OntologyIR,
        retrieved_context: Option<&str>,
        ctx: &branchforge::ExecutionContext,
    ) -> OxResult<(QueryIR, crate::CallProvenance)> {
        // Wrap the existing translate logic so the outer match can
        // record one `InferenceAttempt` per call when an
        // `InferenceContext` is bound on the calling task. Tier 1/2/3
        // fallback + label-correction retry collectively form a single
        // attempt at the InferenceSession layer — multi-attempt chains
        // arise at the Agent level via outer-loop re-invocations.
        let result = self
            .translate_query_inner(question, ontology, retrieved_context, ctx)
            .await;

        match &result {
            Ok((qir, prov)) => {
                self.record_translate_outcome(
                    PipelineStage::Compile,
                    Some(qir),
                    Some(prov),
                    AttemptOutcome::Success,
                )
                .await;
            }
            Err(err) => {
                let outcome = match err {
                    OxError::Validation { message, .. } => AttemptOutcome::ValidationError {
                        classification: ErrorClassification::ValidationFailure,
                        message: message.chars().take(500).collect(),
                    },
                    OxError::Compilation { message } => AttemptOutcome::ValidationError {
                        classification: ErrorClassification::ValidationFailure,
                        message: message.chars().take(500).collect(),
                    },
                    other => AttemptOutcome::RuntimeError {
                        classification: ErrorClassification::RuntimeError,
                        message: format!("{other:?}").chars().take(500).collect(),
                    },
                };
                self.record_translate_outcome(PipelineStage::Compile, None, None, outcome)
                    .await;
            }
        }

        result
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

        self.call_structured(
            operation::PLAN_LOAD,
            None,
            operation::PLAN_LOAD,
            &vars,
            "Planning data load",
        )
        .await
    }

    async fn generate_load_plan(
        &self,
        ontology: &OntologyIR,
        source_schema: &SourceSchema,
    ) -> OxResult<LoadPlan> {
        let ontology_json = serialize_pretty(
            &ontology.to_agent_view(ox_core::llm_locale_fallback_default_tags()),
            "ontology",
        )?;
        let mapping_json = serialize_pretty(&ontology.object_mappings(), "object_mappings")?;
        let schema_json = serialize_pretty(source_schema, "source_schema")?;
        let source_description =
            format!("Object Mappings:\n{mapping_json}\n\nSource Schema:\n{schema_json}");

        let mut vars = HashMap::new();
        vars.insert("source_description", source_description.as_str());
        vars.insert("ontology", ontology_json.as_str());

        self.call_structured(
            operation::PLAN_LOAD,
            None,
            operation::PLAN_LOAD,
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
            operation::SELECT_WIDGET,
            None,
            operation::SELECT_WIDGET,
            &vars,
            "Selecting widget for query results",
        )
        .await
    }
}

// Inherent impl — `translate_query_inner` carries the existing
// 3-tier fallback + label-correction retry logic the public
// trait method wraps for attempt recording. Kept private; only
// `translate_query` invokes it.
impl crate::DefaultBrain {
    async fn translate_query_inner(
        &self,
        question: &str,
        ontology: &OntologyIR,
        retrieved_context: Option<&str>,
        ctx: &branchforge::ExecutionContext,
    ) -> OxResult<(QueryIR, crate::CallProvenance)> {
        // Φ11.2: verified-query exact-hash short-circuit. The
        // `(workspace_id, question_hash)` UNIQUE on
        // `verified_queries` lets us answer "have we already
        // verified this exact question?" in one indexed read.
        // Hit + retrievable status → return the persisted IR with
        // synthetic provenance; LLM round-trip + schema discovery
        // + knowledge RAG all skip. Miss / store error / non-
        // retrievable status / IR deserialise failure → fall
        // through to the full translate path.
        if let Some(cached) = self.try_verified_query_cache(question, ctx).await {
            return Ok(cached);
        }

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

        // Knowledge RAG — hybrid retrieval (trigram title + content,
        // tokenized FTS, optional pgvector NN, optional label boost)
        // fused via RRF. Cold-start workspaces (no embedder, no
        // tokenizer) degrade to a 2-ranker fusion automatically.
        ctx.progress("knowledge_lookup").started();
        let t_knowledge = std::time::Instant::now();
        let label_refs: Vec<&str> = discovered_labels.iter().map(|s| s.as_str()).collect();
        let knowledge_context = if let Some(kb) = &self.knowledge_store {
            knowledge_rag::discover_knowledge(
                kb.as_ref(),
                &knowledge_rag::KnowledgeRetrievalContext {
                    question,
                    discovered_labels: &label_refs,
                    ontology_name: self
                        .ontology_lineage_id
                        .as_deref()
                        .unwrap_or(&ontology.name),
                    ontology_version: ontology.version.number as i32,
                    top_k: 8,
                    workspace_id: current_workspace_id(),
                    tokenizer_registry: self.tokenizer_registry.as_ref(),
                    embedder: self.embedder.as_ref(),
                },
            )
            .await
        } else {
            String::new()
        };

        ctx.progress("knowledge_lookup")
            .completed(t_knowledge.elapsed().as_millis() as u64);

        let glossary_section = crate::design::render_glossary_section(ontology.glossary());

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
        vars.insert("ontology_subgraph_md", retrieved_context.unwrap_or(""));

        // Φ11.2b: top-k verified-query exemplars for ICL
        // injection. The retriever filters status=Verified +
        // complexity != Trivial inline at the SQL layer; an
        // empty bank or store outage returns "" and the
        // placeholder substitutes silently.
        let verified_examples = self.retrieve_verified_examples(question, ctx).await;
        vars.insert("verified_examples", verified_examples.as_str());

        // Primary LLM call (StructuredMatchQuery structured output)
        ctx.progress("llm_primary").started();
        let t_llm = std::time::Instant::now();
        let (query_ir, mut provenance) = match self
            .call_structured_traced::<ox_query_ir::StructuredMatchQuery>(
                "translate_match_query",
                Some("1.0.0"),
                "translate_match_query",
                &vars,
                "Translating to StructuredMatchQuery (structured output)",
            )
            .await
            .and_then(|(match_ir, prov)| match_ir.into_query_ir().map(|qir| (qir, prov)))
        {
            Ok((qir, prov)) => {
                ctx.progress("llm_primary")
                    .completed(t_llm.elapsed().as_millis() as u64);
                info!("StructuredMatchQuery structured output succeeded");
                (qir, prov)
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
                let result: OxResult<(QueryIR, crate::CallProvenance)> = self
                    .call_structured_traced(
                        operation::TRANSLATE_QUERY,
                        Some("1.0.0"),
                        operation::TRANSLATE_QUERY,
                        &vars,
                        "Translating to QueryIR (JSON mode fallback)",
                    )
                    .await;

                match result {
                    Ok((qir, prov)) => {
                        ctx.progress("llm_fallback")
                            .completed(t_fallback.elapsed().as_millis() as u64);
                        (qir, prov)
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
                            .call_structured_traced::<QueryIR>(
                                operation::TRANSLATE_QUERY,
                                Some("1.0.0"),
                                operation::TRANSLATE_QUERY,
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
                let retry: OxResult<(QueryIR, crate::CallProvenance)> = self
                    .call_structured_traced(
                        operation::TRANSLATE_QUERY,
                        Some("1.0.0"),
                        operation::TRANSLATE_QUERY,
                        &retry_vars,
                        "Retrying query translation with label correction",
                    )
                    .await;
                match retry {
                    Ok((qir, retry_prov)) => {
                        let still_unknown = ox_query_ir::unknown_labels_in_query(ontology, &qir);
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
                        // Replace provenance with the retry's — the
                        // returned IR came from this call.
                        provenance = retry_prov;
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

        Ok((query_ir, provenance))
    }

    /// Φ11.2: probe the verified-query bank for an exact hash
    /// match and return `(QueryIR, CallProvenance)` when one is
    /// retrievable. `None` short-circuits nothing — the caller
    /// falls through to the full LLM translate path.
    ///
    /// Returns `None` for every condition that should *not*
    /// short-circuit:
    ///
    /// - No `verified_query_store` attached.
    /// - Lookup error (DB outage, RLS denial — log + drop).
    /// - No row matches the canonical hash.
    /// - Row exists but `status.is_retrievable() == false`
    ///   (UnderReview / Deprecated / Stale).
    /// - Stored `query_ir` JSONB fails to deserialise into
    ///   `QueryIR` (corruption / schema-version drift —
    ///   logged, the LLM path takes over).
    ///
    /// The returned [`crate::CallProvenance`] is *synthetic* —
    /// no LLM was called. The fields encode that explicitly:
    /// `prompt_id = "verified_query_cache"`, `provider =
    /// "ontosyx-cache"`, `prompt_render_hash = "vq:{hash}"`. A
    /// downstream consumer (eval-case provenance, audit DAG)
    /// distinguishes cache-hit from LLM-driven by the prompt id
    /// + render-hash prefix.
    async fn try_verified_query_cache(
        &self,
        question: &str,
        ctx: &branchforge::ExecutionContext,
    ) -> Option<(QueryIR, crate::CallProvenance)> {
        let store = self.verified_query_store.as_ref()?;
        let q_hash = ox_ontology::question_hash(question);
        ctx.progress("verified_query_lookup").started();
        let lookup_started = std::time::Instant::now();
        let row = match store.find_verified_query_by_hash(&q_hash).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                ctx.progress("verified_query_lookup")
                    .completed(lookup_started.elapsed().as_millis() as u64);
                return None;
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    question_hash = %q_hash,
                    "verified-query lookup failed; falling through to LLM translate path"
                );
                ctx.progress("verified_query_lookup")
                    .failed(lookup_started.elapsed().as_millis() as u64);
                return None;
            }
        };

        if !row.status.is_retrievable() {
            ctx.progress("verified_query_lookup").completed_with(
                lookup_started.elapsed().as_millis() as u64,
                serde_json::json!({
                    "outcome": "non_retrievable_status",
                    "status": row.status.as_str(),
                }),
            );
            return None;
        }

        let query_ir: QueryIR = match serde_json::from_value(row.query_ir.clone()) {
            Ok(qir) => qir,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    vq_id = %row.id.as_str(),
                    "verified-query IR deserialise failed; falling through to LLM"
                );
                ctx.progress("verified_query_lookup").completed_with(
                    lookup_started.elapsed().as_millis() as u64,
                    serde_json::json!({"outcome": "deserialise_failed"}),
                );
                return None;
            }
        };

        let provenance = crate::CallProvenance {
            prompt_id: "verified_query_cache".to_string(),
            prompt_version: "1.0.0".to_string(),
            provider: "ontosyx-cache".to_string(),
            model_id: "verified-query".to_string(),
            max_tokens: 0,
            temperature: None,
            // `vq:` prefix on the render hash signals downstream
            // (eval-case provenance, audit DAG) that this entry
            // came from the cache, not an LLM call. The hex hash
            // is the same canonical question hash so the audit
            // trail back-references the verified row.
            prompt_render_hash: format!("vq:{q_hash}"),
        };

        tracing::info!(
            question_hash = %q_hash,
            vq_id = %row.id.as_str(),
            complexity = row.complexity_class.as_str(),
            "verified-query cache hit; returning IR without LLM call"
        );
        ctx.progress("verified_query_lookup").completed_with(
            lookup_started.elapsed().as_millis() as u64,
            serde_json::json!({"outcome": "hit", "vq_id": row.id.as_str()}),
        );
        Some((query_ir, provenance))
    }

    /// Φ11.2b — render top-k verified-query exemplars as an
    /// in-context-learning markdown block injected via the
    /// `{{verified_examples}}` placeholder of the
    /// `translate_match_query` template. The retriever runs on
    /// the cache-miss path *after* `try_verified_query_cache`
    /// already failed, so an exact-hash hit never duplicates
    /// itself into its own ICL block.
    ///
    /// Returns `""` (silently substituted into the placeholder
    /// as an empty line) when:
    ///
    /// - the Brain wasn't built with a verified-query store
    ///   (greenfield deployments before any promotion path),
    /// - the bank carries no eligible rows for this workspace
    ///   (cold-start, or every row is Trivial/UnderReview/etc.),
    /// - the underlying SQL fails (logged at `warn`, treated as
    ///   "fall through to the LLM without ICL" — observability,
    ///   not load-bearing).
    ///
    /// Top-k is fixed at 3 because beyond that the prompt budget
    /// regression dominates the retrieval lift on a trigram
    /// ranker. The Φ11.5 embedding swap raises the ceiling.
    async fn retrieve_verified_examples(
        &self,
        question: &str,
        ctx: &branchforge::ExecutionContext,
    ) -> String {
        let Some(store) = self.verified_query_store.as_ref() else {
            return String::new();
        };
        ctx.progress("verified_query_icl").started();
        let started = std::time::Instant::now();

        // Hybrid 3-ranker retrieval — fuses trigram (typo recall),
        // tokenized FTS (Korean morphology + glossary canonical
        // lemmas), and pgvector cosine NN (paraphrase recall) via
        // RRF in a single SQL roundtrip. Each component degrades
        // gracefully:
        //
        //   - No embedder → 2-ranker fusion (trigram + FTS).
        //   - No tokenizer → FTS arm passes raw question.
        //   - No matches anywhere → empty rows, no ICL block.
        //
        // The hybrid call replaces the prior embedding-then-trigram
        // fall-through chain — RRF surfaces rows that any single
        // ranker would have ranked low but the cohort agreed on.
        let workspace_id = current_workspace_id();
        let tokenized_question = match (self.tokenizer_registry.as_ref(), workspace_id) {
            (Some(reg), Some(ws)) => {
                let tok = reg.for_workspace(ws);
                tok.tokenize(question)
                    .unwrap_or_else(|_| question.to_string())
            }
            _ => question.to_string(),
        };
        let query_embedding = if let Some(embedder) = self.embedder.as_ref() {
            match embedder
                .embed(
                    question,
                    "Represent the analytical question for retrieval",
                    ox_memory::EmbeddingRole::Query,
                )
                .await
            {
                Ok(v) => Some(v),
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        "verified-query question embed failed; hybrid degrades to 2-ranker (trigram + fts)"
                    );
                    None
                }
            }
        } else {
            None
        };

        let retrieval_mode = match (query_embedding.is_some(), self.tokenizer_registry.is_some()) {
            (true, true) => "hybrid_3way",
            (true, false) => "hybrid_2way_no_tok",
            (false, true) => "hybrid_2way_no_vec",
            (false, false) => "hybrid_trgm_fts_raw",
        };
        let rows = match store
            .hybrid_search_verified_queries_for_icl(
                question,
                &tokenized_question,
                query_embedding.as_deref(),
                3,
            )
            .await
        {
            Ok(rs) => rs,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "verified-query hybrid ICL retrieval failed; translating without exemplars"
                );
                ctx.progress("verified_query_icl")
                    .failed(started.elapsed().as_millis() as u64);
                return String::new();
            }
        };
        if rows.is_empty() {
            ctx.progress("verified_query_icl").completed_with(
                started.elapsed().as_millis() as u64,
                serde_json::json!({"outcome": "empty"}),
            );
            return String::new();
        }
        let mut block = String::with_capacity(2048);
        block.push_str("\n## Verified examples (workspace-validated patterns)\n\n");
        block.push_str(
            "These QueryIR shapes were promoted by an operator after a successful run. \
             Treat them as authoritative templates — match the structural pattern when \
             the question is analogous.\n\n",
        );
        for row in &rows {
            block.push_str("### Q: ");
            block.push_str(&row.question);
            block.push_str("\n```json\n");
            match serde_json::to_string_pretty(&row.query_ir) {
                Ok(pretty) => block.push_str(&pretty),
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        vq_id = %row.id.as_str(),
                        "verified-query ICL row IR serialise failed; skipping"
                    );
                    continue;
                }
            }
            block.push_str("\n```\n\n");
        }
        ctx.progress("verified_query_icl").completed_with(
            started.elapsed().as_millis() as u64,
            serde_json::json!({
                "outcome": "hit",
                "count": rows.len(),
                "retrieval_mode": retrieval_mode,
            }),
        );
        block
    }
}

/// Read the current task-local `WORKSPACE_ID` if present.
/// Returns `None` when the Brain is being driven outside a
/// workspace scope (test stubs, isolated unit calls); the
/// retriever's tokenizer lookup falls back to the registry's
/// system-only default.
fn current_workspace_id() -> Option<uuid::Uuid> {
    ox_store::WORKSPACE_ID.try_with(|id| *id).ok()
}
