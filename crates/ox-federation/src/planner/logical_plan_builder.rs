//! `LogicalPlanBuilder` — lower `MatchPlanSpec` to a DataFusion
//! `LogicalPlan`.
//!
//! Pipeline: `scans (+ optional joins) → filter → project → sort →
//! limit`. Every stage is composed as a separate helper so the
//! lowering stays easy to read and slice-sized extensions can land
//! without touching the others.
//!
//! ## Currently supported shapes
//!
//! - **Single-scan** — `MATCH (n:Label)` plus WHERE / projection /
//!   ORDER BY / LIMIT / SKIP / workspace scope.
//! - **Multi-mapping scan** — `UNION ALL` across every mapping that
//!   resolves for a single variable (interface expansion or author-
//!   declared overlap).
//! - **Single FK hop** — `MATCH (a:L)-[:E]->(b:R)` with
//!   `LinkMappingKind::ForeignKey`. Lowered as
//!   `LogicalPlanBuilder::join_on` with an equi-join on the
//!   `source_column = target_column` pair declared by the link
//!   mapping. Columns are always variable-qualified so the join's
//!   output schema stays unambiguous.
//!
//! Every other shape is refused with a descriptive `Unsupported`
//! error that names the future slice — callers get a precise
//! diagnostic, never a silent mis-lowering.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use datafusion::datasource::provider_as_source;
use datafusion::logical_expr::{
    Expr as DfExpr, JoinType, LogicalPlan, LogicalPlanBuilder as DfLogicalPlanBuilder, col,
};
use datafusion::prelude::SessionContext;

use ox_core::property_key::PropertyKey;
use ox_core::variable_name::VariableName;
use ox_ontology::mapping::{ColumnRef, EndpointRef, LinkMappingKind, SourceRelationRef};
use ox_query_ir::query::{
    Expr as IrExpr, GraphPattern, OrderClause, Projection, QueryIR, QueryOp, SortDirection,
};

use crate::adapter_resolver::AdapterResolver;
use crate::error::{FederationError, FederationResult};
use crate::planner::expr_lowering::{expr_to_df, property_filter_to_df};
use crate::planner::match_planner::{
    HopMappingEntry, HopSpec, MatchPlanSpec, NodeScanSpec, ScanMappingEntry,
};
use crate::table_provider::SourceTableProvider;

/// Top-level entry point. Lowers a whole `MatchPlanSpec` to a
/// DataFusion `LogicalPlan`.
pub async fn build_match_plan<R: AdapterResolver + ?Sized>(
    spec: &MatchPlanSpec<'_>,
    adapters: &R,
) -> FederationResult<LogicalPlan> {
    build_match_plan_with_projections(spec, adapters, &[]).await
}

/// Same as [`build_match_plan`] but applies a `RETURN` projection on
/// top of the scan. `projections` comes from the originating
/// `QueryOp::Match { projections, .. }`. An empty slice means "no
/// explicit projection" — the caller gets every column the scan
/// produced (`SELECT *`-style).
///
/// Kept as a separate entry point so slice-4a can add projections
/// without breaking the slice-2/3 signature every call-site already
/// uses. `build_match_op` below reads the `projections` field off
/// the op and threads it through.
pub async fn build_match_plan_with_projections<R: AdapterResolver + ?Sized>(
    spec: &MatchPlanSpec<'_>,
    adapters: &R,
    projections: &[Projection],
) -> FederationResult<LogicalPlan> {
    build_match_plan_full(
        spec,
        adapters,
        projections,
        None,
        &[],
        &TailClauses::default(),
        None,
    )
    .await
}

/// Collected query-level tail clauses (post-scan / post-filter /
/// post-projection) — `ORDER BY`, `LIMIT`, `SKIP`. These live on
/// `QueryIR` itself rather than on the inner `QueryOp::Match`, so
/// callers thread them through separately. See [`build_query_ir`].
#[derive(Debug, Default, Clone)]
pub struct TailClauses<'a> {
    pub order_by: &'a [OrderClause],
    pub limit: Option<usize>,
    pub skip: Option<usize>,
}

/// Workspace predicate gate applied to every scan.
///
/// When `Some`, every scan whose backing `ObjectMappingDef.workspace_scope`
/// names a column gets `col(scope_column) = lit(workspace_id)`
/// filter-pushed on top of the scan. Mappings without a
/// `workspace_scope` pass through unchanged — the ontology author
/// has declared that relation shared across workspaces.
///
/// When `None`, no predicate is injected. This is the system-bypass
/// path used by scheduled jobs, admin bootstraps, and the Phase 2
/// CSV integration tests. The planner does **not** treat `None` as
/// an error — that decision is a policy concern that belongs to
/// the call-site (an `ox-api` route that accepts an unscoped query
/// from a non-admin principal is the bug, not this builder).
#[derive(Debug, Clone, Copy)]
pub struct WorkspaceScope<'a> {
    pub workspace_id: &'a str,
}

/// Full-featured entry point. Applies projections, a top-level
/// `WHERE` expression, any inline `GraphPattern::Node.property_filters`
/// collected from the match patterns, and tail clauses
/// (`ORDER BY` / `LIMIT` / `SKIP`).
///
/// The filter shape is combined as
/// `(inline_1 AND inline_2 AND … AND top_level_filter)`.
/// Stage order: `scan(+join) → filter → project → sort → limit`.
async fn build_match_plan_full<R: AdapterResolver + ?Sized>(
    spec: &MatchPlanSpec<'_>,
    adapters: &R,
    projections: &[Projection],
    top_level_filter: Option<&IrExpr>,
    inline_filters: &[(VariableName, PropertyKey, IrExpr)],
    tail: &TailClauses<'_>,
    scope: Option<WorkspaceScope<'_>>,
) -> FederationResult<LogicalPlan> {
    let scans = spec.scans.as_slice();
    let base = if spec.hops.is_empty() {
        match scans {
            [] => {
                return Err(FederationError::unsupported(
                    "LogicalPlanBuilder: MatchPlanSpec has no scans; nothing to lower",
                ));
            }
            [only] => build_single_scan(only, adapters, scope).await?,
            _ => {
                return Err(FederationError::unsupported(
                    "LogicalPlanBuilder: multi-variable MATCH without hops implies a \
                     Cartesian product — add a relationship pattern or split the query",
                ));
            }
        }
    } else {
        if scans.is_empty() {
            return Err(FederationError::unsupported(
                "LogicalPlanBuilder: hops declared without node scans — the planner \
                 does not synthesise endpoint scans from hop endpoints alone",
            ));
        }
        let mut plans: HashMap<VariableName, LogicalPlan> = HashMap::with_capacity(scans.len());
        for scan in scans {
            let plan = build_single_scan(scan, adapters, scope).await?;
            if plans.insert(scan.variable.clone(), plan).is_some() {
                return Err(FederationError::unsupported(format!(
                    "LogicalPlanBuilder: variable '{}' is bound more than once in the \
                     same MATCH — self-joins on the same variable alias are not \
                     supported",
                    scan.variable
                )));
            }
        }
        let (plan, tail_inputs_consumed) = apply_joins(
            plans,
            &spec.hops,
            scans,
            adapters,
            scope,
            top_level_filter,
            inline_filters,
            projections,
        )
        .await?;
        if tail_inputs_consumed {
            // Multi-mapping seed absorbed filters + projections inside
            // each UNION branch so the merged schema already carries
            // the final column names. Skip the outer filter/project
            // stages and go straight to sort/limit, which operate on
            // the projected names.
            let sorted = apply_order_by(plan, tail.order_by)?;
            return apply_limit_skip(sorted, tail.limit, tail.skip);
        }
        plan
    };

    let filtered = apply_filters(base, top_level_filter, inline_filters)?;
    let projected = if projections.is_empty() {
        filtered
    } else {
        apply_projections(filtered, projections)?
    };
    let sorted = apply_order_by(projected, tail.order_by)?;
    apply_limit_skip(sorted, tail.limit, tail.skip)
}

/// Convenience wrapper: take a whole `QueryOp::Match` and route it
/// through the planner → spec → logical-plan pipeline. Exists so
/// downstream callers (Phase 6-C slice 3 E2E tests, eventually
/// `ox-api` query routes) hand over a `QueryOp` directly instead of
/// building the intermediate spec themselves.
///
/// Tail clauses (ORDER BY / LIMIT / SKIP) live on `QueryIR` rather
/// than `QueryOp`, so this entry point passes an empty `TailClauses`.
/// Use [`build_query_ir`] when tail clauses matter.
pub async fn build_match_op<R: AdapterResolver + ?Sized>(
    ontology: &ox_ontology::OntologyIR,
    op: &QueryOp,
    adapters: &R,
) -> FederationResult<LogicalPlan> {
    let spec = crate::planner::match_planner::MatchPlanner::new(ontology).plan(op)?;
    let (projections, filter, patterns) = match op {
        QueryOp::Match {
            projections,
            filter,
            patterns,
            ..
        } => (projections.as_slice(), filter.as_ref(), patterns.as_slice()),
        _ => {
            return Err(FederationError::unsupported(
                "build_match_op: only QueryOp::Match is accepted",
            ));
        }
    };
    let inline_filters = collect_inline_filters(patterns);
    build_match_plan_full(
        &spec,
        adapters,
        projections,
        filter,
        &inline_filters,
        &TailClauses::default(),
        None,
    )
    .await
}

