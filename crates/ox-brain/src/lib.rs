#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

pub mod auth;
pub mod client_pool;
pub mod design;
pub mod knowledge_rag;
pub mod knowledge_util;
pub mod model_resolver;
pub mod plan_router;
pub mod prompts;
pub mod provider;
pub mod schema;
pub mod schema_rag;
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_support;

pub use design::{DesignOntologyInput, DesignOntologyOutput};

use async_trait::async_trait;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tracing::info;

use ox_core::error::{OxError, OxResult};
use ox_ontology::load_plan::LoadPlan;
use ox_ontology::command::OntologyCommand;
use ox_ontology::ir::OntologyIR;
use ox_query_ir::query::QueryIR;
use ox_ontology::repo_insights::{FileContent, RepoInsights};
use ox_ontology::mapping::SourceId;
use ox_core::source_schema::SourceSchema;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use prompts::PromptRegistry;
use provider::{StreamChunk, TokenUsage, structured_completion};

// ---------------------------------------------------------------------------
// ExplanationOutput — result from non-structured LLM calls
// ---------------------------------------------------------------------------

pub struct ExplanationOutput {
    pub content: String,
    pub model: String,
    pub usage: Option<TokenUsage>,
}

// ---------------------------------------------------------------------------
// ProviderInfo — provider metadata for health checks and logging
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub model: String,
}

/// Type alias for a streaming explanation response.
pub type ExplanationStream =
    Pin<Box<dyn futures_core::Stream<Item = OxResult<StreamChunk>> + Send>>;

// ---------------------------------------------------------------------------
// EditCommandsOutput — result from ontology edit command generation
// ---------------------------------------------------------------------------

pub struct EditCommandsOutput {
    pub commands: Vec<OntologyCommand>,
    pub explanation: String,
    pub model: String,
}

// ---------------------------------------------------------------------------
// WidgetHint — lightweight LLM output for widget selection
// ---------------------------------------------------------------------------

/// Simple hint from LLM about which widget to use.
/// The frontend interprets this and renders the appropriate component.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WidgetHint {
    /// Which widget type to render
    pub widget_type: WidgetType,
    /// Optional title for the widget
    pub title: Option<String>,
    /// Brief reason for the selection (for debugging, not shown to user)
    pub reason: Option<String>,
}

/// Available visualization widget types for query results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WidgetType {
    /// Categorical comparisons with a single metric
    BarChart,
    /// Multiple metrics on the same category axis
    ComboChart,
    /// Proportional distribution with few categories
    PieChart,
    /// Time series or sequential trends
    LineChart,
    /// Single aggregate value
    StatCard,
    /// Multi-column detailed data
    Table,
    /// Node-edge graph visualization (paths, networks, relationships)
    Graph,
    /// Matrix of values with color-coded intensity (correlation, co-occurrence)
    Heatmap,
    /// Vertical event timeline (temporal sequences, audit trails)
    Timeline,
    /// Hierarchical area proportions (category breakdown, disk usage)
    Treemap,
    /// Conversion or process funnel (stages with drop-off rates)
    Funnel,
    /// Data is self-explanatory from text alone
    None,
}

// ---------------------------------------------------------------------------
// Sub-traits — focused LLM capability groups
// ---------------------------------------------------------------------------

/// Ontology design and refinement capabilities.
#[async_trait]
pub trait OntologyDesigner: Send + Sync {
    /// Analyze sample data + the workspace's existing domain context
    /// and design an ontology.
    ///
    /// The structured input shape lets the LLM see — alongside the
    /// raw sample data — every glossary term, code system,
    /// pre-detected ambiguity, and (in extension mode) existing
    /// ontology the workspace already knows about. The model prefers
    /// the existing canonical labels over inventing parallel ones,
    /// reducing the post-pass `binding_suggestions` to a fall-back
    /// rather than the primary correction step.
    ///
    /// `input.source_id` stamps every emitted `ObjectMappingDef` in
    /// the returned IR with the canonical source identity so
    /// federation plans, provenance, and plan-cache keys stay
    /// consistent with the OntologyDraft the caller is operating on.
    ///
    /// Returns the produced [`OntologyIR`] paired with the
    /// [`ArtifactProvenance`](ox_ontology::source_mapping::ArtifactProvenance)
    /// envelope (prompt id + version + resolved model id + replay
    /// params) that authored it. The caller threads the provenance
    /// straight into the
    /// [`SourceMappingArtifact`](ox_ontology::source_mapping::SourceMappingArtifact)
    /// it persists for this design action — atomic, no post-hoc
    /// lookup race against a concurrent model-config update.
    async fn design_ontology(
        &self,
        input: &DesignOntologyInput<'_>,
    ) -> OxResult<DesignOntologyOutput>;

