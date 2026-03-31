pub mod cypher;
pub mod export;
pub mod import;

use std::collections::HashMap;

use ox_core::error::OxResult;
use ox_core::load_plan::LoadPlan;
use ox_core::ontology_ir::OntologyIR;
use ox_core::query_ir::QueryIR;
use ox_core::types::PropertyValue;

// ---------------------------------------------------------------------------
// CompiledQuery — parameterized query output
// ---------------------------------------------------------------------------

/// A compiled query with its parameterized statement and bound parameters.
/// Parameters use `$pN` placeholders (e.g. `$p0`, `$p1`) to prevent injection.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CompiledQuery {
    pub statement: String,
    pub params: HashMap<String, PropertyValue>,
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

    /// Compile a QueryIR into a parameterized query
    fn compile_query(&self, query: &QueryIR) -> OxResult<CompiledQuery>;

    /// Compile a LoadPlan into batch load statements
    fn compile_load(&self, plan: &LoadPlan) -> OxResult<Vec<String>>;

    /// Return the name of this compilation target (for error messages)
    fn target_name(&self) -> &str;
}
