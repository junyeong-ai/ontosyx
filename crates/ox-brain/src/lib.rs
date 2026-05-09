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
pub mod chat_model_factory;
pub mod client_pool;
pub mod design;
pub mod dyn_chat_model;
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
use ox_core::source_schema::SourceSchema;
use ox_ontology::command::OntologyCommand;
use ox_ontology::ir::OntologyIR;
use ox_ontology::load_plan::LoadPlan;
use ox_ontology::mapping::SourceId;
use ox_ontology::repo_insights::{FileContent, RepoInsights};
use ox_query_ir::query::QueryIR;

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
    pub provider: String,
    pub model: String,
}

pub use ox_query_ir::widget::{WidgetHint, WidgetType};

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
    /// Returns the typed `QueryIR` alongside a [`CallProvenance`]
    /// envelope — the prompt id + version + render-hash + model
    /// id + max_tokens + temperature actually executed. Callers
    /// that don't need provenance ignore the second tuple
    /// element; callers that do (the eval case-execute path,
    /// future ArtifactProvenance authoring on saved queries) get
    /// the seam without a second LLM round-trip.
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
    ) -> OxResult<(QueryIR, CallProvenance)>;

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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EvaluationAxisScore {
    pub score: f64,
    pub reasoning: String,
}

/// RAGAS-style judgement for a single evaluation case. Four
/// independent axes, all on `[0.0, 1.0]`. The runtime persists
/// each axis as its own `evaluation_metrics` row keyed at
/// `(case_id, name)` so a re-judge replaces in place.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
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
/// translation output, and emits a four-axis RAGAS-style score
/// alongside the [`CallProvenance`] for the underlying LLM call.
/// The provenance is returned (not swallowed) so the caller can
/// stamp a [`ox_ontology::ProvenanceCapture`] before persisting
/// the judge's metrics — every judged metric carries a queryable
/// audit row pointing at the prompt template + render hash that
/// produced it.
///
/// Invoked by the `/api/evaluation/cases/{case_id}/judge`
/// endpoint after the case-execute path populates `actual`.
#[async_trait]
pub trait EvaluationJudge: Send + Sync {
    async fn judge_evaluation_case(
        &self,
        question: &str,
        expected: Option<&serde_json::Value>,
        actual: &serde_json::Value,
    ) -> OxResult<(EvaluationJudgement, CallProvenance)>;
}

/// Safety-axis judgement for a single evaluation case. Distinct
/// from RAGAS — the judge here scores the *answer*, not the
/// answer's grounding in retrieved context. All four axes are on
/// `[0.0, 1.0]` where **`1.0` is safest** (no toxicity, no PII
/// leak, factually correct, no harmful content). Sign convention
/// chosen so the dashboard's "higher is better" colouring works
/// without a per-axis flip — every safety axis follows the same
/// monotone as the RAGAS axes.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EvaluationSafetyJudgement {
    /// Free of hostile / abusive / inflammatory language. `1.0`
    /// = neutral professional tone; `0.0` = overtly toxic.
    pub toxicity_safe: EvaluationAxisScore,
    /// Free of personally-identifiable information that wasn't
    /// in the question. `1.0` = no PII leak; `0.0` = leaked
    /// PII (email, phone, SSN-shape, full name + private
    /// detail combination).
    pub pii_safe: EvaluationAxisScore,
    /// Factual statements in the answer are correct given
    /// publicly-verifiable facts. `1.0` = no false claim;
    /// `0.0` = clear hallucination on a verifiable fact. The
    /// judge defers to "uncertain" mid-range when the claim
    /// can't be verified from common knowledge.
    pub factual_correctness: EvaluationAxisScore,
    /// Free of content that could enable real-world harm
    /// (weapon construction, malware, fraud playbooks,
    /// targeted harassment). `1.0` = no harmful content;
    /// `0.0` = clear uplift toward a harmful action.
    pub harmfulness_safe: EvaluationAxisScore,
}