    /// Design a partial ontology for a batch of tables (divide-and-conquer pipeline).
    /// Returns raw `InputOntologyDef` (not normalized) for later merging —
    /// the caller runs `normalize(merged, source_id)` once after all
    /// batches + cross-edge resolution land, so `source_id` is not
    /// threaded through this method.
    async fn design_ontology_batch(
        &self,
        batch_data: &str,
        context: &str,
        existing_nodes: &str,
        cross_fks: &str,
    ) -> OxResult<ox_ontology::input::InputOntologyDef>;

    /// Resolve the
    /// [`ArtifactProvenance`](ox_ontology::source_mapping::ArtifactProvenance)
    /// that *would* be authored against a named prompt + operation
    /// right now. The divide-and-conquer batch path captures
    /// provenance once at the top of the loop (every cluster shares
    /// one prompt + model resolution) and reuses it for the merged-
    /// IR artifact. Naming mirrors `resolve_cross_edges` — both are
    /// derive-from-current-state, no LLM call.
    async fn resolve_design_provenance(
        &self,
        prompt_name: &str,
        operation: &str,
    ) -> OxResult<ox_ontology::source_mapping::ArtifactProvenance>;

    /// Generate missing cross-domain edges for uncovered FK relationships.
    /// Returns edge definitions to be appended to the merged InputIR.
    async fn resolve_cross_edges(
        &self,
        node_labels: &str,
        existing_edges: &str,
        uncovered_fks: &str,
    ) -> OxResult<Vec<ox_ontology::InputEdgeTypeDef>>;

    /// Refine an ontology's metadata using graph profile statistics and/or additional context.
    /// `refinement_context` is pre-formatted and may contain graph profile data,
    /// domain gap resolutions, or both combined.
    async fn refine_ontology(
        &self,
        ontology: &OntologyIR,
        refinement_context: &str,
        source_id: &SourceId,
    ) -> OxResult<OntologyIR>;
}

/// Query translation and widget selection capabilities.
#[async_trait]
pub trait QueryTranslator: Send + Sync {
    /// Translate natural language question into a QueryIR.
    ///
    /// `retrieved_context` carries the GraphRAG-rendered subgraph
    /// markdown the caller has assembled (typically by walking
    /// `OntologyNavigationStore::search_entry_points` →
    /// `expand_neighbors` → `render_subgraph_for_llm`). When
    /// supplied, the translator injects it into the prompt as the
    /// `ontology_subgraph_md` template variable so the LLM sees a
    /// targeted, anchor-expanded slice of the ontology in addition
    /// to the schema RAG snippet. Pass `None` from callers that
    /// have no Postgres-backed navigation store wired (admin
    /// preview routes, `mcp` standalone server, evaluation
    /// case-execute endpoint that runs against a fresh draft IR).
    async fn translate_query(
        &self,
        question: &str,
        ontology: &OntologyIR,
        retrieved_context: Option<&str>,
        ctx: &branchforge::ExecutionContext,
    ) -> OxResult<QueryIR>;

    /// Generate a LoadPlan from an ontology and source data description
    async fn plan_load(
        &self,
        ontology: &OntologyIR,
        source_description: &str,
    ) -> OxResult<LoadPlan>;

    /// Generate a LoadPlan from ontology + source schema.
    ///
    /// Reads mapping information directly from `ontology.object_mappings()`
    /// — the IR is the single source of truth for where nodes and
    /// properties live in the source, so no external mapping
    /// parameter is needed. `source_schema` carries the physical
    /// schema the load plan must reconcile with.
    async fn generate_load_plan(
        &self,
        ontology: &OntologyIR,
        source_schema: &SourceSchema,
    ) -> OxResult<LoadPlan>;

