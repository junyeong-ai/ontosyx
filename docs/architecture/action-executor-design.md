# ActionExecutor — typed write surface for ontology mutations

**Status:** Design sketch — Phase 5 second half of the
long-horizon work plan. The trait + integration points are
documented here so the next session can land the
implementation without re-deriving the contract.

## Problem

`ox-ontology::action::ActionDef` already exists as a
first-class type — every NodeType / EdgeType can declare
typed write operations alongside its read shape. But the
agent surface has no path to *invoke* an `ActionDef`:

- `query_graph` is read-only.
- `apply_ontology` mutates the *schema*, not instance data.
- `edit_ontology` regenerates the schema from natural
  language; it does not invoke a typed action.

Foundry AIP's defining surface is the `ActionType` registry
+ the `Function` (read) / `Action` (write) split that lets
NL questions land typed mutations under operator approval.
Without an `ActionExecutor`, every "schedule this customer
for follow-up", "approve this pending order", "mark this
record as resolved" intent has to either route through
hand-written endpoints or sit in the LLM's free-form
suggestion stream waiting for a human to translate.

## Decision (sketch)

`ActionExecutor` is the runtime trait the agent invokes
when it has resolved an intent to a typed `ActionDef`.
Symmetric in shape to the `GraphRuntime` trait that
`query_graph` uses for reads:

```rust
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    /// Invoke a typed action against the platform. The
    /// implementation:
    ///
    ///   1. Resolves `action_id` against the active
    ///      `OntologyIR::actions()` (per the
    ///      `GRAPH_ONTOLOGY` task-local).
    ///   2. Validates `params` against the action's
    ///      `parameters: Vec<ActionParameter>` schema.
    ///   3. Evaluates the action's `preconditions` against
    ///      the bound subject (`ApprovalPolicy` checks land
    ///      here too).
    ///   4. Executes the action's body — a typed
    ///      `Cypher` / `Federation` / `Function` write
    ///      operation declared on the `ActionDef`.
    ///   5. Emits a `prov:Activity` row recording the
    ///      ActionExecute envelope (per ADR-0008's PROV-O
    ///      contract; the activity kind is
    ///      `ProvenanceActivityKind::ActionExecute { action_id,
    ///      idempotency_key }`).
    ///   6. Honours the `ApprovalPolicy` declared on the
    ///      action — `RequiresApproval` returns
    ///      `ActionResult::PendingApproval { proposal_id }`
    ///      instead of executing, routing the request into
    ///      the `HeuristicProposal` queue (per ADR-0023).
    async fn invoke_action(
        &self,
        action_id: &ActionId,
        params: &ActionInvocationParams,
        principal: &Principal,
    ) -> OxResult<ActionResult>;
}

pub enum ActionResult {
    /// The action ran. `affected` carries typed counters
    /// (rows / nodes / edges) the FE renders on the
    /// completion toast; `provenance_id` is the canonical
    /// link for "show me what happened" drilldown.
    Executed {
        affected: ActionAffected,
        provenance_id: ProvenanceId,
    },
    /// The action's `ApprovalPolicy` requires human
    /// review. The proposal is durable in the queue per
    /// ADR-0023; the operator approval surface picks it
    /// up and a future `invoke_action` call (post-approval)
    /// re-runs through the same path with the approval id
    /// threaded.
    PendingApproval {
        proposal_id: HeuristicProposalId,
    },
    /// Dry-run mode — the planner produced the would-be
    /// effects without executing. Same `affected` shape,
    /// no `provenance_id` (nothing committed).
    DryRun {
        affected: ActionAffected,
    },
}

pub struct ActionInvocationParams {
    /// Subject the action operates on. `ActionDef.subject`
    /// declares the expected `EntityKind`; the executor
    /// rejects on mismatch.
    pub subject: EntityRef,
    /// Caller-supplied parameter values, validated against
    /// `ActionDef.parameters`.
    pub values: HashMap<String, PropertyValue>,
    /// `Some` for replay-safe invocations; `None` for
    /// idempotency-best-effort.
    pub idempotency_key: Option<String>,
    /// `true` reroutes execution into a planner-only path
    /// that returns the effects without committing.
    pub dry_run: bool,
}

pub struct ActionAffected {
    pub nodes_created: u64,
    pub nodes_updated: u64,
    pub edges_created: u64,
    pub edges_updated: u64,
    pub rows_written: u64,        // federation-backed actions
}
```

## Approval policy enforcement

`ActionDef.approval_policy` already declares the gate:

- **`Auto`** — execute immediately. Returns `Executed`.
- **`RequiresApproval { roles }`** — never executes
  directly. Inserts a `HeuristicProposal` row carrying the
  `ActionInvocationParams`, the proposed `prov:Activity`
  envelope, and the requesting principal. Returns
  `PendingApproval { proposal_id }`. The governance
  approval surface (existing `/api/governance/approvals`)
  picks up the proposal; on approve, the approval handler
  re-invokes `ActionExecutor::invoke_action` with the
  same params and an `approved_by` principal threaded
  through, which the executor recognises as
  "post-approval invocation" and lands as `Executed`.
- **`DryRunOnly`** — every invocation lands `DryRun`.
  Used for sandboxed exploration of effects.

The four invariants from ADR-0023 (no auto decisions,
durable queue, threshold gating, audit trail) all apply
to `RequiresApproval` actions automatically through this
routing — the executor doesn't get to override.

## Agent tool

A new `invoke_action` agent tool sits alongside `query_graph`:

```rust
pub struct InvokeActionTool { domain: DomainContext }