/// Primary entry point — lowers a whole `QueryIR` (including
/// `order_by` / `limit` / `skip`) to a DataFusion `LogicalPlan`.
///
/// `query.as_of` (when set) pins the planner's `MappingResolver` to
/// that instant, so a query with `as_of = 2025-01-01` resolves
/// `ObjectMappingDef`s using their `valid_from`/`valid_to` window —
/// the federation engine sees the mapping world as it was on that
/// date. ADR 0007 calls this the "temporal pivot"; without the wire-
/// through, the planner silently used "now" regardless of the IR
/// field, defeating the whole bitemporal contract.
pub async fn build_query_ir<R: AdapterResolver + ?Sized>(
    ontology: &ox_ontology::OntologyIR,
    query: &QueryIR,
    adapters: &R,
) -> FederationResult<LogicalPlan> {
    let planner = match query.as_of {
        Some(t) => crate::planner::match_planner::MatchPlanner::at(ontology, t),
        None => crate::planner::match_planner::MatchPlanner::new(ontology),
    };
    let spec = planner.plan(&query.operation)?;
    let (projections, filter, patterns) = match &query.operation {
        QueryOp::Match {
            projections,
            filter,
            patterns,
            ..
        } => (projections.as_slice(), filter.as_ref(), patterns.as_slice()),
        _ => {
            return Err(FederationError::unsupported(
                "build_query_ir: only QueryOp::Match is accepted at top level \
                 (Aggregate / Union / Chain lowering lands in later slices)",
            ));
        }
    };
    let inline_filters = collect_inline_filters(patterns);
    let tail = TailClauses {
        order_by: query.order_by.as_slice(),
        limit: query.limit,
        skip: query.skip,
    };
    build_match_plan_full(
        &spec,
        adapters,
        projections,
        filter,
        &inline_filters,
        &tail,
        None,
    )
    .await
}

/// Workspace-scoped variant of [`build_query_ir`]. Every scan whose
/// backing mapping declares a `workspace_scope` column is filtered
/// to `col(scope) = lit(workspace_id)`. Mappings without a
/// declared scope still run — the ontology author is telling the
/// planner that relation is shared across workspaces.
///
/// This is the entry point production call-sites should use. The
/// unscoped `build_query_ir` stays available for bring-up,
/// scheduled jobs, and admin paths that have already proven they
/// should see everything; a wrapper at the `ox-api` layer picks
/// between the two based on the principal's role.
pub async fn build_query_ir_scoped<R: AdapterResolver + ?Sized>(
    ontology: &ox_ontology::OntologyIR,
    query: &QueryIR,
    workspace_id: &str,
    adapters: &R,
) -> FederationResult<LogicalPlan> {
    let planner = match query.as_of {
        Some(t) => crate::planner::match_planner::MatchPlanner::at(ontology, t),
        None => crate::planner::match_planner::MatchPlanner::new(ontology),
    };
    let spec = planner.plan(&query.operation)?;
    let (projections, filter, patterns) = match &query.operation {
        QueryOp::Match {
            projections,
            filter,
            patterns,
            ..
        } => (projections.as_slice(), filter.as_ref(), patterns.as_slice()),
        _ => {
            return Err(FederationError::unsupported(
                "build_query_ir_scoped: only QueryOp::Match is accepted at top level",
            ));
        }
    };
    let inline_filters = collect_inline_filters(patterns);
    let tail = TailClauses {
        order_by: query.order_by.as_slice(),
        limit: query.limit,
        skip: query.skip,
    };
    build_match_plan_full(
        &spec,
        adapters,
        projections,
        filter,
        &inline_filters,
        &tail,
        Some(WorkspaceScope { workspace_id }),
    )
    .await
}

/// Walk every `GraphPattern::Node` in the match and extract inline
/// property filters: `MATCH (n:User {status: "active"})` yields a
/// `(n, status, Literal("active"))` entry. The variable travels
/// with the filter so multi-scan JOIN plans stay disambiguated —
/// `col("n.status")` resolves even when a sibling pattern happens
/// to carry its own `status` column.
fn collect_inline_filters(
    patterns: &[GraphPattern],
) -> Vec<(VariableName, PropertyKey, IrExpr)> {
    let mut out = Vec::new();
    for p in patterns {
        if let GraphPattern::Node {
            variable,
            property_filters,
            ..
        } = p
        {
            for pf in property_filters {
                out.push((variable.clone(), pf.property.clone(), pf.value.clone()));
            }
        }
    }
    out
}

async fn build_single_scan<R: AdapterResolver + ?Sized>(
    scan: &NodeScanSpec<'_>,
    adapters: &R,
    scope: Option<WorkspaceScope<'_>>,
) -> FederationResult<LogicalPlan> {
    let entries = scan.mappings.as_slice();
    match entries {
        [] => Err(FederationError::unsupported(format!(
            "LogicalPlanBuilder: variable '{}' has no applicable mapping — \
             either the label is label-less, the interface has no \
             implementers with mappings, or the ontology binds no \
             physical relation to it",
            scan.variable
        ))),
        [only] => build_table_scan(scan, only, adapters, scope).await,
        _ => build_union_scan(scan, entries, adapters, scope).await,
    }
}