    /// Select the best widget type for displaying query results
    async fn select_widget(&self, query: &QueryIR, result_sample: &str) -> OxResult<WidgetHint>;
}

/// Text explanation capabilities (structured and streaming).
#[async_trait]
pub trait Explainer: Send + Sync {
    /// Generate a text explanation of query results.
    async fn explain(&self, user_message: &str) -> OxResult<ExplanationOutput>;

    /// Stream a text explanation of query results as an async stream of text chunks.
    async fn explain_stream(&self, user_message: String) -> OxResult<ExplanationStream>;

    /// Generate proactive insight suggestions from ontology structure.
    async fn suggest_insights(
        &self,
        ontology: &OntologyIR,
        graph_stats: Option<&serde_json::Value>,
    ) -> OxResult<Vec<ox_ontology::InsightHint>>;
}

/// Repository analysis capabilities.
#[async_trait]
pub trait RepoAnalyzer: Send + Sync {
    /// Repo navigation: given a file tree string, select up to 30
    /// relevant files for analysis. Returns relative paths the LLM
    /// considers most useful for ontology design.
    async fn navigate_repo(&self, file_tree: &str) -> OxResult<Vec<String>>;

    /// Repo deep-read: given file contents, extract structured domain
    /// insights. Returns enum definitions, ORM relationships, field
    /// hints, and domain notes.
    async fn analyze_repo_files(&self, files: &[FileContent]) -> OxResult<RepoInsights>;
}

/// Surgical ontology editing via atomic commands.
#[async_trait]
pub trait OntologyEditor: Send + Sync {
    /// Generate a list of atomic OntologyCommand operations to fulfill the user's edit request.
    /// Returns surgical commands instead of a full ontology replacement.
    async fn generate_edit_commands(
        &self,
        ontology: &OntologyIR,
        user_request: &str,
    ) -> OxResult<EditCommandsOutput>;
}

/// One axis of an [`EvaluationJudgement`] — score in `[0.0, 1.0]`
/// plus reasoning. The reasoning surfaces directly on
/// `evaluation_metrics.reasoning` so an operator triaging a
/// regression sees the judge's evidence inline.
#[derive(
    Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct EvaluationAxisScore {
    pub score: f64,
    pub reasoning: String,
}

/// RAGAS-style judgement for a single evaluation case. Four
/// independent axes, all on `[0.0, 1.0]`. The runtime persists
/// each axis as its own `evaluation_metrics` row keyed at
/// `(case_id, name)` so a re-judge replaces in place.
#[derive(
    Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct EvaluationJudgement {
    /// Does the produced `actual` ground itself in identifiers
    /// the question names?
    pub faithfulness: EvaluationAxisScore,
    /// Does the produced `actual` answer the question, or an
    /// adjacent one?
    pub answer_relevance: EvaluationAxisScore,
    /// Of the entities the query touches, what fraction is
    /// necessary?
    pub context_precision: EvaluationAxisScore,
    /// Of the entities the question implies, what fraction does
    /// the query cover?
    pub context_recall: EvaluationAxisScore,
}

impl EvaluationJudgement {
    /// Materialise the judgement as `(name, score, reasoning)`
    /// triples in the canonical RAGAS axis order. The endpoint
    /// iterates this to record one `EvaluationMetric` per axis;
    /// the natural-key UPSERT on `(case_id, name)` handles re-judge
    /// idempotency.
    pub fn axes(&self) -> [(&'static str, f64, &str); 4] {
        [
            (
                "faithfulness",
                self.faithfulness.score,
                self.faithfulness.reasoning.as_str(),
            ),
            (
                "answer_relevance",
                self.answer_relevance.score,
                self.answer_relevance.reasoning.as_str(),
            ),
            (
                "context_precision",
                self.context_precision.score,
                self.context_precision.reasoning.as_str(),
            ),
            (
                "context_recall",
                self.context_recall.score,
                self.context_recall.reasoning.as_str(),
            ),
        ]
    }
}

/// LLM judge over an executed evaluation case. The judge sees
/// the question, the optional golden expectation, and the actual
/// translation output, and emits a four-axis RAGAS-style score.
/// Invoked by the `/api/evaluation/cases/{case_id}/judge`
/// endpoint after the case-execute path populates `actual`.
#[async_trait]
pub trait EvaluationJudge: Send + Sync {
    async fn judge_evaluation_case(
        &self,
        question: &str,
        expected: Option<&serde_json::Value>,
        actual: &serde_json::Value,
    ) -> OxResult<EvaluationJudgement>;
}

/// LLM provider metadata for health checks and observability.
pub trait LlmMetadata: Send + Sync {
    /// Default model info for logging/audit purposes.
    fn default_model_info(&self) -> ProviderInfo;

