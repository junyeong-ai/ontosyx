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
use ox_ontology::ir::OntologyIR;
use ox_ontology::load_plan::{LoadPlan, LoadStep};
use ox_query_ir::query::QueryIR;

use crate::concept_map_rewrite::{build_translation_table_for_query, rewrite_concept_map_values};
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

    fn compile_query(
        &self,
        query: &QueryIR,
        ontology: Option<&OntologyIR>,
    ) -> OxResult<CompiledQuery> {
        // The IR carries a temporal AS-OF field but the Cypher emitter
        // has no snapshot-resolution path yet. Refuse rather than
        // silently ignore the timestamp — the upstream
        // TemporalRewriter pass is expected to feed the compiler a
        // temporally-resolved QueryIR with `as_of = None`.
        if query.as_of.is_some() {
            return Err(OxError::Compilation {
                message: "temporal AS-OF queries are not yet supported by the Cypher compiler — \
                          resolve the snapshot via a TemporalRewriter pass before compiling"
                    .to_string(),
            });
        }

        // Concept-map rewrite — translates literal codes onto the
        // binding's canonical vocabulary before emission. Runs once
        // per compile here so every backend (and every call site —
        // agent tool, direct API, MCP, dashboards, federation) gets
        // the same safety net without re-implementing the funnel.
        // Short-circuits cleanly when the ontology carries no concept
        // maps, or when the caller explicitly opts out of rewrite by
        // passing `None` (raw QueryIR with no semantic context).
        let (rewritten_query, concept_map_report) = match ontology {
            Some(ont) => {
                let translation_table = build_translation_table_for_query(query, ont);
                if translation_table.is_empty() {
                    (query.clone(), Default::default())
                } else {
                    let (q, report) = rewrite_concept_map_values(query.clone(), &translation_table);
                    tracing::debug!(
                        fired = report.translations.len(),
                        untranslated = report.untranslated.len(),
                        "ConceptMap rewrite applied"
                    );
                    (q, report)
                }
            }
            None => (query.clone(), Default::default()),
        };

        let mut parts = Vec::new();
        let mut collector = ParamCollector::new(self.dialect);

        compile_op(&rewritten_query.operation, &mut parts, &mut collector)?;

        if !rewritten_query.order_by.is_empty() {
            parts.push(compile_order_by(&rewritten_query.order_by, &mut collector)?);
        }

        // SKIP/LIMIT stay inline (integers, safe for query plan caching)
        if let Some(skip) = rewritten_query.skip {
            parts.push(format!("SKIP {skip}"));
        }

        if let Some(limit) = rewritten_query.limit {
            parts.push(format!("LIMIT {limit}"));
        }

        Ok(CompiledQuery {
            statement: parts.join("\n"),
            params: collector.into_map(),
            concept_map_report,
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
