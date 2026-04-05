mod expr;
mod load;
pub mod migration;
mod mutate;
mod params;
mod pattern;
mod query;
mod schema;
pub use migration::DataMigrationStep;
#[cfg(test)]
mod tests;

use ox_core::error::OxResult;
use ox_core::load_plan::{LoadPlan, LoadStep};
use ox_core::ontology_ir::OntologyIR;
use ox_core::query_ir::QueryIR;

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
/// - `Memgraph`: Bolt-compatible subset. Generates Neo4j 5.x DDL (constraints + indexes);
///   the runtime rewrites statements to Memgraph syntax and filters unsupported types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CypherDialect {
    /// Full Neo4j Cypher (default). Supports all features.
    #[default]
    Neo4j,
    /// openCypher subset (Neptune). No shortestPath, no APOC,
    /// no CREATE INDEX, limited MERGE.
    OpenCypher,
    /// Memgraph Cypher. DDL is generated in Neo4j 5.x format and rewritten
    /// by the MemGraphRuntime to Memgraph-compatible syntax at execution time.
    /// No full-text/vector indexes, no NODE KEY constraints, no APOC/GDS.
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

    /// Create a CypherCompiler targeting the openCypher subset (Neptune).
    pub fn open_cypher() -> Self {
        Self {
            dialect: CypherDialect::OpenCypher,
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
    fn target_name(&self) -> &str {
        match self.dialect {
            CypherDialect::Neo4j => "Cypher (Neo4j)",
            CypherDialect::OpenCypher => "openCypher (Neptune)",
            CypherDialect::Memgraph => "Cypher (Memgraph)",
        }
    }

    fn compile_schema(&self, ontology: &OntologyIR) -> OxResult<Vec<String>> {
        // openCypher backends (Neptune) handle indexes automatically and
        // do not support CREATE CONSTRAINT / CREATE INDEX DDL.
        if self.dialect == CypherDialect::OpenCypher {
            tracing::info!(
                dialect = ?self.dialect,
                "openCypher dialect: skipping schema DDL (indexes are automatic)"
            );
            return Ok(Vec::new());
        }

        let mut statements = Vec::new();

        for node in &ontology.node_types {
            statements.extend(compile_node_constraints(node));
        }

        // Explicit indices from ontology.indexes (never capped)
        for index in &ontology.indexes {
            statements.push(compile_index(ontology, index));
        }

        // Auto-generated range indices (priority-sorted, capped)
        let (auto_indices, stats) = compile_auto_indices(ontology);
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