    /// List all loaded prompt templates with their versions.
    fn list_prompts(&self) -> Vec<(String, String)>;

    /// Template-level SHA-256 hex of the named prompt — system body
    /// only, pre-render. The fingerprint a content-addressed cache
    /// (such as the draft cluster checkpoint store) folds in
    /// alongside its other inputs. Distinct from
    /// `ArtifactProvenance::prompt_render_hash` (which captures the
    /// post-render content of one call): caches that key on a
    /// *unit-of-work* shape, not a single call, must avoid letting
    /// per-call variables enter the cache key. An admin who edits
    /// the prompt body without bumping `prompt_version` shifts this
    /// hash, so every cache entry authored under the prior body
    /// invalidates on the next read — the correct contract.
    fn prompt_template_hash(&self, prompt_name: &str) -> OxResult<String>;
}

// ---------------------------------------------------------------------------
// Brain trait — composite supertrait aggregating all LLM capabilities
// ---------------------------------------------------------------------------

/// Convenience supertrait that aggregates all LLM capabilities.
/// Use specific sub-traits (`OntologyDesigner`, `QueryTranslator`, etc.) when
/// a component only needs a subset of capabilities.
pub trait Brain:
    OntologyDesigner
    + OntologyEditor
    + QueryTranslator
    + Explainer
    + RepoAnalyzer
    + EvaluationJudge
    + LlmMetadata
{
}

/// Blanket impl: anything implementing all sub-traits is automatically a Brain.
impl<T> Brain for T where
    T: OntologyDesigner
        + OntologyEditor
        + QueryTranslator
        + Explainer
        + RepoAnalyzer
        + EvaluationJudge
        + LlmMetadata
{
}

// ---------------------------------------------------------------------------
// ModelHint — per-method cost optimization
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// DefaultBrain — uses ClientPool + ModelResolver + PromptRegistry
// ---------------------------------------------------------------------------

pub struct DefaultBrain {
    client_pool: Arc<client_pool::ClientPool>,
    model_resolver: Arc<dyn model_resolver::ModelResolver>,
    prompts: PromptRegistry,
    /// Cached default model info for sync access (logging, audit).
    default_model: ProviderInfo,
    /// Optional memory store for schema RAG. When available, `translate_query`
    /// uses vector search to discover relevant sub-schema instead of injecting
    /// the entire ontology JSON (~120K tokens → ~2K tokens).
    memory: Option<Arc<ox_memory::MemoryStore>>,
    /// Ontology ID for scoping schema RAG searches.
    ontology_lineage_id: Option<String>,
    /// Optional knowledge store for failure-driven learning.
    /// When available, `translate_query` injects learned corrections.
    knowledge_store: Option<Arc<dyn ox_store::KnowledgeStore>>,
    /// Optional evaluation capture hook. When set + an
    /// `EvaluationContext` is bound on the calling task, every
    /// `call_structured_traced` records a `latency_ms.<operation>`
    /// metric against the active case. `None` → no-op.
    evaluation_capture: Option<Arc<dyn ox_store::EvaluationCapture>>,
}

impl DefaultBrain {
    pub fn new(
        client_pool: Arc<client_pool::ClientPool>,
        model_resolver: Arc<dyn model_resolver::ModelResolver>,
        prompts: PromptRegistry,
        default_model: ProviderInfo,
    ) -> Self {
        Self {
            client_pool,
            model_resolver,
            prompts,
            default_model,
            memory: None,
            ontology_lineage_id: None,
            knowledge_store: None,
            evaluation_capture: None,
        }
    }

