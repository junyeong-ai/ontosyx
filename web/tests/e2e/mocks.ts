// Type-safe wire-shape factories for Playwright E2E specs.
//
// WHY THIS FILE EXISTS
//
// Hand-written mock objects that miss a required field end up in
// `page.route().fulfill()` bodies, reach the UI, and trip a runtime
// `x.map(...)` on `undefined` — the page then swaps to the error
// boundary and the test fails with "locator not visible" instead of
// something helpful. We hit this once this month (the glossary
// scorer spec: mock had `reasons` but not `signals`).
//
// Each factory:
//   1. Returns the canonical wire shape (every required field
//      populated with a safe default).
//   2. Takes `Partial<T>` for per-test overrides.
//   3. Has a return type annotated as the exported TS type, so
//      TypeScript fails the compile the moment a new required
//      field appears in the wire schema but the factory hasn't
//      been updated.
//
// DESIGN
//
// - No factory for trivially-shaped values (strings, uuids, dates).
//   Callers inline those.
// - Factories compose — `mockPropertyCandidate` reuses
//   `mockBindingSignal`. Overrides flow top-down.
// - Deterministic defaults: same factory called twice returns the
//   same shape (no `Math.random()`, no `Date.now()`). Tests that
//   need variation override explicitly.

// Binding-suggestion + edit types live under `lib/api/` rather than
// the public `@/types/api` barrel; ambiguity types mirror the same
// split. Importing from the owning module keeps the factories
// immune to barrel-reshaping.
import type {
  AmbiguityContext,
  AmbiguityMapping,
  AmbiguityResolution,
  AmbiguitySummary,
} from "@/lib/api/ambiguity";
import type {
  BindingSignal,
  OntologyEditReceipt,
  PropertyCandidate,
  SuggestBindingsResponse,
} from "@/lib/api/binding-suggestions";
import type {
  CurrentVersionSummary,
  OntologyDetail,
  OntologyIR,
  OntologyListItem,
  QueryDiagnostic,
  QueryMetadata,
  QueryProvenance,
  QueryResult,
} from "@/types/api";

// Stable uuid-ish values for cross-spec referencing. Tests that
// need conflict-free ids override these per-call.
export const MOCK_ONTOLOGY_ID = "00000000-0000-0000-0000-00000000abcd";
export const MOCK_WORKSPACE_ID = "00000000-0000-0000-0000-000000000000";
export const MOCK_USER_ID = "00000000-0000-0000-0000-000000000001";

// ---------------------------------------------------------------------------
// Binding suggestions
// ---------------------------------------------------------------------------

export function mockBindingSignal(
  overrides?: Partial<BindingSignal>,
): BindingSignal {
  return {
    kind: "canonical_name",
    ...overrides,
  } as BindingSignal;
}

export function mockPropertyCandidate(
  overrides?: Partial<PropertyCandidate>,
): PropertyCandidate {
  return {
    owner_kind: "node",
    owner_type_id: "type-customer",
    owner_label: "Customer",
    property_id: "prop-email",
    property_name: "email",
    score: 0.8,
    signals: [mockBindingSignal()],
    ...overrides,
  };
}

export function mockSuggestBindingsResponse(
  overrides?: Partial<SuggestBindingsResponse>,
): SuggestBindingsResponse {
  return {
    ontology_id: MOCK_ONTOLOGY_ID,
    candidates: [mockPropertyCandidate()],
    ...overrides,
  };
}

