#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

pub mod auto_distinct;
pub mod concept_map_rewrite;
pub mod cost;
pub mod cypher;
pub mod export;
pub mod import;
pub mod plan_cache;
pub mod provenance;
pub mod temporal;

pub use auto_distinct::rewrite_auto_distinct;
pub use concept_map_rewrite::{
    RewriteReport, TranslationDirection, TranslationEvent, TranslationPolicy, TranslationTable,
    UntranslatedLiteral, build_translation_table_for_query, rewrite_concept_map_values,
};
pub use plan_cache::{DEFAULT_PLAN_CACHE_CAPACITY, PlanCache, PlanCacheHandle, PlanCacheStats};
pub use provenance::{ProvenanceContext, build_provenance};
pub use temporal::{rewrite_temporal, rewrite_temporal_with_renames};

use std::collections::HashMap;

use ox_core::error::OxResult;
use ox_core::types::PropertyValue;
use ox_ontology::ir::OntologyIR;
use ox_ontology::load_plan::LoadPlan;
use ox_query_ir::query::QueryIR;

// ---------------------------------------------------------------------------
// CompiledQuery — parameterized query output
// ---------------------------------------------------------------------------

/// A compiled query with its parameterized statement and bound parameters.
/// Parameters use `$pN` placeholders (e.g. `$p0`, `$p1`) to prevent injection.
///
/// `concept_map_report` carries the rewrite trace for callers that
/// surface "translated `A` → `ACTIVE` via map X" guidance to the
/// operator (the agent tool path renders `untranslated` literals as
/// hints). Empty for queries against ontologies without concept maps.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct CompiledQuery {
    pub statement: String,
    pub params: HashMap<String, PropertyValue>,
    #[serde(default, skip_serializing_if = "RewriteReport::is_empty")]
    pub concept_map_report: RewriteReport,
}

// ---------------------------------------------------------------------------
// GraphCompiler trait — the compilation boundary
//
// Each backend (Cypher, openCypher, GQL, Gremlin) implements this trait.
// Adding a new graph DB = implementing this trait. Zero changes elsewhere.
// ---------------------------------------------------------------------------

pub trait GraphCompiler: Send + Sync {
    /// Compile an OntologyIR into schema DDL statements
    fn compile_schema(&self, ontology: &OntologyIR) -> OxResult<Vec<String>>;

    /// Compile a QueryIR into a parameterized query.
    ///
    /// `ontology` is consulted for the concept-map rewrite that
    /// translates source-vocabulary literals onto the binding's
    /// canonical vocabulary before emission — every backend gets
    /// the safety automatically by routing through this single
    /// entry point. The rewrite is a no-op when the ontology
    /// carries no concept maps (the table builder short-circuits).
    ///
    /// `None` is the explicit "raw QueryIR, no ontology context"
    /// signal — the planner skips the rewrite. Production routes
    /// that have an `ontology_id` should always pass `Some` so
    /// concept-map bindings actually fire; passing `None` against
    /// an ontology with concept maps is a silent-corruption escape
    /// hatch that the type signature now forces a caller to choose
    /// deliberately.
    fn compile_query(
        &self,
        query: &QueryIR,
        ontology: Option<&OntologyIR>,
    ) -> OxResult<CompiledQuery>;

    /// Compile a LoadPlan into batch load statements
    fn compile_load(&self, plan: &LoadPlan) -> OxResult<Vec<String>>;

    /// Return the name of this compilation target (for error messages)
    fn name(&self) -> &str;
}

// Blanket impl so an `Arc<dyn GraphCompiler>` (the AppState-side handle)
// can itself satisfy the `GraphCompiler` bound. This lets
// `PlanCache<Arc<dyn GraphCompiler>>` wrap an existing registry-built
// compiler without forcing callers to name the concrete type.
impl<T: GraphCompiler + ?Sized> GraphCompiler for std::sync::Arc<T> {
    fn compile_schema(&self, ontology: &OntologyIR) -> OxResult<Vec<String>> {
        T::compile_schema(self, ontology)
    }

    fn compile_query(
        &self,
        query: &QueryIR,
        ontology: Option<&OntologyIR>,
    ) -> OxResult<CompiledQuery> {
        T::compile_query(self, query, ontology)
    }

    fn compile_load(&self, plan: &LoadPlan) -> OxResult<Vec<String>> {
        T::compile_load(self, plan)
    }

    fn name(&self) -> &str {
        T::name(self)
    }
}