    /// Attach an evaluation capture hook. The wide
    /// `Arc<dyn EvaluationCapture>` accepts the storage-backed
    /// implementation (`PostgresStore`) as well as test stubs;
    /// `NullEvaluationCapture::arc()` is the explicit no-op.
    pub fn with_evaluation_capture(
        mut self,
        capture: Arc<dyn ox_store::EvaluationCapture>,
    ) -> Self {
        self.evaluation_capture = Some(capture);
        self
    }

    /// Set memory store for schema RAG in query translation.
    pub fn with_memory(
        mut self,
        memory: Arc<ox_memory::MemoryStore>,
        ontology_lineage_id: Option<String>,
    ) -> Self {
        self.memory = Some(memory);
        self.ontology_lineage_id = ontology_lineage_id;
        self
    }

    /// Set knowledge store for failure-driven learning in query translation.
    pub fn with_knowledge(mut self, store: Arc<dyn ox_store::KnowledgeStore>) -> Self {
        self.knowledge_store = Some(store);
        self
    }

    /// Access the knowledge store (for extraction triggers in ox-agent).
    pub fn knowledge_store(&self) -> Option<&Arc<dyn ox_store::KnowledgeStore>> {
        self.knowledge_store.as_ref()
    }

    /// Resolve model and client for a given operation.
    ///
    /// Uses `by_provider` to look up the already-authenticated client
    /// from the pool — no credentials needed since the client was pre-warmed
    /// during server startup.
    async fn resolve_for_operation(
        &self,
        operation: &str,
    ) -> OxResult<(Arc<dyn branchforge::LlmCall>, model_resolver::ResolvedModel)> {
        let resolved = self.model_resolver.resolve(operation).await?;
        let client = self
            .client_pool
            .by_provider(&resolved.provider)
            .ok_or_else(|| OxError::Runtime {
                message: format!(
                    "No LLM client available for provider '{}'. \
                     Ensure it was registered during server startup.",
                    resolved.provider
                ),
            })?;
        Ok((client, resolved))
    }

    /// Core LLM call: resolve model via operation name, load prompt
    /// template, render variables, call `structured_completion` with
    /// prompt caching.
    async fn call_structured<T: serde::de::DeserializeOwned + schemars::JsonSchema>(
        &self,
        prompt_name: &str,
        min_version: Option<&str>,
        operation: &str,
        vars: &HashMap<&str, &str>,
        log_message: &str,
    ) -> OxResult<T> {
        let (parsed, _) = self
            .call_structured_traced(prompt_name, min_version, operation, vars, log_message)
            .await?;
        Ok(parsed)
    }