export function mockOntologyEditReceipt(
  overrides?: Partial<OntologyEditReceipt>,
): OntologyEditReceipt {
  return {
    new_version: 4,
    new_version_id: "00000000-0000-0000-0000-0000000000ee",
    parent_version_id: "00000000-0000-0000-0000-0000000000dd",
    applied_operations: 1,
    committed_at: "2026-04-23T00:00:00Z",
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Ambiguity contexts
// ---------------------------------------------------------------------------

export function mockAmbiguityContext(
  overrides?: Partial<AmbiguityContext>,
): AmbiguityContext {
  return {
    id: "ctx-1",
    source_id: "src-postgres",
    column: { relation: "orders", column: "status" },
    kind: { kind: "numeric_code" },
    sample_values: ["1", "2", "3"],
    clarification_prompt: "What does this code mean?",
    detection_source_hash: "sha256:abc",
    detected_at: "2026-04-23T00:00:00Z",
    ...overrides,
  };
}

export function mockAmbiguityResolution(
  overrides?: Partial<AmbiguityResolution>,
): AmbiguityResolution {
  return {
    id: "res-1",
    context_id: "ctx-1",
    context_source_hash: "sha256:abc",
    mapping: { kind: "glossary_ref", term_id: "term-x" } as AmbiguityMapping,
    resolved_at: "2026-04-23T00:00:00Z",
    ...overrides,
  };
}

export function mockAmbiguitySummary(
  overrides?: Partial<AmbiguitySummary>,
): AmbiguitySummary {
  return {
    context: mockAmbiguityContext(),
    active_resolution: null,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Ontology identity + IR
// ---------------------------------------------------------------------------

export function mockCurrentVersionSummary(
  overrides?: Partial<CurrentVersionSummary>,
): CurrentVersionSummary {
  return {
    version: 3,
    version_id: "ver-3",
    ...overrides,
  } as CurrentVersionSummary;
}

export function mockOntologyListItem(
  overrides?: Partial<OntologyListItem>,
): OntologyListItem {
  return {
    id: MOCK_ONTOLOGY_ID,
    lineage_id: "lin-pilot",
    name: "Pilot",
    description: { default: "E2E test pilot" },
    created_at: "2026-04-22T00:00:00Z",
    updated_at: "2026-04-22T00:00:00Z",
    current_version: mockCurrentVersionSummary(),
    ...overrides,
  };
}

/** Minimal `OntologyIR`. Only the fields `ResponseBasis` /
 *  `CrossRefFlow` / other read-side components actually access. */
export function mockOntologyIR(overrides?: Partial<OntologyIR>): OntologyIR {
  return {
    metadata: {},
    node_types: [],
    edge_types: [],
    rules: [],
    ...overrides,
  } as OntologyIR;
}

export function mockOntologyDetail(
  overrides?: Partial<OntologyDetail>,
): OntologyDetail {
  return {
    id: MOCK_ONTOLOGY_ID,
    lineage_id: "lin-pilot",
    name: "Pilot",
    description: { default: "E2E test pilot" },
    created_at: "2026-04-22T00:00:00Z",
    updated_at: "2026-04-22T00:00:00Z",
    current_version: mockCurrentVersionSummary(),
    ontology_ir: mockOntologyIR(),
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Query results + provenance
// ---------------------------------------------------------------------------

export function mockQueryProvenance(
  overrides?: Partial<QueryProvenance>,
): QueryProvenance {
  return {
    ontology_id: MOCK_ONTOLOGY_ID,
    ontology_version: "v3",
    as_of: "2026-04-23T00:00:00Z",
    source_ids: ["src-postgres"],
    type_ids: ["type-customer"],
    filter_summary: "n.active = true",
    ...overrides,
  };
}

export function mockQueryDiagnostic(
  overrides?: Partial<QueryDiagnostic>,
): QueryDiagnostic {
  return {
    validator: "complexity",
    level: "warning",
    message: {
      code: "runtime.cypher.complexity.unbounded_var_length",
      message: "unbounded variable-length pattern",
    },
    ...overrides,
  };
}

export function mockQueryMetadata(
  overrides?: Partial<QueryMetadata>,
): QueryMetadata {
  return {
    execution_time_ms: 12,
    rows_returned: 1,
    provenance: mockQueryProvenance(),
    warnings: [],
    ...overrides,
  };
}

export function mockQueryResult(
  overrides?: Partial<QueryResult>,
): QueryResult {
  return {
    columns: ["n"],
    rows: [{ n: { id: 1, labels: ["Customer"] } }],
    metadata: mockQueryMetadata(),
    ...overrides,
  };
}
