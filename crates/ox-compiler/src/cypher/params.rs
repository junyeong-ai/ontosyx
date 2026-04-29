use std::collections::HashMap;

use ox_core::types::{PropertyValue, escape_cypher_identifier};

use super::CypherDialect;

// ---------------------------------------------------------------------------
// ParamCollector — accumulates query parameters for parameterization
// ---------------------------------------------------------------------------
//
// Carries the active [`CypherDialect`] so every emitter that takes a
// `ParamCollector` (the canonical compile-side context object) can
// gate dialect-specific syntax without threading an extra parameter
// through every function in the lowering tree (ADR-0036).

pub(crate) struct ParamCollector {
    params: Vec<(String, PropertyValue)>,
    counter: usize,
    dialect: CypherDialect,
}

impl ParamCollector {
    pub(crate) fn new(dialect: CypherDialect) -> Self {
        Self {
            params: Vec::new(),
            counter: 0,
            dialect,
        }
    }

    pub(crate) fn dialect(&self) -> CypherDialect {
        self.dialect
    }

    /// Push a value and return the `$pN` placeholder string.
    pub(crate) fn push(&mut self, val: PropertyValue) -> String {
        let name = format!("p{}", self.counter);
        self.counter += 1;
        self.params.push((name.clone(), val));
        format!("${name}")
    }

    pub(crate) fn into_map(self) -> HashMap<String, PropertyValue> {
        self.params.into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// Identifier escaping — prevents injection via labels, property names, etc.
// ---------------------------------------------------------------------------

/// Backtick-escapes a Neo4j identifier (label, property name, relationship type).
/// Delegates to the shared `escape_cypher_identifier` in ox-core.
pub(crate) fn escape_identifier(name: &str) -> String {
    escape_cypher_identifier(name)
}