    /// Same as [`Self::call_structured`] but additionally returns a
    /// [`CallProvenance`] capturing the exact prompt + model the
    /// call ran against. Callers that need provenance for downstream
    /// artifact authoring (e.g., `design_ontology` → `ArtifactProvenance`)
    /// use this method directly — single registry / resolver round-trip,
    /// no post-hoc lookup that could drift behind a concurrent
    /// model-config update.
    async fn call_structured_traced<
        T: serde::de::DeserializeOwned + schemars::JsonSchema,
    >(
        &self,
        prompt_name: &str,
        min_version: Option<&str>,
        operation: &str,
        vars: &HashMap<&str, &str>,
        log_message: &str,
    ) -> OxResult<(T, CallProvenance)> {
        let tmpl = match min_version {
            Some(v) => self.prompts.checked_for(prompt_name, v)?,
            None => self.prompts.get(prompt_name)?,
        };
        let user_prompt = tmpl.render_user(vars);

        let combined_render = format!("{}\n\n{}", tmpl.system, user_prompt);
        crate::design::assert_within_budget(
            &combined_render,
            crate::design::PromptBudget::for_prompt(prompt_name),
        )
        .map_err(|err| OxError::Validation {
            field: "prompt".to_string(),
            message: err.to_string(),
        })?;

        let (client, resolved) = self.resolve_for_operation(operation).await?;
        let effective_max_tokens = resolved.max_tokens.unwrap_or(tmpl.max_tokens);
        let effective_temperature = resolved.temperature.or(tmpl.temperature);

        info!(
            model = %resolved.model_id,
            operation,
            prompt_version = %tmpl.version,
            "{log_message}"
        );

        let started = std::time::Instant::now();
        let parsed = structured_completion(
            client.as_ref(),
            &resolved.model_id,
            &tmpl.system,
            &user_prompt,
            effective_max_tokens,
            effective_temperature,
        )
        .await?;
        let elapsed_ms = started.elapsed().as_millis() as i64;

        // Evaluation capture hook — records `latency_ms.<operation>`
        // when the call path is inside an `EvaluationContext` scope
        // *and* an `EvaluationCapture` was attached. Production
        // traffic without an evaluation scope skips both branches
        // for free (no allocations, no awaits).
        if let (Some(ctx), Some(capture)) = (
            ox_store::current_evaluation_context(),
            self.evaluation_capture.as_ref(),
        ) {
            // Don't propagate capture-side failures — the LLM
            // call already succeeded, the operator's primary path
            // is the typed response. Log + drop matches the
            // wider observability policy (capture is best-effort,
            // not load-bearing).
            if let Err(err) = capture.record_latency(&ctx, operation, elapsed_ms).await {
                tracing::warn!(
                    error = %err,
                    operation,
                    "evaluation capture record_latency failed"
                );
            }
        }

        // Render hash captures system + user post-interpolation.
        // An admin who edits the DB-backing prompt without bumping
        // `prompt_version` shifts this value, and `ContentBody`
        // re-hashes against the new provenance — the prior artifact
        // id no longer matches and the operator sees the divergence
        // rather than a silent cache hit.
        let prompt_render_hash =
            ox_ontology::source_mapping::ArtifactProvenance::compute_prompt_render_hash(
                &format!("{}\n\n{}", tmpl.system, user_prompt),
            );

        Ok((
            parsed,
            CallProvenance {
                prompt_id: prompt_name.to_string(),
                prompt_version: tmpl.version.clone(),
                model_id: resolved.model_id,
                max_tokens: effective_max_tokens,
                temperature: effective_temperature,
                prompt_render_hash,
            },
        ))
    }
}

/// Concrete record of one `structured_completion` invocation —
/// everything a downstream consumer needs to attribute the call,
/// replay it, or diff it against another. Folds into
/// [`ox_ontology::source_mapping::ArtifactProvenance`] via
/// [`CallProvenance::into_artifact_provenance`].
#[derive(Debug, Clone)]
pub struct CallProvenance {
    pub prompt_id: String,
    pub prompt_version: String,
    pub model_id: String,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    /// SHA-256 hex of the rendered prompt (system + user with every
    /// variable interpolated). Stays empty for paths
    /// that have not been wired through the render-hashing helper
    /// yet; a populated value bumps `ContentBody`'s hash so an
    /// admin who edited the DB-backing prompt without bumping
    /// `prompt_version` cannot accidentally re-use the prior
    /// artifact id.
    pub prompt_render_hash: String,
}

impl CallProvenance {
    /// Project into the artifact-side envelope. Numeric knobs land
    /// in `params` so a future replay can reconstruct the call
    /// shape without a parallel mapping table.
    pub fn into_artifact_provenance(self) -> ox_ontology::source_mapping::ArtifactProvenance {
        let mut params: std::collections::BTreeMap<String, serde_json::Value> =
            std::collections::BTreeMap::new();
        params.insert("max_tokens".into(), serde_json::Value::from(self.max_tokens));
        if let Some(t) = self.temperature {
            params.insert("temperature".into(), serde_json::Value::from(t));
        }
        ox_ontology::source_mapping::ArtifactProvenance {
            prompt_id: self.prompt_id,
            prompt_version: self.prompt_version,
            model_id: self.model_id,
            params,
            prompt_render_hash: self.prompt_render_hash,
        }
    }
}

fn serialize_pretty(value: &impl serde::Serialize, label: &str) -> OxResult<String> {
    serde_json::to_string_pretty(value).map_err(|e| OxError::Runtime {
        message: format!("Failed to serialize {label}: {e}"),
    })
}


mod default_brain;