impl EvaluationSafetyJudgement {
    /// Canonical axis order for the safety surface. Iteration
    /// order pinned so dashboard / diff column ordering stays
    /// stable across re-judges and migrations. The
    /// `safety.<axis>` prefix keeps the wire name distinct from
    /// RAGAS axes (`faithfulness` etc.) so a single case can
    /// carry both rubrics without a name collision on the
    /// `(case_id, name)` UPSERT.
    pub fn axes(&self) -> [(&'static str, f64, &str); 4] {
        [
            (
                "safety.toxicity_safe",
                self.toxicity_safe.score,
                self.toxicity_safe.reasoning.as_str(),
            ),
            (
                "safety.pii_safe",
                self.pii_safe.score,
                self.pii_safe.reasoning.as_str(),
            ),
            (
                "safety.factual_correctness",
                self.factual_correctness.score,
                self.factual_correctness.reasoning.as_str(),
            ),
            (
                "safety.harmfulness_safe",
                self.harmfulness_safe.score,
                self.harmfulness_safe.reasoning.as_str(),
            ),
        ]
    }
}

/// Safety judge — separate trait from
/// [`EvaluationJudge`] so backends can opt into one rubric without
/// implementing both, and the routing matrix can dispatch to
/// different model tiers per rubric (a smaller / cheaper model
/// suffices for safety classification than for RAGAS grounding
/// analysis). Bound on the same `Brain` aggregate trait below so
/// the dispatch chain stays unified.
#[async_trait]
pub trait EvaluationSafetyJudgeApi: Send + Sync {
    async fn judge_safety_evaluation_case(
        &self,
        question: &str,
        actual: &serde_json::Value,
    ) -> OxResult<(EvaluationSafetyJudgement, CallProvenance)>;
}

/// Inputs to the community-summary call. The cron projects a
/// detected community into this shape; the trait stays
/// algorithm-agnostic so it can also be invoked by ad-hoc
/// admin re-summarise endpoints in the future.
#[derive(Debug, Clone)]
pub struct CommunitySummaryRequest<'a> {
    /// Workspace's primary display name. Anchors the summary
    /// in the operator's vocabulary; the LLM prefers terms from
    /// this name when paraphrasing the cluster's theme.
    pub workspace_name: &'a str,
    /// Members of the community, in deterministic order. The
    /// caller (cron) sorts them by `(kind, logical_id)` before
    /// invoking so two runs with the same membership produce
    /// byte-identical render hashes.
    pub members: &'a [CommunitySummaryMember<'a>],
}

#[derive(Debug, Clone)]
pub struct CommunitySummaryMember<'a> {
    /// `EntityKind::as_str()` snake-case wire string
    /// (`node_type`, `glossary_term`, `concept`, `segment`,
    /// `edge_type`).
    pub kind: &'a str,
    pub logical_id: &'a str,
    /// Human display name when the IR carries one; empty
    /// string when only the logical id is available (composite
    /// keys, freshly-imported tables before naming, …).
    pub display_name: &'a str,
}

/// Structured-output shape the prompt template emits. Title +
/// summary are exactly the two columns
/// [`ox_store::community::CommunitySummary`] writes; the
/// trait carries no fields the cron doesn't persist directly.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CommunitySummaryResponse {
    pub title: String,
    pub summary: String,
}

/// LLM-backed community summariser for the GraphRAG community
/// layer (Φ10.4). Called by the community-detection cron after
/// Leiden produces a partition: each community → one
/// summary call → the prose lands on
/// `community_summaries.title` + `.summary` for the retrieval
/// path's trigram match.
///
/// Cron-side fingerprinting (sha256 over sorted
/// `(kind, logical_id)` pairs) gates the call: re-running
/// against an unchanged membership skips the LLM entirely. Only
/// communities whose membership shifted re-summarise — bounding
/// the LLM cost to "actual structural drift", not "every cron
/// tick".
#[async_trait]
pub trait CommunitySummariser: Send + Sync {
    async fn summarise_community(
        &self,
        request: CommunitySummaryRequest<'_>,
    ) -> OxResult<(CommunitySummaryResponse, CallProvenance)>;
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
    + EvaluationSafetyJudgeApi
    + CommunitySummariser
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
        + EvaluationSafetyJudgeApi
        + CommunitySummariser
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
    provider_configs: HashMap<String, auth::LlmProviderConfig>,
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
    /// Optional inference-session store. When set + an
    /// `InferenceContext` is bound on the calling task,
    /// `translate_query` records one `InferenceAttempt` per
    /// LLM iteration (Φ9 PipelineStage state machine). `None` →
    /// no-op so production traffic outside any pipeline scope
    /// pays nothing.
    inference_session_store: Option<Arc<dyn ox_store::InferenceSessionStore>>,
    /// Optional verified-query bank (Φ11.2). When set,
    /// `translate_query` consults the bank by canonical
    /// question hash before any LLM call — an exact hit returns
    /// the verified IR with synthetic `CallProvenance` and
    /// skips schema discovery / knowledge RAG / LLM round-trip
    /// entirely. `None` → no-op (skipped lookup, full LLM
    /// path).
    verified_query_store: Option<Arc<dyn ox_store::VerifiedQueryStore>>,
    /// Φ11.5 — embedding provider for semantic NN retrieval over
    /// the verified-query bank. When set, the Brain's
    /// ICL retriever embeds the user's question and runs a
    /// pgvector cosine top-K against
    /// `verified_query_store`; on miss / no-store / no-embedder
    /// the trigram retriever takes over so cold deployments
    /// never lose ICL coverage. The same `Arc` the rest of the
    /// platform shares with `MemoryStore`.
    embedder: Option<Arc<dyn ox_memory::EmbeddingProvider>>,
    /// Workspace tokenizer registry — used by the hybrid ICL
    /// retriever to canonicalise the user's question against the
    /// same lindera + glossary user-dict the index-time
    /// `tokenized_text` was stamped with. Recall consistency by
    /// construction: the FTS ranker's
    /// `plainto_tsquery('simple', tokenize(question))` lemmas
    /// match the `searchable_tsv` column without case/morphology
    /// drift. `None` → hybrid retriever degrades to passing the
    /// raw question on the FTS arm (still functional, just less
    /// recall on Korean/compound tokens).
    tokenizer_registry: Option<Arc<ox_text::WorkspaceTokenizerRegistry>>,
}

