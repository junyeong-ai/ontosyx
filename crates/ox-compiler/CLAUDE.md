# ox-compiler

IR → target query language compilers. Currently: Cypher (Neo4j). Adding a new graph DB = implement `GraphCompiler` trait.

## Module Layout

- `cypher/` — CypherCompiler: query.rs (QueryOp→Cypher), pattern.rs (GraphPattern→syntax), expr.rs (Expr→WHERE), mutate.rs (CREATE/MERGE/DELETE), schema.rs (DDL), migration.rs (schema diff), load.rs (batch load), params.rs (parameter binding).
- `cost.rs` — DB-agnostic query cost estimation (Cartesian detection, var-length depth, index/cardinality awareness).
- `export/` — OntologyIR → OWL/Turtle, SHACL, Python, TypeScript, GraphQL, Mermaid, Cypher DDL.
- `import/` — OWL/Turtle → OntologyIR.

## Cost Estimation

`estimate_cost(query, ontology)` analyses QueryIR before compilation. Uses OntologyIR to check index coverage and relationship cardinality. Returns `QueryCost` with `RiskLevel` (Low/Medium/High).

## Adding a New Export Format

1. Create `export/my_format.rs` with a `pub fn export(ontology: &OntologyIR) -> String`.
2. Register in `export/mod.rs`.
3. No trait needed — export is a one-way transformation, not a pluggable backend.
