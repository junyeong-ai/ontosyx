//! [`DomainContext`] — shared dependency bag every domain tool reads.
//!
//! Threaded into the registry once at agent build, then cloned (cheap
//! `Arc` bump) into each tool. Tools that mutate the ontology
//! (`apply_ontology`, `edit_ontology`) publish a replacement through
//! [`DomainContext::replace_ontology`]; readers pick the latest
//! snapshot from the [`ArcSwap`] slot at the start of every
//! invocation, so a schema edit takes effect on the next tool call
//! without rebuilding the context.

use std::sync::Arc;

use arc_swap::ArcSwap;

use ox_compiler::GraphCompiler;
use ox_graph_runtime::GraphRuntime;
use ox_ontology::ir::OntologyIR;
use ox_store::Store;

use crate::clarification_tracker;

/// Shared state for all agent tools — graph backends, store, and
/// current ontology context.
///
/// `ontology` is an [`ArcSwap`] (not a plain `Arc`) so tools that
/// mutate the ontology can publish the new snapshot without rebuilding
/// the context. Downstream tools in the same session read the latest
/// snapshot right before wrapping `runtime.execute_query` in
/// `GRAPH_ONTOLOGY.scope`, so schema edits take effect on the very
/// next query.
pub struct DomainContext {
    pub compiler: Arc<dyn GraphCompiler>,
    pub runtime: Option<Arc<dyn GraphRuntime>>,
    pub store: Arc<dyn Store>,
    pub ontology: Option<ArcSwap<OntologyIR>>,
    pub user_id: String,
    pub workspace_id: uuid::Uuid,
    /// Identity of the ontology this session is pinned to (matches
    /// `ontologies.id`). `None` for ad-hoc sessions operating on a
    /// draft IR that has not been committed through
    /// `OntologyVersionStore` yet.
    pub ontology_id: Option<uuid::Uuid>,
    pub ontology_draft_id: Option<uuid::Uuid>,
    pub ontology_draft_revision: Option<i32>,
    /// Source schema for introspection (available once the source has
    /// been analysed).
    pub source_schema: Option<ox_core::source_schema::SourceSchema>,
    /// Source profile (column statistics) for introspection.
    pub source_profile: Option<ox_core::source_schema::SourceProfile>,
    /// Repo analysis summary (framework, domain notes, field hints)
    /// from the initial source analysis.
    pub repo_insights: Option<ox_ontology::repo_insights::RepoInsights>,
    /// Knowledge store for failure-driven learning corrections.
    pub knowledge_store: Option<Arc<dyn ox_store::KnowledgeStore>>,
    /// Ambiguity resolver store. Wired when the session has access to
    /// a source — the `resolve_ambiguity` tool is registered only when
    /// this is populated, so ad-hoc sessions without a source surface
    /// aren't offered a tool that has nothing to resolve against.
    pub ambiguity_store: Option<Arc<dyn ox_store::AmbiguityStore>>,
    /// Per-agent-process "session has resolved an ambiguity recently"
    /// tracker. Populated by `ResolveAmbiguityTool` and read by
    /// `QueryGraphTool` so the `clarification_success_rate` quality
    /// signal can flip `ambiguity_was_clarified = true` on a query
    /// that followed a clarification in the same session.
    pub clarification_tracker: clarification_tracker::SharedClarificationTracker,
    /// Original user question — always passed to translate_query as
    /// primary context. Prevents agent-driven question fragmentation
    /// that defeats graph traversal.
    pub user_question: Option<String>,
    /// Workspace tokenizer registry. Hybrid retrieval over the
    /// community / verified-query / knowledge banks consults the same
    /// `Arc<dyn Tokenizer>` the index-time pipeline used so recall
    /// stays consistent. `None` → retrieval degrades to raw-text
    /// rankers (still functional, less recall on Korean compounds +
    /// glossary canonicalisations).
    pub tokenizer_registry: Option<Arc<ox_text::WorkspaceTokenizerRegistry>>,
    /// Embedding provider — same `Arc` the Brain consumes. The
    /// retrieval fan-out embeds the question once and threads the
    /// vector into every hybrid SQL call; cold-start workspaces
    /// without an embedder still hit the trigram + FTS arms.
    pub embedder: Option<Arc<dyn ox_memory::EmbeddingProvider>>,
}

impl DomainContext {
    /// Load the current ontology snapshot. Returns `None` when no
    /// ontology has been attached to this session. Callers that need
    /// a short-lived reference should hold the `Arc` across a single
    /// tool invocation rather than for the entire session so a
    /// mid-session edit can publish a replacement.
    pub fn current_ontology(&self) -> Option<Arc<OntologyIR>> {
        self.ontology.as_ref().map(|o| o.load_full())
    }

    /// Publish a replacement ontology. Called by tools that mutate
    /// the ontology (e.g. `apply_ontology`, `edit_ontology`) so every
    /// subsequent tool in the session sees the new snapshot.
    ///
    /// Returns `true` when a replacement was stored, `false` when the
    /// session has no ontology slot (and therefore no subscribers).
    pub fn replace_ontology(&self, ontology: OntologyIR) -> bool {
        match &self.ontology {
            Some(slot) => {
                slot.store(Arc::new(ontology));
                true
            }
            None => false,
        }
    }
}