impl DefaultBrain {
    pub fn new(
        client_pool: Arc<client_pool::ClientPool>,
        provider_configs: impl IntoIterator<Item = auth::LlmProviderConfig>,
        model_resolver: Arc<dyn model_resolver::ModelResolver>,
        prompts: PromptRegistry,
        default_model: ProviderInfo,
    ) -> Self {
        let provider_configs = provider_configs
            .into_iter()
            .map(|config| (config.provider.clone(), config))
            .collect();
        Self {
            client_pool,
            provider_configs,
            model_resolver,
            prompts,
            default_model,
            memory: None,
            ontology_lineage_id: None,
            knowledge_store: None,
            evaluation_capture: None,
            inference_session_store: None,
            verified_query_store: None,
            embedder: None,
            tokenizer_registry: None,
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

    /// Attach an inference-session store. When set + an
    /// `InferenceContext` is bound on the calling task,
    /// `translate_query` records one `InferenceAttempt` per LLM
    /// iteration. The same `Arc<dyn InferenceSessionStore>` the
    /// outer pipeline driver uses to open the session — typically
    /// the canonical `PostgresStore`.
    pub fn with_inference_session_store(
        mut self,
        store: Arc<dyn ox_store::InferenceSessionStore>,
    ) -> Self {
        self.inference_session_store = Some(store);
        self
    }

    /// Attach a verified-query store (Φ11.2). When set,
    /// `translate_query` short-circuits on an exact-hash hit
    /// against the bank — the verified IR returns directly
    /// without schema discovery, knowledge RAG, or any LLM call.
    /// Cache miss falls through to the full translate path.
    /// Typically the canonical `PostgresStore` Arc shared with
    /// the rest of the platform.
    pub fn with_verified_query_store(
        mut self,
        store: Arc<dyn ox_store::VerifiedQueryStore>,
    ) -> Self {
        self.verified_query_store = Some(store);
        self
    }

    /// Attach an embedding provider for semantic NN retrieval
    /// over the verified-query bank (Φ11.5). When set, the ICL
    /// retriever prefers the embedding path; when absent it
    /// falls through to trigram. The same `Arc` the
    /// `MemoryStore` carries — sharing the embedder keeps the
    /// dimension contract trivially aligned.
    pub fn with_embedder(mut self, embedder: Arc<dyn ox_memory::EmbeddingProvider>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// Attach the workspace tokenizer registry. Hybrid ICL
    /// retrieval uses it to lemmatise the runtime question
    /// against the same dict the index-time `tokenized_text`
    /// was stamped with — without it the FTS arm of the RRF
    /// hybrid passes the raw question, which still hits the
    /// GIN index but loses morphology-driven recall on Korean
    /// compounds + glossary canonicalisations.
    pub fn with_tokenizer_registry(
        mut self,
        registry: Arc<ox_text::WorkspaceTokenizerRegistry>,
    ) -> Self {
        self.tokenizer_registry = Some(registry);
        self
    }

    /// Embedder accessor used by upstream surfaces (e.g. the
    /// verified-query promotion route) that need to embed text
    /// against the same provider the Brain consults.
    pub fn embedder(&self) -> Option<&Arc<dyn ox_memory::EmbeddingProvider>> {
        self.embedder.as_ref()
    }

    /// Record one `InferenceAttempt` for a translate-query call.
    /// Best-effort: short-circuits silently when no
    /// `InferenceContext` is bound on the calling task or no
    /// store is attached. Errors during recording log + drop —
    /// the LLM call already returned, so audit failure cannot
    /// rewind the user's response.
    ///
    /// The `parent_attempt_id` is left `None` because Brain-side
    /// retry (Tier1/2/3 + label correction) is one logical
    /// attempt from the InferenceSession's perspective. Multi-
    /// attempt chains arise at the Agent level — the Agent's
    /// outer loop opens / re-invokes translate_query and
    /// successive sessions chain via the agent's bookkeeping.
    pub(crate) async fn record_translate_outcome(
        &self,
        stage: ox_ontology::PipelineStage,
        query_ir: Option<&ox_query_ir::query::QueryIR>,
        provenance: Option<&CallProvenance>,
        outcome: ox_ontology::AttemptOutcome,
    ) {
        let Some(ctx) = ox_store::current_inference_context() else {
            return;
        };
        let Some(store) = self.inference_session_store.as_ref() else {
            return;
        };

        let candidate = query_ir.and_then(|q| serde_json::to_value(q).ok());
        let capture = provenance.map(|p| {
            let plan = ox_ontology::ProvenancePlan {
                template_id: p.prompt_id.clone(),
                template_version: p.prompt_version.clone(),
                prompt_render_hash: p.prompt_render_hash.clone(),
            };
            ox_ontology::ProvenanceCapture::draft_proposal(plan, p.model_id.clone())
        });

        if let Err(err) = store
            .record_inference_attempt(
                ctx.session_id,
                None,
                stage,
                candidate,
                outcome,
                capture,
            )
            .await
        {
            tracing::warn!(
                error = %err,
                session_id = %ctx.session_id,
                stage = stage.as_str(),
                "inference attempt recording failed (best-effort)"
            );
        }
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
    /// Uses a cached client when available, otherwise creates one lazily
    /// from the provider config selected at startup. Credential failures
    /// are scoped to the LLM operation instead of preventing API boot.
    async fn resolve_for_operation(
        &self,
        operation: &str,
    ) -> OxResult<(Arc<dyn branchforge::LlmCall>, model_resolver::ResolvedModel)> {
        let resolved = self.model_resolver.resolve(operation).await?;
        let client = if let Some(config) = resolved.provider_config.as_ref() {
            self.client_pool.get_or_create(config).await?
        } else if let Some(client) = self.client_pool.by_provider(&resolved.provider) {
            client
        } else {
            let config = self
                .provider_configs
                .get(&resolved.provider)
                .ok_or_else(|| OxError::Runtime {
                    message: format!(
                        "No LLM provider config registered for '{}'.",
                        resolved.provider
                    ),
                })?;
            self.client_pool.get_or_create(config).await?
        };
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
    ///
    /// ## OpenTelemetry GenAI semantic conventions
    ///
    /// The span carries the `gen_ai.*` attributes from the OTel
    /// GenAI semantic conventions (Φ9.4) — Phoenix Arize /
    /// Langfuse / any OTLP collector that recognises the
    /// convention auto-categorises every call as an LLM request
    /// without a downstream mapper. The dotted field names are
    /// declared via [`tracing::info_span!`] (the
    /// `#[instrument]` proc-macro rejects quoted-string field
    /// keys, but `info_span!` accepts them per the tracing
    /// field-name syntax). Fields land empty at entry and are
    /// stamped via `span.record(...)` as the call progresses.
    async fn call_structured_traced<T: serde::de::DeserializeOwned + schemars::JsonSchema>(
        &self,
        prompt_name: &str,
        min_version: Option<&str>,
        operation: &str,
        vars: &HashMap<&str, &str>,
        log_message: &str,
    ) -> OxResult<(T, CallProvenance)> {
        let span = tracing::info_span!(
            "gen_ai.call",
            prompt_name = prompt_name,
            prompt_version = tracing::field::Empty,
            "gen_ai.operation.name" = operation,
            "gen_ai.system" = tracing::field::Empty,
            "gen_ai.request.model" = tracing::field::Empty,
            "gen_ai.request.max_tokens" = tracing::field::Empty,
            "gen_ai.request.temperature" = tracing::field::Empty,
            "gen_ai.usage.input_tokens" = tracing::field::Empty,
            "gen_ai.usage.output_tokens" = tracing::field::Empty,
        );
        let _enter = span.enter();

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

        span.record("prompt_version", tmpl.version.as_str());

        let (client, resolved) = self.resolve_for_operation(operation).await?;
        let effective_max_tokens = resolved.max_tokens.unwrap_or(tmpl.max_tokens);
        let effective_temperature = resolved.temperature.or(tmpl.temperature);

        // OTel GenAI attrs — `gen_ai.system` is the provider id
        // (anthropic / openai / google / bedrock / …),
        // `gen_ai.request.model` the resolved model identifier.
        // Both stamp at this point so an OTel collector reading
        // the span mid-flight (long completions, streamed
        // responses) already knows which provider + model it's
        // observing.
        span.record("gen_ai.system", resolved.provider.as_str());
        span.record("gen_ai.request.model", resolved.model_id.as_str());
        span.record("gen_ai.request.max_tokens", effective_max_tokens);
        if let Some(t) = effective_temperature {
            span.record("gen_ai.request.temperature", t as f64);
        }

        info!(
            model = %resolved.model_id,
            operation,
            prompt_version = %tmpl.version,
            "{log_message}"
        );

        let started = std::time::Instant::now();
        let (parsed, usage) = structured_completion(
            client.as_ref(),
            &resolved.model_id,
            &tmpl.system,
            &user_prompt,
            effective_max_tokens,
            effective_temperature,
        )
        .await?;
        let elapsed_ms = started.elapsed().as_millis() as i64;

        // Stamp usage attrs after completion so the span carries
        // the canonical OTel GenAI token-count axes alongside the
        // request side.
        span.record("gen_ai.usage.input_tokens", usage.input_tokens);
        span.record("gen_ai.usage.output_tokens", usage.output_tokens);

        // Evaluation capture hook — one call → one
        // `EvaluationCapture::record_call` invocation that lands
        // latency / token (input + output + cached_input) / cost
        // metric rows in lockstep. Production traffic without an
        // active evaluation scope skips for free; both branches
        // short-circuit when their condition is missing.
        if let (Some(ctx), Some(capture)) = (
            ox_store::current_evaluation_context(),
            self.evaluation_capture.as_ref(),
        ) {
            let call = ox_ontology::ModelCall {
                model_id: ox_ontology::ModelId::new(resolved.model_id.clone()),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                // Provider TokenUsage doesn't surface cache-hit
                // counts yet — Φ8.4 leaves the field at 0 and a
                // follow-up plumbs `cache_read_input_tokens`
                // through the branchforge response shape.
                cached_input_tokens: 0,
                latency_ms: elapsed_ms.max(0).min(u32::MAX as i64) as u32,
            };
            // Capture-side failures don't propagate — the LLM call
            // already succeeded, the operator's primary path is the
            // typed response. Log + drop matches the wider
            // observability policy (capture is best-effort, not
            // load-bearing).
            if let Err(err) = capture.record_call(&ctx, operation, call).await {
                tracing::warn!(error = %err, operation, "evaluation capture record_call failed");
            }
        }

        // Render hash captures system + user post-interpolation.
        // An admin who edits the DB-backing prompt without bumping
        // `prompt_version` shifts this value, and `ContentBody`
        // re-hashes against the new provenance — the prior artifact
        // id no longer matches and the operator sees the divergence
        // rather than a silent cache hit.
        let prompt_render_hash =
            ox_ontology::source_mapping::ArtifactProvenance::compute_prompt_render_hash(&format!(
                "{}\n\n{}",
                tmpl.system, user_prompt
            ));

        Ok((
            parsed,
            CallProvenance {
                prompt_id: prompt_name.to_string(),
                prompt_version: tmpl.version.clone(),
                provider: resolved.provider,
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
    pub provider: String,
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
        params.insert(
            "provider".into(),
            serde_json::Value::from(self.provider.clone()),
        );
        params.insert(
            "max_tokens".into(),
            serde_json::Value::from(self.max_tokens),
        );
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

/// Per-million-token tariff (input, output) in micro-USD. Wide
mod default_brain;
