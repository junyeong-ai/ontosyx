mod expr;
mod load;
pub mod migration;
mod mutate;
mod params;
mod pattern;
mod query;
pub mod schema;
pub use migration::DataMigrationStep;
#[cfg(test)]
mod tests;

use ox_core::error::{OxError, OxResult};
use ox_ontology::load_plan::{LoadPlan, LoadStep};
use ox_ontology::ir::OntologyIR;
use ox_query_ir::query::QueryIR;

use crate::{CompiledQuery, GraphCompiler};

use expr::compile_order_by;
use load::compile_load_op;
use params::ParamCollector;
use query::compile_op;
pub use schema::IndexStats;
use schema::{compile_auto_indices, compile_index, compile_node_constraints};

// ---------------------------------------------------------------------------
// CypherDialect — target-specific Cypher flavor
// ---------------------------------------------------------------------------

/// Cypher dialect controls which features are available during compilation.
///
/// - `Neo4j`: Full Cypher with APOC, shortestPath, MERGE ON CREATE/ON MATCH, CREATE INDEX.
/// - `OpenCypher`: Subset supported by Neptune. No shortestPath, no APOC,
///   no CREATE INDEX/CONSTRAINT DDL.
/// - `Memgraph`: Bolt-compatible subset. Generates native Memgraph DDL
///   directly (constraints in 4.x `ASSERT` form, indexes in `CREATE INDEX
///   ON :Label(prop)` form). No full-text/vector indexes, no NODE KEY
///   constraints, no APOC/GDS. The runtime is now a straight pass-through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CypherDialect {
    /// Full Neo4j Cypher (default). Supports all features.
    #[default]
    Neo4j,
    /// Memgraph Cypher. The compiler emits Memgraph-native DDL in
    /// `compile_schema` — no runtime string rewriting.
    Memgraph,
}

// ---------------------------------------------------------------------------
// CypherCompiler — IR → Cypher (Neo4j or openCypher dialect)
// ---------------------------------------------------------------------------

pub struct CypherCompiler {
    pub dialect: CypherDialect,
}

impl CypherCompiler {
    /// Create a CypherCompiler targeting full Neo4j Cypher.
    pub fn neo4j() -> Self {
        Self {
            dialect: CypherDialect::Neo4j,
        }
    }

    /// Create a CypherCompiler targeting Memgraph.
    /// Generates Neo4j 5.x-style DDL; the MemGraphRuntime rewrites it at execution.
    pub fn memgraph() -> Self {
        Self {
            dialect: CypherDialect::Memgraph,
        }
    }
}

impl GraphCompiler for CypherCompiler {
    fn name(&self) -> &str {
        match self.dialect {
            CypherDialect::Neo4j => "Cypher (Neo4j)",
            CypherDialect::Memgraph => "Cypher (Memgraph)",
        }
    }

    fn compile_schema(&self, ontology: &OntologyIR) -> OxResult<Vec<String>> {
        let mut statements = Vec::new();

        for node in ontology.node_types() {
            statements.extend(compile_node_constraints(node, self.dialect));
        }

        // Explicit indices from ontology.indexes (never capped). Some
        // index kinds are unrepresentable in certain dialects (e.g.
        // FULLTEXT / VECTOR on Memgraph); `compile_index` returns None
        // for those so the compiler drops them cleanly instead of
        // handing a bogus statement to the runtime.
        for index in ontology.indexes() {
            if let Some(stmt) = compile_index(ontology, index, self.dialect) {
                statements.push(stmt);
            }
        }

        // Auto-generated range indices (priority-sorted, capped)
        let (auto_indices, stats) = compile_auto_indices(ontology, self.dialect);
        statements.extend(auto_indices);

        tracing::info!(
            total = stats.total,
            explicit = stats.explicit,
            auto_generated = stats.auto_generated,
            truncated = stats.truncated,
            "Schema index compilation complete"
        );

        Ok(statements)
    }

    fn compile_query(&self, query: &QueryIR) -> OxResult<CompiledQuery> {
        // Phase 2-2 foundation: the IR now carries a temporal AS-OF field
        // but the Cypher emitter has no snapshot-resolution path yet. Fail
        // loudly with a clear message rather than silently ignore the
        // timestamp and return stale-against-caller-intent results. The
        // dedicated TemporalRewriter pass (future commit) will consume the
        // field upstream and hand the compiler a temporally-resolved
        // QueryIR with `as_of = None`.
        if query.as_of.is_some() {
            return Err(OxError::Compilation {
                message: "temporal AS-OF queries are not yet supported by the Cypher compiler — \
                          resolve the snapshot via a TemporalRewriter pass before compiling"
                    .to_string(),
            });
        }

        let mut parts = Vec::new();
        let mut collector = ParamCollector::new();

        compile_op(&query.operation, &mut parts, &mut collector)?;

        if !query.order_by.is_empty() {
            parts.push(compile_order_by(&query.order_by, &mut collector)?);
        }

        // SKIP/LIMIT stay inline (integers, safe for query plan caching)
        if let Some(skip) = query.skip {
            parts.push(format!("SKIP {skip}"));
        }

        if let Some(limit) = query.limit {
            parts.push(format!("LIMIT {limit}"));
        }

        Ok(CompiledQuery {
            statement: parts.join("\n"),
            params: collector.into_map(),
        })
    }

    fn compile_load(&self, plan: &LoadPlan) -> OxResult<Vec<String>> {
        let mut statements = Vec::new();

        // Sort steps by execution order
        let mut steps: Vec<&LoadStep> = plan.steps.iter().collect();
        steps.sort_by_key(|s| s.order);

        for step in steps {
            statements.push(compile_load_op(&step.operation)?);
        }

        Ok(statements)
    }
}
