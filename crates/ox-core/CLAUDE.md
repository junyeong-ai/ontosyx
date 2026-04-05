# ox-core

DB-agnostic IR type definitions. Zero heavy dependencies — every other crate depends on this.

## Key IRs

- `OntologyIR` — graph schema (node types, edge types, indexes, constraints).
- `QueryIR` — DB-agnostic query algebra. `QueryOp` has 8 variants (Match, PathFind, Aggregate, Union, Chain, CallSubquery, Mutate, Analytics).
- `MatchQueryIR` — simplified structured-output form for LLM generation, converted to QueryIR before compilation.
- `LoadPlan` — batch data load operations.
- `OntologyCommand` — incremental schema edit operations (add/delete/rename node/edge/property).

## Type-Safe IDs

`NodeTypeId`, `EdgeTypeId`, `PropertyId`, `ConstraintId` are newtype wrappers over String. They implement `Deref<Target=str>` and `PartialEq<str>` for ergonomic use but prevent accidental mixing.

## PropertyType

`PropertyType` enum covers: Bool, Int, Float, String, Date, DateTime, Duration, Bytes, List, Map. Key methods:
- `infer_from_db_type(db_type)` — maps raw DB type strings (e.g., "varchar", "int4") to PropertyType.
- `check_compatibility_with(db_type)` — returns None (match), Some(true) (safe widening), Some(false) (breaking).

## Error Handling

All errors use `OxResult<T>` = `Result<T, OxError>`. Key `OxError` variants: Compilation, Runtime, Validation, NotFound, Conflict, Contextual (wraps source + location for diagnostics). No `unwrap()` or `expect()` in this crate.

## Validation

`QueryIR::validate()` checks structural integrity (empty patterns, recursion). `OntologyIR::validate()` checks referential integrity (dangling IDs, duplicate labels).