impl SchemaTool for InvokeActionTool {
    const NAME: &str = "invoke_action";
    const DESCRIPTION: &str = "Invoke a typed action against the platform.";
    const READ_ONLY: bool = false;
    // ...
}
```

Tool input shape:

```jsonc
{
  "action_id": "a-schedule-followup",
  "subject": { "kind": "node_instance", "node_type_id": "nt-customer", "element_id": "c-12345" },
  "params": { "follow_up_date": "2026-06-01", "owner_user_id": "u-7" },
  "dry_run": false
}
```

Tool result envelope (per the `ox-agent::CLAUDE.md`
"Tool Result Contract" rule):

- `executed: bool` — was the action committed.
- `pending_approval: bool` — true when `RequiresApproval`
  routed to the queue.
- `affected: { nodes, edges, rows }` summary — the LLM
  uses this to decide its next step.
- `provenance_id` — string id; the FE fetches the full
  PROV-O record from `/api/provenance/{id}` for rendering.

The full `ActionAffected` + the typed proposal payload land
on the persisted `QueryExecution` row (or a new
`ActionExecution` row — TBD by the Phase 5 implementation),
not on the tool result envelope.

## NL → Action prompt path

The Brain's `translate_query` currently returns `QueryIR`
only. Phase 6 (PlanRouter) lets it also return a
`RouteDecision`. Phase 5's analogue: a new Brain method
`translate_action(question, ontology)` that returns
`Option<TypedActionInvocation>`. The agent call sequence
becomes:

```
translate_query(...)             // try the read path
  → if Ok(QueryIR) → query_graph
  → if NoSemanticMatch → translate_action(...)
     → if Ok(TypedActionInvocation) → invoke_action
     → if NoMatch → clarification request
```

Schema-RAG (per ADR-0014) injects `ActionDef` summaries
alongside `NodeTypeDef` / `MetricDef` so the LLM sees the
available actions inline with the ontology shape. This
mirrors the Foundry AIP pattern: actions are first-class
in the model's prompt, not a side-channel.

## UI surface (FE)

The matching FE side ships an `ActionType` registry primitive
(per ADR-0022's `PluginRegistry<T>`):

```ts
const actionRegistry = new PluginRegistry<ActionTypeDef>({ compare });
```

Surfaces register actions by mounting `useActionType(actionRegistry, def)`.
The action-invocation form renders from `ActionDef.parameters`
declaratively (one less form-per-domain to hand-build). The
bulk-action-bar primitive (per ADR-0020) gains an
`actionRegistry` slot so multi-select cohorts can invoke
actions in bulk.

The approval gate primitive (`<ApprovalGate proposalId={...}
onApproved={...}>`) wraps any inline action button so
`RequiresApproval` routing surfaces the inline approval flow
without leaving the workbench.

## Test pyramid

- **Unit tests on `ActionExecutor` impl** — every
  `ApprovalPolicy` variant, every parameter-validation
  failure case, idempotency replay.
- **Integration tests in `ox-api/tests/`** — fire
  `invoke_action` through the agent's tool wire,
  assert `prov:Activity` row, assert `HeuristicProposal`
  row for `RequiresApproval` path, assert dry-run path
  doesn't write.
- **Eval golden cases** — extend
  `tests/golden/nl2cypher.golden.json` with NL → Action
  cases (`expected_query_op_kind: "action_invocation"` +
  `expected_action_id: "a-..."`).

## Out of scope (v1)

- **Action composition** — calling action A whose body
  invokes action B. v1 actions are leaf operations; the
  composition primitive lands when a real use case
  surfaces.
- **Bulk action atomicity** — multi-subject actions today
  loop over subjects sequentially; transactional bulk
  actions need source-side support beyond the v1 surface.
- **NL action authoring** — the LLM doesn't author
  `ActionDef` rows in v1; actions are operator-declared
  alongside other ontology objects.

## References

- ADR-0008 — PROV-O lineage (action → activity).
- ADR-0014 — `ConceptDef` (the LLM-facing prompt path
  that includes actions in the schema-RAG payload).
- ADR-0017 — Typed error wire shape (action validation
  errors land as `ActionParameterMismatch` / similar
  typed codes).
- ADR-0020 — `BulkActionBar` (the FE primitive the
  bulk-action surface rides on).
- ADR-0022 — `PluginRegistry<T>` (the FE actionRegistry
  primitive).
- ADR-0023 — `HeuristicProposal` queue (the
  `RequiresApproval` routing target).
- `crates/ox-ontology/src/action.rs` — `ActionDef`,
  `ActionParameter`, `ApprovalPolicy`.
- Phase 5 second half of the long-horizon plan.