/// Slice 3: UNION ALL every mapping into one plan.
///
/// Every scan uses the same variable as its table alias so the
/// union's output schema inherits that alias; downstream stages
/// can reference columns the same way regardless of how many
/// mappings produced them.
///
/// Schema alignment is DataFusion's responsibility — an
/// incompatible schema across mappings surfaces as a DataFusion
/// error at plan build time, which our FederationError::DataFusion
/// carrier preserves. Mapping-time schema projection (PropertyMapping
/// column renames) lands in Phase 6-C slice 4; until then the
/// onus is on the ontology author to keep the underlying columns
/// aligned.
async fn build_union_scan<R: AdapterResolver + ?Sized>(
    scan: &NodeScanSpec<'_>,
    entries: &[ScanMappingEntry<'_>],
    adapters: &R,
    scope: Option<WorkspaceScope<'_>>,
) -> FederationResult<LogicalPlan> {
    let mut iter = entries.iter();
    let first = iter
        .next()
        .expect("caller guarantees at least two entries");
    let mut combined = build_table_scan(scan, first, adapters, scope).await?;
    for entry in iter {
        let next = build_table_scan(scan, entry, adapters, scope).await?;
        combined = DfLogicalPlanBuilder::from(combined)
            .union(next)
            .map_err(FederationError::from)?
            .build()
            .map_err(FederationError::from)?;
    }
    Ok(combined)
}

/// Slice 5a/5b: turn the pre-built per-variable scan plans into a
/// single joined `LogicalPlan` by folding each hop into a growing
/// join tree. Slice 5c adds the `LinkMappingKind::Bridge` branch.
///
/// ## Algorithm
///
/// 1. Seed the tree with the first hop: `source ⋈ target`.
/// 2. For each subsequent hop, decide how it relates to the tree:
///    - **extend-right** (source already in, target not) → append
///      the target scan via `INNER JOIN` on the hop predicate.
///    - **extend-left** (target already in, source not) → symmetric.
///    - **close cycle** (both already in) → emit the predicate as a
///      top-level filter; the join topology is unchanged.
///    - **disconnected** (neither in) → refuse; disconnected match
///      components (implicit cross-product between sub-patterns)
///      land in a later slice once we have an explicit cross-join
///      decision from the author.
///
/// A `Bridge` hop inserts an extra scan for the bridge relation,
/// aliased as `__brN` (where `N` is the hop's position in the
/// MATCH). The bridge scan is joined into the tree in the same
/// pass; the attachment decision matrix (seed / extend / close) is
/// parallel to the FK case but stitches two equi-predicates per
/// hop.
///
/// The qualified-column scheme from slice 5a is unchanged — every
/// scan is aliased by its variable (or `__brN` for bridges) and
/// every predicate column is qualified with `<alias>.<column>`.
///
/// ## Still unsupported (explicit future-slice messages)
///
/// - Multi-mapping hops (UNION over link mappings).
/// - `LinkMappingKind::Computed` (predicate string in source
///   dialect — needs per-adapter pushdown).
/// - Multi-mapping scan on a hop endpoint (UNION-of-mappings × JOIN).
/// - Composite endpoint keys on bridges (`key_columns.len() > 1`).
#[allow(clippy::too_many_arguments)]
async fn apply_joins<R: AdapterResolver + ?Sized>(
    base_plans: HashMap<VariableName, LogicalPlan>,
    hops: &[HopSpec<'_>],
    scans: &[NodeScanSpec<'_>],
    adapters: &R,
    scope: Option<WorkspaceScope<'_>>,
    top_level_filter: Option<&IrExpr>,
    inline_filters: &[(VariableName, PropertyKey, IrExpr)],
    projections: &[Projection],
) -> FederationResult<(LogicalPlan, bool)> {
    if hops.is_empty() {
        return Err(FederationError::unsupported(
            "LogicalPlanBuilder::apply_joins: called with no hops — caller should \
             route single-scan / multi-scan-cartesian cases separately",
        ));
    }

    let mut assembler = JoinAssembler::new(base_plans);
    // `tail_inputs_consumed` is set by the multi-mapping seed when it
    // applies filter + projection inside each branch before the
    // UNION. The caller uses the flag to skip the outer filter and
    // project stages, since they've already been folded in.
    let mut tail_inputs_consumed = false;

    for (idx, hop) in hops.iter().enumerate() {
        // Sanity-check: every hop variable must be backed by a scan
        // with a single mapping. The scan relation is never read
        // directly for join predicates — disambiguation comes from
        // the link mapping's endpoint relations.
        single_scan_relation(scans, &hop.source_variable)?;
        single_scan_relation(scans, &hop.target_variable)?;

        // Multi-mapping hop: if the planner handed us N>1 link
        // mappings on this hop, emit one INNER JOIN per mapping and
        // UNION ALL the results. Restricted to seed position today
        // (see `JoinAssembler::seed_multi_mapping_hop`).
        //
        // The seed absorbs the query's WHERE filter, inline property
        // filters, and RETURN projection inside each branch — DataFusion's
        // UNION strips variable-level qualifiers, so the projected
        // column names must be finalised before the UNION to keep
        // downstream stages (ORDER BY, LIMIT) wired up correctly.
        if hop.link_mappings.len() > 1 {
            match assembler.endpoint_state(hop) {
                (false, false) => {
                    assembler
                        .seed_multi_mapping_hop(
                            hop,
                            hop.link_mappings.as_slice(),
                            top_level_filter,
                            inline_filters,
                            projections,
                            adapters,
                            scope,
                            idx,
                        )
                        .await?;
                }
                (true, false) | (false, true) => {
                    assembler
                        .extend_multi_mapping_hop(
                            hop,
                            hop.link_mappings.as_slice(),
                            top_level_filter,
                            inline_filters,
                            projections,
                            adapters,
                            scope,
                            idx,
                        )
                        .await?;
                }
                (true, true) => {
                    assembler
                        .close_cycle_multi_mapping_hop(
                            hop,
                            hop.link_mappings.as_slice(),
                            top_level_filter,
                            inline_filters,
                            projections,
                            adapters,
                            scope,
                            idx,
                        )
                        .await?;
                }
            }
            tail_inputs_consumed = true;
            continue;
        }

        let link = select_single_link_mapping(hop)?;

        match &link.mapping.kind {
            LinkMappingKind::ForeignKey {
                source_column,
                target_column,
            } => {
                let predicate = build_equi_join_predicate(
                    hop,
                    &link.mapping.source_endpoint,
                    &link.mapping.target_endpoint,
                    source_column,
                    target_column,
                )?;
                assembler.attach_single_predicate_hop(hop, predicate)?;
            }
            LinkMappingKind::Federated {
                source_match_column,
                target_match_column,
            } => {
                // Functionally identical to ForeignKey today — both
                // lower to `INNER JOIN ... ON equi-predicate`.
                // DataFusion already materialises each side into
                // Arrow before joining, so a cross-source hop works
                // through the generic execute path without extra
                // plumbing. The variant distinction is kept so a
                // future slice can attach cost-model hints (bloom
                // filters, side-preference) without re-deriving
                // "is this cross-source?" from endpoint inspection.
                let predicate = build_equi_join_predicate(
                    hop,
                    &link.mapping.source_endpoint,
                    &link.mapping.target_endpoint,
                    source_match_column,
                    target_match_column,
                )?;
                assembler.attach_single_predicate_hop(hop, predicate)?;
            }
            LinkMappingKind::Bridge {
                bridge_relation,
                source_join,
                target_join,
                bridge_workspace_scope,
            } => {
                let bridge_alias = format!("__br{idx}");
                let bridge_plan = build_bridge_scan(
                    bridge_relation,
                    &bridge_alias,
                    adapters,
                    scope,
                    bridge_workspace_scope.as_ref(),
                )
                .await?;
                let source_predicate = build_bridge_endpoint_predicate(
                    &link.mapping.source_endpoint,
                    &hop.source_variable,
                    &bridge_alias,
                    source_join,
                )?;
                let target_predicate = build_bridge_endpoint_predicate(
                    &link.mapping.target_endpoint,
                    &hop.target_variable,
                    &bridge_alias,
                    target_join,
                )?;
                assembler.attach_bridge_hop(
                    hop,
                    bridge_plan,
                    source_predicate,
                    target_predicate,
                )?;
            }
            LinkMappingKind::Computed { predicate } => {
                assembler.attach_computed_hop(hop, predicate).await?;
            }
        }
    }

    let plan = assembler.finish()?;
    Ok((plan, tail_inputs_consumed))
}

/// Mutable scratch space for the hop-folding loop — holds the
/// growing join tree, the set of variables already in that tree,
/// and the pool of seed scans yet to be absorbed.
///
/// Grouping the three fields eliminates the 6–8-argument helper
/// signatures that would otherwise appear on every attachment
/// function, and lets future slices (OPTIONAL MATCH, multi-mapping
/// hops) add state without touching every call-site.
struct JoinAssembler {
    joined_plan: Option<LogicalPlan>,
    joined_vars: HashSet<VariableName>,
    base_plans: HashMap<VariableName, LogicalPlan>,
}

impl JoinAssembler {
    fn new(base_plans: HashMap<VariableName, LogicalPlan>) -> Self {
        Self {
            joined_plan: None,
            joined_vars: HashSet::new(),
            base_plans,
        }
    }

    fn endpoint_state(&self, hop: &HopSpec<'_>) -> (bool, bool) {
        (
            self.joined_vars.contains(&hop.source_variable),
            self.joined_vars.contains(&hop.target_variable),
        )
    }

    /// Consume the pre-built scan plan for `variable` or surface a
    /// helpful error when the variable has already been absorbed
    /// (or was never seeded).
    fn take_base(&mut self, variable: &VariableName) -> FederationResult<LogicalPlan> {
        self.base_plans.remove(variable).ok_or_else(|| {
            FederationError::unsupported(format!(
                "LogicalPlanBuilder: hop references variable '{variable}' which has \
                 already been consumed by a previous hop, or was never bound as a \
                 node scan"
            ))
        })
    }

    fn mark_joined(&mut self, variable: &VariableName) {
        self.joined_vars.insert(variable.clone());
    }

    /// Attach a hop whose semantics collapse to a single equi-
    /// predicate on the two endpoint scans — [`LinkMappingKind::ForeignKey`].
    fn attach_single_predicate_hop(
        &mut self,
        hop: &HopSpec<'_>,
        predicate: DfExpr,
    ) -> FederationResult<()> {
        match (self.endpoint_state(hop), self.joined_plan.take()) {
            ((false, false), None) => {
                let left = self.take_base(&hop.source_variable)?;
                let right = self.take_base(&hop.target_variable)?;
                let seeded = DfLogicalPlanBuilder::from(left)
                    .join_on(right, JoinType::Inner, [predicate])
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                self.mark_joined(&hop.source_variable);
                self.mark_joined(&hop.target_variable);
                self.joined_plan = Some(seeded);
            }
            ((false, false), Some(_)) => {
                return Err(FederationError::unsupported(format!(
                    "LogicalPlanBuilder: hop {}→{} is disconnected from the already-\
                     joined components — implicit cross-products between sub-patterns \
                     are refused; split the MATCH or add a connecting hop",
                    hop.source_variable, hop.target_variable,
                )));
            }
            ((true, false), Some(jp)) => {
                let right = self.take_base(&hop.target_variable)?;
                let extended = DfLogicalPlanBuilder::from(jp)
                    .join_on(right, JoinType::Inner, [predicate])
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                self.mark_joined(&hop.target_variable);
                self.joined_plan = Some(extended);
            }
            ((false, true), Some(jp)) => {
                let right = self.take_base(&hop.source_variable)?;
                let extended = DfLogicalPlanBuilder::from(jp)
                    .join_on(right, JoinType::Inner, [predicate])
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                self.mark_joined(&hop.source_variable);
                self.joined_plan = Some(extended);
            }
            ((true, true), Some(jp)) => {
                let filtered = DfLogicalPlanBuilder::from(jp)
                    .filter(predicate)
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                self.joined_plan = Some(filtered);
            }
            ((true, _), None) | ((_, true), None) => {
                return Err(FederationError::unsupported(
                    "LogicalPlanBuilder::apply_joins: internal invariant broken \
                     (joined_vars tracked a variable but the plan slot was empty)",
                ));
            }
        }
        Ok(())
    }

    /// Attach a [`LinkMappingKind::Bridge`] hop — the tree grows by
    /// a bridge scan *plus* at most one endpoint scan per iteration.
    /// The four branches mirror [`Self::attach_single_predicate_hop`]
    /// but emit two equi-joins (or one join + one `AND` predicate
    /// when closing a cycle) per hop.
    fn attach_bridge_hop(
        &mut self,
        hop: &HopSpec<'_>,
        bridge_plan: LogicalPlan,
        source_predicate: DfExpr,
        target_predicate: DfExpr,
    ) -> FederationResult<()> {
        match (self.endpoint_state(hop), self.joined_plan.take()) {
            ((false, false), None) => {
                let source_plan = self.take_base(&hop.source_variable)?;
                let target_plan = self.take_base(&hop.target_variable)?;
                let step1 = DfLogicalPlanBuilder::from(source_plan)
                    .join_on(bridge_plan, JoinType::Inner, [source_predicate])
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                let step2 = DfLogicalPlanBuilder::from(step1)
                    .join_on(target_plan, JoinType::Inner, [target_predicate])
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                self.mark_joined(&hop.source_variable);
                self.mark_joined(&hop.target_variable);
                self.joined_plan = Some(step2);
            }
            ((false, false), Some(_)) => {
                return Err(FederationError::unsupported(format!(
                    "LogicalPlanBuilder: bridge hop {}→{} is disconnected from the \
                     already-joined components — implicit cross-products between \
                     sub-patterns are refused; split the MATCH or add a connecting hop",
                    hop.source_variable, hop.target_variable,
                )));
            }
            ((true, false), Some(jp)) => {
                let target_plan = self.take_base(&hop.target_variable)?;
                let step1 = DfLogicalPlanBuilder::from(jp)
                    .join_on(bridge_plan, JoinType::Inner, [source_predicate])
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                let step2 = DfLogicalPlanBuilder::from(step1)
                    .join_on(target_plan, JoinType::Inner, [target_predicate])
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                self.mark_joined(&hop.target_variable);
                self.joined_plan = Some(step2);
            }
            ((false, true), Some(jp)) => {
                let source_plan = self.take_base(&hop.source_variable)?;
                let step1 = DfLogicalPlanBuilder::from(jp)
                    .join_on(bridge_plan, JoinType::Inner, [target_predicate])
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                let step2 = DfLogicalPlanBuilder::from(step1)
                    .join_on(source_plan, JoinType::Inner, [source_predicate])
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                self.mark_joined(&hop.source_variable);
                self.joined_plan = Some(step2);
            }
            ((true, true), Some(jp)) => {
                let combined = source_predicate.and(target_predicate);
                let step = DfLogicalPlanBuilder::from(jp)
                    .join_on(bridge_plan, JoinType::Inner, [combined])
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                self.joined_plan = Some(step);
            }
            ((true, _), None) | ((_, true), None) => {
                return Err(FederationError::unsupported(
                    "LogicalPlanBuilder::apply_joins: internal invariant broken \
                     (joined_vars tracked a variable but the plan slot was empty)",
                ));
            }
        }
        Ok(())
    }

    fn finish(self) -> FederationResult<LogicalPlan> {
        self.joined_plan.ok_or_else(|| {
            FederationError::unsupported(
                "LogicalPlanBuilder::apply_joins: loop finished without producing a plan",
            )
        })
    }

    /// Handle a hop that has multiple applicable link mappings at
    /// **seed position** (neither endpoint already in the tree,
    /// `joined_plan` empty) by emitting one INNER JOIN per mapping
    /// and UNION ALL-ing the results.
    ///
    /// Extend-position multi-mapping is routed to
    /// [`Self::extend_multi_mapping_hop`]; close-cycle (both
    /// endpoints already bound) to [`Self::close_cycle_multi_mapping_hop`].
    ///
    /// ## Filter + projection push-down
    ///
    /// DataFusion's UNION strips variable-level qualifiers from the
    /// merged schema — after the UNION you see `name`, `id`, etc.,
    /// without the `u.` / `o.` prefixes. The qualified-column path
    /// that the rest of the lowering uses (`col("u.name")`) would
    /// then fail to resolve. To keep post-hop stages honest, the
    /// WHERE filter and the RETURN projection are applied **inside
    /// each branch**, *before* the UNION. The branch schema has
    /// qualifiers, so qualified refs resolve correctly; the branch's
    /// projection rewrites them into the final (usually aliased)
    /// names; the UNION then merges schemas that already match.
    ///
    /// Accepted mapping kinds: `ForeignKey`, `Federated`, and
    /// `Bridge`. FK / Federated collapse to a shared equi-join
    /// predicate. Bridge resolves its own scan per branch
    /// (`__br<hop_idx>_<branch_idx>` alias) and stitches the two
    /// endpoint predicates the same way [`Self::attach_bridge_hop`]
    /// does at seed position.
    ///
    /// `Computed` still waits on slice 5d.
    #[allow(clippy::too_many_arguments)]
    async fn seed_multi_mapping_hop<R: AdapterResolver + ?Sized>(
        &mut self,
        hop: &HopSpec<'_>,
        links: &[HopMappingEntry<'_>],
        top_level_filter: Option<&IrExpr>,
        inline_filters: &[(VariableName, PropertyKey, IrExpr)],
        projections: &[Projection],
        adapters: &R,
        scope: Option<WorkspaceScope<'_>>,
        hop_idx: usize,
    ) -> FederationResult<()> {
        // Seed position is the only supported shape today.
        if !matches!(self.endpoint_state(hop), (false, false)) || self.joined_plan.is_some() {
            return Err(FederationError::unsupported(format!(
                "LogicalPlanBuilder: multi-mapping hop {}→{} is only supported at \
                 seed position (neither endpoint already joined, no pre-existing \
                 tree). Extend / close-cycle multi-mapping is a follow-up.",
                hop.source_variable, hop.target_variable
            )));
        }

        let source_plan = self.take_base(&hop.source_variable)?;
        let target_plan = self.take_base(&hop.target_variable)?;

        let mut branches: Vec<LogicalPlan> = Vec::with_capacity(links.len());
        for (branch_idx, entry) in links.iter().enumerate() {
            let joined: LogicalPlan = match &entry.mapping.kind {
                LinkMappingKind::ForeignKey {
                    source_column,
                    target_column,
                } => {
                    let predicate = build_equi_join_predicate(
                        hop,
                        &entry.mapping.source_endpoint,
                        &entry.mapping.target_endpoint,
                        source_column,
                        target_column,
                    )?;
                    DfLogicalPlanBuilder::from(source_plan.clone())
                        .join_on(target_plan.clone(), JoinType::Inner, [predicate])
                        .map_err(FederationError::from)?
                        .build()
                        .map_err(FederationError::from)?
                }
                LinkMappingKind::Federated {
                    source_match_column,
                    target_match_column,
                } => {
                    let predicate = build_equi_join_predicate(
                        hop,
                        &entry.mapping.source_endpoint,
                        &entry.mapping.target_endpoint,
                        source_match_column,
                        target_match_column,
                    )?;
                    DfLogicalPlanBuilder::from(source_plan.clone())
                        .join_on(target_plan.clone(), JoinType::Inner, [predicate])
                        .map_err(FederationError::from)?
                        .build()
                        .map_err(FederationError::from)?
                }
                LinkMappingKind::Bridge {
                    bridge_relation,
                    source_join,
                    target_join,
                    bridge_workspace_scope,
                } => {
                    // Per-branch bridge scan — aliased so two mappings
                    // that happen to point at the same physical
                    // relation don't collide in the plan's schema.
                    let bridge_alias = format!("__br{hop_idx}_{branch_idx}");
                    let bridge_plan = build_bridge_scan(
                        bridge_relation,
                        &bridge_alias,
                        adapters,
                        scope,
                        bridge_workspace_scope.as_ref(),
                    )
                    .await?;
                    let source_pred = build_bridge_endpoint_predicate(
                        &entry.mapping.source_endpoint,
                        &hop.source_variable,
                        &bridge_alias,
                        source_join,
                    )?;
                    let target_pred = build_bridge_endpoint_predicate(
                        &entry.mapping.target_endpoint,
                        &hop.target_variable,
                        &bridge_alias,
                        target_join,
                    )?;
                    let step1 = DfLogicalPlanBuilder::from(source_plan.clone())
                        .join_on(bridge_plan, JoinType::Inner, [source_pred])
                        .map_err(FederationError::from)?
                        .build()
                        .map_err(FederationError::from)?;
                    DfLogicalPlanBuilder::from(step1)
                        .join_on(target_plan.clone(), JoinType::Inner, [target_pred])
                        .map_err(FederationError::from)?
                        .build()
                        .map_err(FederationError::from)?
                }
                LinkMappingKind::Computed { .. } => {
                    return Err(FederationError::unsupported(format!(
                        "LogicalPlanBuilder: multi-mapping hop {}→{} carries a \
                         Computed link mapping — adapter-side predicate pushdown \
                         (slice 5d) is required",
                        hop.source_variable, hop.target_variable
                    )));
                }
            };
            // Apply filters + projections inside this branch so the
            // UNION's output schema matches the final projected shape.
            let filtered = apply_filters(joined, top_level_filter, inline_filters)?;
            let finalised = if projections.is_empty() {
                filtered
            } else {
                apply_projections(filtered, projections)?
            };
            branches.push(finalised);
        }

        // UNION ALL across the branches. At least two branches are
        // guaranteed — the caller routes only when `len > 1`.
        let mut iter = branches.into_iter();
        let first = iter.next().ok_or_else(|| {
            FederationError::unsupported(
                "LogicalPlanBuilder: multi-mapping hop dispatched with an empty \
                 mapping slice — caller must guard on len() > 1",
            )
        })?;
        let mut combined = first;
        for next in iter {
            combined = DfLogicalPlanBuilder::from(combined)
                .union(next)
                .map_err(FederationError::from)?
                .build()
                .map_err(FederationError::from)?;
        }

        self.mark_joined(&hop.source_variable);
        self.mark_joined(&hop.target_variable);
        self.joined_plan = Some(combined);
        Ok(())
    }

    /// Handle a multi-mapping hop at **extend position** — one
    /// endpoint (source or target) is already in the joined tree,
    /// the other is a fresh scan to absorb. Each link mapping
    /// produces a branch that clones the existing tree, attaches
    /// the missing scan via that mapping's predicate, and folds in
    /// the query's filter + projection. UNION ALL over the branches
    /// replaces the tree.
    ///
    /// The same filter + projection push-down from the seed variant
    /// applies (DataFusion's UNION strips qualifiers), so this
    /// method sets `tail_inputs_consumed` via its caller as well.
    ///
    /// Bridge mappings resolve a per-branch bridge scan aliased
    /// `__br<hop_idx>_<branch_idx>`; Computed still refuses.
    #[allow(clippy::too_many_arguments)]
    async fn extend_multi_mapping_hop<R: AdapterResolver + ?Sized>(
        &mut self,
        hop: &HopSpec<'_>,
        links: &[HopMappingEntry<'_>],
        top_level_filter: Option<&IrExpr>,
        inline_filters: &[(VariableName, PropertyKey, IrExpr)],
        projections: &[Projection],
        adapters: &R,
        scope: Option<WorkspaceScope<'_>>,
        hop_idx: usize,
    ) -> FederationResult<()> {
        // Exactly-one-endpoint-in-tree is the invariant; the caller
        // routes (false, false) to the seed method and (true, true)
        // to the refusal branch.
        let (source_in, target_in) = self.endpoint_state(hop);
        let existing_tree = self.joined_plan.take().ok_or_else(|| {
            FederationError::unsupported(
                "LogicalPlanBuilder::extend_multi_mapping_hop: called without a \
                 pre-existing joined_plan — caller must route the seed case \
                 separately",
            )
        })?;

        // Decide which variable is the "fresh endpoint" that needs
        // its scan absorbed into each branch.
        let (fresh_variable, newly_joined_variable) = match (source_in, target_in) {
            (true, false) => (&hop.target_variable, &hop.target_variable),
            (false, true) => (&hop.source_variable, &hop.source_variable),
            _ => {
                return Err(FederationError::unsupported(
                    "LogicalPlanBuilder::extend_multi_mapping_hop: endpoint state \
                     invariant broken — expected exactly one endpoint in the tree",
                ));
            }
        };
        let fresh_plan = self.take_base(fresh_variable)?;

        let mut branches: Vec<LogicalPlan> = Vec::with_capacity(links.len());
        for (branch_idx, entry) in links.iter().enumerate() {
            let branch_plan: LogicalPlan = match &entry.mapping.kind {
                LinkMappingKind::ForeignKey {
                    source_column,
                    target_column,
                } => {
                    let predicate = build_equi_join_predicate(
                        hop,
                        &entry.mapping.source_endpoint,
                        &entry.mapping.target_endpoint,
                        source_column,
                        target_column,
                    )?;
                    DfLogicalPlanBuilder::from(existing_tree.clone())
                        .join_on(fresh_plan.clone(), JoinType::Inner, [predicate])
                        .map_err(FederationError::from)?
                        .build()
                        .map_err(FederationError::from)?
                }
                LinkMappingKind::Federated {
                    source_match_column,
                    target_match_column,
                } => {
                    let predicate = build_equi_join_predicate(
                        hop,
                        &entry.mapping.source_endpoint,
                        &entry.mapping.target_endpoint,
                        source_match_column,
                        target_match_column,
                    )?;
                    DfLogicalPlanBuilder::from(existing_tree.clone())
                        .join_on(fresh_plan.clone(), JoinType::Inner, [predicate])
                        .map_err(FederationError::from)?
                        .build()
                        .map_err(FederationError::from)?
                }
                LinkMappingKind::Bridge {
                    bridge_relation,
                    source_join,
                    target_join,
                    bridge_workspace_scope,
                } => {
                    let bridge_alias = format!("__br{hop_idx}_{branch_idx}");
                    let bridge_plan = build_bridge_scan(
                        bridge_relation,
                        &bridge_alias,
                        adapters,
                        scope,
                        bridge_workspace_scope.as_ref(),
                    )
                    .await?;
                    let source_pred = build_bridge_endpoint_predicate(
                        &entry.mapping.source_endpoint,
                        &hop.source_variable,
                        &bridge_alias,
                        source_join,
                    )?;
                    let target_pred = build_bridge_endpoint_predicate(
                        &entry.mapping.target_endpoint,
                        &hop.target_variable,
                        &bridge_alias,
                        target_join,
                    )?;
                    // When the source is already joined: existing ⋈
                    // bridge (source_pred) ⋈ fresh (target_pred).
                    // When the target is already joined: symmetric.
                    let (first_pred, second_pred) = if source_in {
                        (source_pred, target_pred)
                    } else {
                        (target_pred, source_pred)
                    };
                    let step1 = DfLogicalPlanBuilder::from(existing_tree.clone())
                        .join_on(bridge_plan, JoinType::Inner, [first_pred])
                        .map_err(FederationError::from)?
                        .build()
                        .map_err(FederationError::from)?;
                    DfLogicalPlanBuilder::from(step1)
                        .join_on(fresh_plan.clone(), JoinType::Inner, [second_pred])
                        .map_err(FederationError::from)?
                        .build()
                        .map_err(FederationError::from)?
                }
                LinkMappingKind::Computed { .. } => {
                    return Err(FederationError::unsupported(format!(
                        "LogicalPlanBuilder: multi-mapping hop {}→{} carries a \
                         Computed link mapping — adapter-side predicate pushdown \
                         (slice 5d) is required",
                        hop.source_variable, hop.target_variable
                    )));
                }
            };
            // Same filter + projection push-down as the seed variant:
            // DataFusion's UNION strips variable qualifiers, so the
            // branch must finalise its column names before the merge.
            let filtered = apply_filters(branch_plan, top_level_filter, inline_filters)?;
            let finalised = if projections.is_empty() {
                filtered
            } else {
                apply_projections(filtered, projections)?
            };
            branches.push(finalised);
        }

        let mut iter = branches.into_iter();
        let first = iter.next().ok_or_else(|| {
            FederationError::unsupported(
                "LogicalPlanBuilder::extend_multi_mapping_hop: empty branch slice \
                 — caller must guard on len() > 1",
            )
        })?;
        let mut combined = first;
        for next in iter {
            combined = DfLogicalPlanBuilder::from(combined)
                .union(next)
                .map_err(FederationError::from)?
                .build()
                .map_err(FederationError::from)?;
        }

        self.mark_joined(newly_joined_variable);
        self.joined_plan = Some(combined);
        Ok(())
    }

    /// Handle a multi-mapping hop at **close-cycle position** — both
    /// endpoints are already in the joined tree. Each branch clones
    /// the existing tree and adds one mapping's predicate:
    ///
    /// - **ForeignKey / Federated** → `filter(predicate)`. The
    ///   single-mapping close-cycle case already uses `filter` for
    ///   the same shape; the multi-mapping case just iterates it
    ///   per branch before the UNION.
    /// - **Bridge** → `existing ⋈ bridge ON (source_pred AND target_pred)`
    ///   with a fresh bridge scan (alias `__br<hop_idx>_<branch_idx>`),
    ///   matching the single-mapping close-cycle shape from
    ///   [`Self::attach_bridge_hop`].
    /// - **Computed** refuses with the slice-5d hint.
    ///
    /// Filter + projection push-down is the same as the other two
    /// multi-mapping methods: both stages apply *inside* each branch
    /// before the UNION so the merged schema carries the final
    /// projected column names (DataFusion's UNION strips variable-
    /// level qualifiers).
    #[allow(clippy::too_many_arguments)]
    async fn close_cycle_multi_mapping_hop<R: AdapterResolver + ?Sized>(
        &mut self,
        hop: &HopSpec<'_>,
        links: &[HopMappingEntry<'_>],
        top_level_filter: Option<&IrExpr>,
        inline_filters: &[(VariableName, PropertyKey, IrExpr)],
        projections: &[Projection],
        adapters: &R,
        scope: Option<WorkspaceScope<'_>>,
        hop_idx: usize,
    ) -> FederationResult<()> {
        if !matches!(self.endpoint_state(hop), (true, true)) {
            return Err(FederationError::unsupported(
                "LogicalPlanBuilder::close_cycle_multi_mapping_hop: endpoint \
                 state invariant broken — expected both endpoints already in \
                 the tree",
            ));
        }
        let existing_tree = self.joined_plan.take().ok_or_else(|| {
            FederationError::unsupported(
                "LogicalPlanBuilder::close_cycle_multi_mapping_hop: called \
                 without a pre-existing joined_plan — caller must route the \
                 seed case separately",
            )
        })?;

        let mut branches: Vec<LogicalPlan> = Vec::with_capacity(links.len());
        for (branch_idx, entry) in links.iter().enumerate() {
            let branch_plan: LogicalPlan = match &entry.mapping.kind {
                LinkMappingKind::ForeignKey {
                    source_column,
                    target_column,
                } => {
                    let predicate = build_equi_join_predicate(
                        hop,
                        &entry.mapping.source_endpoint,
                        &entry.mapping.target_endpoint,
                        source_column,
                        target_column,
                    )?;
                    DfLogicalPlanBuilder::from(existing_tree.clone())
                        .filter(predicate)
                        .map_err(FederationError::from)?
                        .build()
                        .map_err(FederationError::from)?
                }
                LinkMappingKind::Federated {
                    source_match_column,
                    target_match_column,
                } => {
                    let predicate = build_equi_join_predicate(
                        hop,
                        &entry.mapping.source_endpoint,
                        &entry.mapping.target_endpoint,
                        source_match_column,
                        target_match_column,
                    )?;
                    DfLogicalPlanBuilder::from(existing_tree.clone())
                        .filter(predicate)
                        .map_err(FederationError::from)?
                        .build()
                        .map_err(FederationError::from)?
                }
                LinkMappingKind::Bridge {
                    bridge_relation,
                    source_join,
                    target_join,
                    bridge_workspace_scope,
                } => {
                    let bridge_alias = format!("__br{hop_idx}_{branch_idx}");
                    let bridge_plan = build_bridge_scan(
                        bridge_relation,
                        &bridge_alias,
                        adapters,
                        scope,
                        bridge_workspace_scope.as_ref(),
                    )
                    .await?;
                    let source_pred = build_bridge_endpoint_predicate(
                        &entry.mapping.source_endpoint,
                        &hop.source_variable,
                        &bridge_alias,
                        source_join,
                    )?;
                    let target_pred = build_bridge_endpoint_predicate(
                        &entry.mapping.target_endpoint,
                        &hop.target_variable,
                        &bridge_alias,
                        target_join,
                    )?;
                    let combined_pred = source_pred.and(target_pred);
                    DfLogicalPlanBuilder::from(existing_tree.clone())
                        .join_on(bridge_plan, JoinType::Inner, [combined_pred])
                        .map_err(FederationError::from)?
                        .build()
                        .map_err(FederationError::from)?
                }
                LinkMappingKind::Computed { .. } => {
                    return Err(FederationError::unsupported(format!(
                        "LogicalPlanBuilder: multi-mapping hop {}→{} carries a \
                         Computed link mapping — adapter-side predicate pushdown \
                         (slice 5d) is required",
                        hop.source_variable, hop.target_variable
                    )));
                }
            };
            let filtered = apply_filters(branch_plan, top_level_filter, inline_filters)?;
            let finalised = if projections.is_empty() {
                filtered
            } else {
                apply_projections(filtered, projections)?
            };
            branches.push(finalised);
        }

        let mut iter = branches.into_iter();
        let first = iter.next().ok_or_else(|| {
            FederationError::unsupported(
                "LogicalPlanBuilder::close_cycle_multi_mapping_hop: empty branch \
                 slice — caller must guard on len() > 1",
            )
        })?;
        let mut combined = first;
        for next in iter {
            combined = DfLogicalPlanBuilder::from(combined)
                .union(next)
                .map_err(FederationError::from)?
                .build()
                .map_err(FederationError::from)?;
        }

        // No new variables joined — both endpoints were already in
        // the tree by definition of close-cycle.
        self.joined_plan = Some(combined);
        Ok(())
    }

    /// Attach a `LinkMappingKind::Computed` hop — the predicate is
    /// a source-dialect SQL string, parsed via DataFusion's SQL
    /// expression parser against the combined scan schema.
    ///
    /// ## Scope (this slice)
    ///
    /// - **Seed position only** — neither endpoint already joined.
    ///   Extend / close-cycle dispatch refuses with a follow-up hint.
    /// - **DataFusion SQL dialect** — parsing uses DataFusion's
    ///   generic SQL surface. Source-specific syntax (PostgreSQL's
    ///   `ILIKE`, Snowflake's `PARSE_JSON`, etc.) surfaces as a parse
    ///   error with the offending fragment echoed back. A future
    ///   slice can delegate parsing to the underlying adapter's
    ///   dialect when a Computed edge carries source-pinned SQL.
    ///
    /// ## Shape
    ///
    /// 1. Build a CROSS JOIN of the two endpoint scans — without the
    ///    predicate in `join_on` because the parser needs the
    ///    combined schema to resolve column refs. DataFusion's
    ///    optimiser lifts the filter-after-cross-join back into a
    ///    proper join at execute time, so there is no performance
    ///    regression vs a native INNER JOIN ON.
    /// 2. Parse the predicate into a `DfExpr` using a throwaway
    ///    `SessionContext` — parsing is stateless, so the session
    ///    context is cheap and purely for the API shape.
    /// 3. Apply the parsed expression as a `filter` on the cross-
    ///    joined plan.
    async fn attach_computed_hop(
        &mut self,
        hop: &HopSpec<'_>,
        predicate: &str,
    ) -> FederationResult<()> {
        match self.endpoint_state(hop) {
            (false, false) => {
                if self.joined_plan.is_some() {
                    return Err(FederationError::unsupported(format!(
                        "LogicalPlanBuilder: Computed hop {}→{} is disconnected \
                         from the already-joined components — implicit cross-\
                         products between sub-patterns are refused",
                        hop.source_variable, hop.target_variable
                    )));
                }
                let source_plan = self.take_base(&hop.source_variable)?;
                let target_plan = self.take_base(&hop.target_variable)?;
                let crossed = DfLogicalPlanBuilder::from(source_plan)
                    .cross_join(target_plan)
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                let expr =
                    parse_computed_predicate(predicate, crossed.schema(), hop)?;
                let filtered = DfLogicalPlanBuilder::from(crossed)
                    .filter(expr)
                    .map_err(FederationError::from)?
                    .build()
                    .map_err(FederationError::from)?;
                self.mark_joined(&hop.source_variable);
                self.mark_joined(&hop.target_variable);
                self.joined_plan = Some(filtered);
                Ok(())
            }
            (true, _) | (_, true) => Err(FederationError::unsupported(format!(
                "LogicalPlanBuilder: Computed hop {}→{} at extend / close-cycle \
                 position is a follow-up slice. Today only seed-position \
                 Computed edges lower — split the MATCH or reorder hops so the \
                 Computed hop comes first.",
                hop.source_variable, hop.target_variable
            ))),
        }
    }
}

/// Parse a `LinkMappingKind::Computed` predicate into a DataFusion
/// `Expr` against the combined CROSS JOIN schema. A throwaway
/// `SessionContext` is fine because parsing is stateless — the
/// context never registers tables or executes anything.
fn parse_computed_predicate(
    predicate: &str,
    schema: &datafusion::common::DFSchema,
    hop: &HopSpec<'_>,
) -> FederationResult<DfExpr> {
    let ctx = SessionContext::new();
    ctx.parse_sql_expr(predicate, schema).map_err(|e| {
        FederationError::unsupported(format!(
            "LogicalPlanBuilder: could not parse Computed link mapping \
             predicate for hop {}→{}: {e}. The federation planner uses \
             DataFusion's SQL dialect for these predicates; source-specific \
             syntax needs per-dialect parsing (a follow-up slice).",
            hop.source_variable, hop.target_variable
        ))
    })
}

/// Pick the single link mapping from a hop, refusing hops with
/// zero or multiple applicable mappings.
fn select_single_link_mapping<'a, 'b>(
    hop: &'a HopSpec<'b>,
) -> FederationResult<&'a HopMappingEntry<'b>> {
    match hop.link_mappings.as_slice() {
        [] => Err(FederationError::unsupported(format!(
            "LogicalPlanBuilder: hop {}→{} on edge label {:?} has no applicable \
             link mapping — the ontology declares the edge type but no \
             `LinkMappingDef` binds it to a physical relation",
            hop.source_variable,
            hop.target_variable,
            hop.edge_label.as_ref().map(|l| l.as_str()),
        ))),
        [only] => Ok(only),
        // Multi-mapping hops are routed to `seed_multi_mapping_hop`
        // by the main loop before this helper runs, so reaching this
        // branch means the routing invariant got violated (likely a
        // future refactor that split the main-loop check without
        // matching this one). Stay strict — returning the first
        // mapping silently would hide the drop.
        multi => Err(FederationError::unsupported(format!(
            "LogicalPlanBuilder: hop {}→{} reached single-mapping dispatch with \
             {} link mappings — internal routing invariant broken",
            hop.source_variable,
            hop.target_variable,
            multi.len()
        ))),
    }
}

/// Build an equi-predicate from a hop whose link mapping collapses
/// to a pair of qualified columns — today that is
/// `LinkMappingKind::ForeignKey` and `LinkMappingKind::Federated`.
/// Qualifies both sides by the hop's variable aliases.
fn build_equi_join_predicate(
    hop: &HopSpec<'_>,
    source_endpoint: &EndpointRef,
    target_endpoint: &EndpointRef,
    source_column: &ColumnRef,
    target_column: &ColumnRef,
) -> FederationResult<DfExpr> {
    let src_rel = source_endpoint.relation.as_str();
    let tgt_rel = target_endpoint.relation.as_str();
    let lhs = qualify_join_column(
        source_column,
        &hop.source_variable,
        &hop.target_variable,
        src_rel,
        tgt_rel,
    )?;
    let rhs = qualify_join_column(
        target_column,
        &hop.source_variable,
        &hop.target_variable,
        src_rel,
        tgt_rel,
    )?;
    Ok(lhs.eq(rhs))
}

/// Build the equi-predicate that stitches one endpoint to the
/// bridge relation. Accepts composite keys: the endpoint's
/// `key_columns` and the bridge-side `bridge_columns` are zipped
/// pairwise and `AND`-combined into a single predicate.
///
/// Refuses on:
/// - either side empty (nothing to join on),
/// - lengths mismatched (the bridge author said `key_columns` has
///   *k* columns but the bridge only has *k-1* pairing columns —
///   the planner has no safe fallback).
fn build_bridge_endpoint_predicate(
    endpoint: &EndpointRef,
    endpoint_variable: &VariableName,
    bridge_alias: &str,
    bridge_columns: &[ColumnRef],
) -> FederationResult<DfExpr> {
    let endpoint_keys = endpoint.key_columns.as_slice();
    if endpoint_keys.is_empty() {
        return Err(FederationError::unsupported(format!(
            "LogicalPlanBuilder: bridge endpoint for variable '{endpoint_variable}' \
             declares no key_columns — nothing to join on"
        )));
    }
    if bridge_columns.is_empty() {
        return Err(FederationError::unsupported(format!(
            "LogicalPlanBuilder: bridge link mapping declares no join columns on \
             the side bound to variable '{endpoint_variable}' — nothing to zip \
             with the endpoint's {n} key_columns",
            n = endpoint_keys.len()
        )));
    }
    if endpoint_keys.len() != bridge_columns.len() {
        return Err(FederationError::unsupported(format!(
            "LogicalPlanBuilder: bridge endpoint '{endpoint_variable}' declares \
             {k_ep} key_columns but the bridge side supplies {k_br} join columns \
             — the planner zips them positionally and needs equal lengths",
            k_ep = endpoint_keys.len(),
            k_br = bridge_columns.len()
        )));
    }

    let mut combined: Option<DfExpr> = None;
    for (key_column, bridge_column) in endpoint_keys.iter().zip(bridge_columns.iter()) {
        let endpoint_side = col(format!("{}.{}", endpoint_variable.as_str(), key_column));
        let bridge_side = col(format!("{}.{}", bridge_alias, bridge_column.column));
        let pair = endpoint_side.eq(bridge_side);
        combined = Some(match combined {
            None => pair,
            Some(acc) => acc.and(pair),
        });
    }
    combined.ok_or_else(|| {
        FederationError::unsupported(
            "LogicalPlanBuilder::build_bridge_endpoint_predicate: internal \
             invariant broken — no predicate produced despite non-empty inputs",
        )
    })
}

/// Build a DataFusion scan for a bridge relation. The scan is
/// aliased with a unique `__brN` identifier so it does not collide
/// with any query-bound variable.
///
/// Workspace scope is injected per-bridge: when the
/// `LinkMappingKind::Bridge` declares a `bridge_workspace_scope`
/// column AND the caller supplies a `WorkspaceScope`, an extra
/// equi-predicate against the workspace id is appended to the
/// bridge scan. A `None` declaration keeps the legacy "shared
/// bridge" behaviour — only safe when the bridge holds no
/// workspace-private joins (the ontology author owns that
/// declaration).
async fn build_bridge_scan<R: AdapterResolver + ?Sized>(
    bridge_relation: &SourceRelationRef,
    bridge_alias: &str,
    adapters: &R,
    scope: Option<WorkspaceScope<'_>>,
    bridge_workspace_scope: Option<&ColumnRef>,
) -> FederationResult<LogicalPlan> {
    let adapter = adapters.resolve(&bridge_relation.source_id)?;
    let provider = Arc::new(
        SourceTableProvider::try_new(adapter, bridge_relation.relation.clone()).await?,
    );
    let source = provider_as_source(provider);
    let mut plan = DfLogicalPlanBuilder::scan(bridge_alias, source, None)
        .map_err(FederationError::from)?
        .build()
        .map_err(FederationError::from)?;

    if let (Some(ws), Some(scope_col)) = (scope, bridge_workspace_scope) {
        // Filter directly on the bridge alias: `__brN.<scope_col> = <ws_id>`.
        // The bridge scan aliases its source as `bridge_alias`, so qualified
        // column refs land on the right side of the join plan.
        let predicate = col(format!("{}.{}", bridge_alias, scope_col.column))
            .eq(datafusion::logical_expr::lit(ws.workspace_id.to_string()));
        plan = DfLogicalPlanBuilder::from(plan)
            .filter(predicate)
            .map_err(FederationError::from)?
            .build()
            .map_err(FederationError::from)?;
    }
    Ok(plan)
}

/// Look up the single backing relation for a variable. Returns
/// `Unsupported` when the scan binds to zero or multiple mappings —
/// multi-mapping hops are slice 5b's problem.
fn single_scan_relation<'a>(
    scans: &'a [NodeScanSpec<'_>],
    variable: &VariableName,
) -> FederationResult<&'a str> {
    let scan = scans
        .iter()
        .find(|s| &s.variable == variable)
        .ok_or_else(|| {
            FederationError::unsupported(format!(
                "LogicalPlanBuilder: hop references variable '{variable}' which is \
                 not bound by any MATCH pattern"
            ))
        })?;
    match scan.mappings.as_slice() {
        [] => Err(FederationError::unsupported(format!(
            "LogicalPlanBuilder: variable '{variable}' has no applicable mapping — \
             no scan relation to join against"
        ))),
        [only] => Ok(only.mapping.relation.as_str()),
        _ => Err(FederationError::unsupported(format!(
            "LogicalPlanBuilder: variable '{variable}' binds to multiple mappings — \
             joining across a UNION-of-mappings scan lands in Phase 6-C slice 5b"
        ))),
    }
}

/// Build a qualified DataFusion column expression for one side of a
/// join equi-predicate. `col_ref.relation` is matched against the
/// link mapping's *endpoint* relations (not the object mapping's
/// physical relation, which for adapters like CSV is a single
/// hardcoded table). The endpoint relation is the author-declared
/// symbolic name that aligns with how the `ForeignKey` variant's
/// `source_column` / `target_column` were written.
///
/// Refusing on a mismatch keeps ambiguous / stale link mappings
/// from silently joining against the wrong variable.
fn qualify_join_column(
    col_ref: &ColumnRef,
    source_variable: &VariableName,
    target_variable: &VariableName,
    source_endpoint_relation: &str,
    target_endpoint_relation: &str,
) -> FederationResult<DfExpr> {
    let variable = if col_ref.relation == source_endpoint_relation {
        source_variable.as_str()
    } else if col_ref.relation == target_endpoint_relation {
        target_variable.as_str()
    } else {
        return Err(FederationError::unsupported(format!(
            "LogicalPlanBuilder: FK column '{}.{}' does not reference either \
             endpoint relation (source={} target={}) for hop {}→{} — the link \
             mapping's ForeignKey columns must match one of its endpoint \
             relations",
            col_ref.relation,
            col_ref.column,
            source_endpoint_relation,
            target_endpoint_relation,
            source_variable,
            target_variable,
        )));
    };
    Ok(col(format!("{}.{}", variable, col_ref.column)))
}

/// Apply ORDER BY to `base`. Empty slice → pass-through. Each
/// `OrderClause` must reference a concrete field today — Variable /
/// Expression / Aggregation projections surface as Unsupported
/// (Variable because "order by the whole row" has no SQL meaning;
/// Aggregation because it needs GROUP BY lowering, Phase 6-C
/// slice 6).
fn apply_order_by(
    base: LogicalPlan,
    clauses: &[OrderClause],
) -> FederationResult<LogicalPlan> {
    if clauses.is_empty() {
        return Ok(base);
    }
    let mut sort_exprs: Vec<datafusion::logical_expr::SortExpr> = Vec::with_capacity(clauses.len());
    for c in clauses {
        let df_expr = match &c.projection {
            Projection::Field { variable, field, .. } => {
                col(format!("{}.{}", variable.as_str(), field.as_str()))
            }
            Projection::Variable { variable, .. } => {
                return Err(FederationError::unsupported(format!(
                    "LogicalPlanBuilder: ORDER BY on bare variable '{variable}' has \
                     no single-column meaning — order by a specific field"
                )));
            }
            Projection::Expression { alias, .. } => {
                return Err(FederationError::unsupported(format!(
                    "LogicalPlanBuilder: ORDER BY on expression alias '{alias}' waits \
                     on slice 6 (expression compilation + projection aliasing)"
                )));
            }
            Projection::Aggregation { function, alias, .. } => {
                return Err(FederationError::unsupported(format!(
                    "LogicalPlanBuilder: ORDER BY on aggregation (function={function:?}, \
                     alias='{alias}') waits on slice 6 (GROUP BY + HAVING)"
                )));
            }
            Projection::AllProperties { variable } => {
                return Err(FederationError::unsupported(format!(
                    "LogicalPlanBuilder: ORDER BY on AllProperties({variable}) has \
                     no scalar lowering"
                )));
            }
        };
        let asc = matches!(c.direction, SortDirection::Asc);
        // DataFusion default: NULLS FIRST for DESC, NULLS LAST for
        // ASC. Keep the default — Cypher does not pin nulls ordering
        // either, and pinning it here would surprise mixed-dialect
        // users. When Ontosyx gets an explicit NULLS clause in
        // QueryIR, thread it through here.
        let nulls_first = !asc;
        sort_exprs.push(datafusion::logical_expr::SortExpr {
            expr: df_expr,
            asc,
            nulls_first,
        });
    }
    DfLogicalPlanBuilder::from(base)
        .sort(sort_exprs)
        .map_err(FederationError::from)?
        .build()
        .map_err(FederationError::from)
}

/// Apply SKIP / LIMIT. `None` for both → pass-through. DataFusion
/// expresses these as `.limit(skip, fetch)` where `skip` is the
/// row offset and `fetch` the row count cap.
fn apply_limit_skip(
    base: LogicalPlan,
    limit: Option<usize>,
    skip: Option<usize>,
) -> FederationResult<LogicalPlan> {
    if limit.is_none() && skip.is_none() {
        return Ok(base);
    }
    DfLogicalPlanBuilder::from(base)
        .limit(skip.unwrap_or(0), limit)
        .map_err(FederationError::from)?
        .build()
        .map_err(FederationError::from)
}

/// Apply WHERE + inline property filters to `base`. Either side
/// may be empty; the result is `(inline_1 AND … AND top_level)`.
/// Returns `base` unchanged when both sides are empty.
fn apply_filters(
    base: LogicalPlan,
    top_level: Option<&IrExpr>,
    inline: &[(VariableName, PropertyKey, IrExpr)],
) -> FederationResult<LogicalPlan> {
    let mut conj: Vec<DfExpr> = Vec::new();
    for (variable, field, value) in inline {
        conj.push(property_filter_to_df(variable, field, value)?);
    }
    if let Some(expr) = top_level {
        conj.push(expr_to_df(expr)?);
    }
    match conj.as_slice() {
        [] => Ok(base),
        _ => {
            let combined = conj.into_iter().reduce(|a, b| a.and(b)).expect("non-empty");
            DfLogicalPlanBuilder::from(base)
                .filter(combined)
                .map_err(FederationError::from)?
                .build()
                .map_err(FederationError::from)
        }
    }
}

/// Lower every `Projection` to a DataFusion `Expr`, then apply them
/// as the final `project(...)` step on top of `base`.
///
/// Slice 4a scope:
/// - `Projection::Field { variable, field, alias }` → `col("field").alias(...)`.
///   The `variable` is the table alias chosen by `build_table_scan`;
///   since DataFusion's `col("x")` resolves against the current
///   plan scope, a plain `col(field)` is correct for a single-scan
///   plan. A future hop-aware version qualifies with `variable.field`.
/// - `Projection::Variable { variable, alias }` → pass-through.
///   When the alias is `Some`, we'd ideally rename every column;
///   slice 4a keeps it simple — the plan projects every column
///   unchanged and the alias is stored in a comment rather than a
///   schema rewrite.
/// - Everything else → `Unsupported` with a descriptive message.
fn apply_projections(
    base: LogicalPlan,
    projections: &[Projection],
) -> FederationResult<LogicalPlan> {
    // `Projection::Variable` alone (no fields) is a "keep every
    // column" request — equivalent to no projection at all. Detect
    // the all-pass-through shape and skip the project node so the
    // resulting plan stays a bare TableScan where possible.
    let all_pass_through = projections.iter().all(|p| matches!(p, Projection::Variable { .. }));
    if all_pass_through {
        return Ok(base);
    }

    let mut exprs: Vec<DfExpr> = Vec::with_capacity(projections.len());
    for p in projections {
        exprs.push(projection_to_df_expr(p)?);
    }
    DfLogicalPlanBuilder::from(base)
        .project(exprs)
        .map_err(FederationError::from)?
        .build()
        .map_err(FederationError::from)
}

fn projection_to_df_expr(p: &Projection) -> FederationResult<DfExpr> {
    match p {
        Projection::Field {
            variable,
            field,
            alias,
        } => {
            // The scan aliases its table by the query variable, so
            // `col("<variable>.<field>")` always resolves to the
            // intended column — in a single-scan plan this is the
            // unambiguous choice DataFusion would pick anyway; in a
            // multi-scan JOIN plan qualification is load-bearing.
            let expr = col(format!("{}.{}", variable.as_str(), field.as_str()));
            Ok(match alias {
                Some(a) => expr.alias(a.clone()),
                None => expr,
            })
        }
        Projection::Variable { variable, alias } => {
            // Variable projection with no alias means "keep every
            // column"; the caller filtered that out before calling
            // us. An alias-bearing Variable is ambiguous over a
            // multi-column scan, so report it explicitly.
            Err(FederationError::unsupported(format!(
                "LogicalPlanBuilder: Projection::Variable with alias on '{variable}' \
                 (alias={alias:?}) does not have a single-column lowering — \
                 project the specific fields instead"
            )))
        }
        Projection::Expression { alias, .. } => Err(FederationError::unsupported(format!(
            "LogicalPlanBuilder: Projection::Expression (alias='{alias}') lowering \
             lands in Phase 6-C slice 4b (expression compilation)"
        ))),
        Projection::Aggregation { function, alias, .. } => {
            Err(FederationError::unsupported(format!(
                "LogicalPlanBuilder: Projection::Aggregation (function={function:?}, \
                 alias='{alias}') lands in Phase 6-C slice 6 with GROUP BY"
            )))
        }
        Projection::AllProperties { variable } => {
            Err(FederationError::unsupported(format!(
                "LogicalPlanBuilder: Projection::AllProperties on '{variable}' is \
                 a Cypher map-projection; SQL emits `SELECT *` only when the \
                 projection list is empty — use an empty Vec<Projection> instead"
            )))
        }
    }
}

async fn build_table_scan<R: AdapterResolver + ?Sized>(
    scan: &NodeScanSpec<'_>,
    entry: &ScanMappingEntry<'_>,
    adapters: &R,
    scope: Option<WorkspaceScope<'_>>,
) -> FederationResult<LogicalPlan> {
    let adapter = adapters.resolve(&entry.mapping.source_id)?;
    let provider = Arc::new(
        SourceTableProvider::try_new(adapter, entry.mapping.relation.clone()).await?,
    );
    let source = provider_as_source(provider);

    // Use the query variable as the DataFusion table alias so
    // downstream planner stages (hops, projections) can reference
    // columns as `<variable>.<column>`. Projection / filter
    // pushdown is the job of slice 3; this call always scans every
    // column.
    let mut plan = DfLogicalPlanBuilder::scan(scan.variable.as_str(), source, None)
        .map_err(FederationError::from)?
        .build()
        .map_err(FederationError::from)?;

    // Inject the workspace predicate when the mapping declares a
    // `workspace_scope` column AND the caller provided a scope. The
    // check is per-mapping: a relation shared across workspaces
    // (scope = None) passes through unchanged, matching the
    // author's declaration.
    if let (Some(ws), Some(scope_col)) = (scope, entry.mapping.workspace_scope.as_ref()) {
        let predicate = col(scope_col.column.as_str())
            .eq(datafusion::logical_expr::lit(ws.workspace_id.to_string()));
        plan = DfLogicalPlanBuilder::from(plan)
            .filter(predicate)
            .map_err(FederationError::from)?
            .build()
            .map_err(FederationError::from)?;
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::variable_name::VariableName;
    use ox_ontology::OntologyIR;
    use ox_ontology::ir::NodeTypeDef;
    use ox_ontology::mapping::ObjectMappingDef;
    use ox_query_ir::query::{GraphPattern, QueryOp};
    use ox_source::DataSourceAdapter;
    use ox_source::sample::CsvAdapter;

    use crate::adapter_resolver::InMemoryAdapterResolver;
    use crate::planner::match_planner::MatchPlanner;

    fn gl(s: &str) -> GraphLabel {
        GraphLabel::new(s).expect("valid")
    }

    fn vn(s: &str) -> VariableName {
        VariableName::new(s).expect("valid")
    }

    /// Minimal ontology: one NodeType mapped to a CSV relation
    /// the tests register under `csv-src`.
    fn ontology_and_resolver() -> (OntologyIR, InMemoryAdapterResolver) {
        let mut ont = OntologyIR::new(
            "ont".into(),
            "sample".into(),
            LocalizedText::default(),
            1,
            vec![NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        ont.add_object_mapping(ObjectMappingDef::new(
            "om-u",
            "nt-user",
            "csv-src",
            "records",
        ))
        .unwrap();

        let mut r = InMemoryAdapterResolver::new();
        let adapter: Arc<dyn DataSourceAdapter> =
            Arc::new(CsvAdapter::new("id,name\n1,Alice\n2,Bob\n").unwrap());
        r.register("csv-src", adapter);
        (ont, r)
    }

    fn match_single(var: &str, label: &str) -> QueryOp {
        QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn(var),
                label: Some(gl(label)),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![],
            optional: false,
            group_by: vec![],
        }
    }

    #[tokio::test]
    async fn single_node_match_lowers_to_a_scan_plan() {
        let (ont, resolver) = ontology_and_resolver();
        let spec = MatchPlanner::new(&ont).plan(&match_single("n", "User")).unwrap();
        let plan = build_match_plan(&spec, &resolver).await.unwrap();

        // The DataFusion plan kind should be `TableScan` for a
        // bare single-relation match.
        match &plan {
            LogicalPlan::TableScan(ts) => {
                assert_eq!(ts.table_name.table(), "n");
                assert!(ts.projection.is_none());
            }
            other => panic!("expected TableScan, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_mapping_surfaces_as_unsupported() {
        let mut ont = OntologyIR::new(
            "ont".into(),
            "sample".into(),
            LocalizedText::default(),
            1,
            vec![NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        // No object mapping — spec comes back with an empty
        // `mappings` vec; the builder must refuse.
        let _ = &mut ont;
        let resolver = InMemoryAdapterResolver::new();
        let spec = MatchPlanner::new(&ont).plan(&match_single("n", "User")).unwrap();
        let err = build_match_plan(&spec, &resolver).await.expect_err("must fail");
        assert!(matches!(err, FederationError::Unsupported(_)));
    }

    #[tokio::test]
    async fn multi_variable_match_without_hops_is_rejected() {
        let (mut ont, mut resolver) = ontology_and_resolver();
        // Add a second node type + mapping + adapter so the
        // resolver side is clean — we only want to exercise the
        // multi-variable refusal.
        ont.add_object_mapping(ObjectMappingDef::new("om-o", "nt-user", "csv-src", "records"))
            .unwrap();
        let _ = &mut resolver;

        let op = QueryOp::Match {
            patterns: vec![
                GraphPattern::Node {
                    variable: vn("a"),
                    label: Some(gl("User")),
                    property_filters: vec![],
                },
                GraphPattern::Node {
                    variable: vn("b"),
                    label: Some(gl("User")),
                    property_filters: vec![],
                },
            ],
            filter: None,
            projections: vec![],
            optional: false,
            group_by: vec![],
        };
        let spec = MatchPlanner::new(&ont).plan(&op).unwrap();
        let err = build_match_plan(&spec, &resolver)
            .await
            .expect_err("must fail");
        assert!(matches!(err, FederationError::Unsupported(_)));
    }

    #[tokio::test]
    async fn multi_mapping_on_single_variable_unions_every_entry() {
        // Slice 3: two object mappings for the same node type pointing
        // at the same CSV relation. The planner emits a `UNION ALL`
        // logical plan — the exact row-doubling is data-level detail
        // the execute path verifies; this test only checks the plan
        // shape (root is a Union node).
        let (mut ont, resolver) = ontology_and_resolver();
        ont.add_object_mapping(ObjectMappingDef::new("om-u2", "nt-user", "csv-src", "records"))
            .unwrap();
        let spec = MatchPlanner::new(&ont).plan(&match_single("n", "User")).unwrap();
        assert_eq!(spec.scans[0].mappings.len(), 2);
        let plan = build_match_plan(&spec, &resolver).await.unwrap();
        match &plan {
            LogicalPlan::Union(_) => {}
            other => panic!("expected Union, got {other:?}"),
        }
    }
}
