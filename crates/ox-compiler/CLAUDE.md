# ox-compiler

IR → target query language compilers. Currently: Cypher (Neo4j + Memgraph, dialect-selected at construction). Adding a new graph DB = implement `GraphCompiler` trait.

## Module Layout

- `cypher/` — CypherCompiler: query.rs (QueryOp→Cypher), pattern.rs (GraphPattern→syntax), expr.rs (Expr→WHERE), mutate.rs (CREATE/MERGE/DELETE), schema.rs (DDL), migration.rs (schema diff), load.rs (batch load), params.rs (parameter binding).
- `cost.rs` — DB-agnostic query cost estimation (Cartesian detection, var-length depth, index/cardinality awareness).
- `export/` — OntologyIR → OWL/Turtle, SHACL, Python, TypeScript, GraphQL, Mermaid, Cypher DDL.
- `import/` — OWL/Turtle → OntologyIR.

## Cypher Dialects

`CypherCompiler::neo4j()` emits Neo4j 5.x DDL (`CREATE CONSTRAINT IF NOT EXISTS FOR (n:L) REQUIRE ... IS UNIQUE`, `CREATE INDEX IF NOT EXISTS FOR (n:L) ON (n.p)`, plus `FULLTEXT INDEX` / `VECTOR INDEX` / `NODE KEY`). `CypherCompiler::memgraph()` emits Memgraph 4.x DDL (`CREATE CONSTRAINT ON (n:L) ASSERT ... IS UNIQUE`, `CREATE INDEX ON :L(p)`) and drops features Memgraph doesn't support (`FULLTEXT` / `VECTOR` / `NODE KEY`) with an info log. The runtime is a straight pass-through — no string rewriting after compile-time.

`compile_migration(diff, old, new, dialect)` takes the dialect explicitly; every caller picks one. `compile_index` returns `Option<String>` so a dialect that can't represent an index kind (e.g. Memgraph + FULLTEXT) drops it cleanly instead of emitting a bogus statement.

## Cost Estimation

`estimate_cost(query, ontology)` analyses QueryIR before compilation. Uses OntologyIR to check index coverage and relationship cardinality. Returns `QueryCost` with `RiskLevel` (Low/Medium/High).

## Adding a New Export Format

1. Create `export/my_format.rs` with a `pub fn export(ontology: &OntologyIR) -> String`.
2. Register in `export/mod.rs`.
3. No trait needed — export is a one-way transformation, not a pluggable backend.
