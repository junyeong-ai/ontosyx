//! Eval golden gate — retrieval regression check.
//!
//! Seeds a small frozen ontology into a fresh test workspace,
//! commits it (which materialises the Level-3 navigation indexes),
//! and then drives the same `OntologyNavigationStore::search_entry_points`
//! / `score_retrieval_metrics` pipeline the production case-execute
//! handler runs. Each golden case asserts a per-axis floor
//! (`precision@k`, `recall@k`, `MRR`, `NDCG@k`) so the gate fails
//! when a refactor regresses the blended trigram + full-text +
//! embedding scoring (or a migration breaks the materialise path).
//!
//! The fixture is intentionally tiny — four NodeTypes, three
//! EdgeTypes, four GlossaryTerms. Big enough to score a meaningful
//! ranking, small enough that the gate runs in seconds and the
//! expected ids are obvious from the schema. Operators extend by
//! adding cases to `golden_cases()` rather than scaling the
//! ontology.
//!
//! Ignored by default — same `OX_TEST_DATABASE_URL` requirement as
//! every other PG-gated integration test:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test integration -- --ignored \
//!     eval_retrieval_golden
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::let_underscore_must_use
)]

use ox_core::graph_label::GraphLabel;
use ox_core::i18n::LocalizedText;
use ox_ontology::glossary::{GlossaryTermDef, GlossaryTermId};
use ox_ontology::ir::{EdgeTypeDef, NodeTypeDef, OntologyIR};
use ox_store::evaluation::score_retrieval_metrics;
use ox_store::navigation::EntryPointSearchOptions;
use ox_store::{OntologyNavigationStore, OntologyVersionStore, PostgresStore};
use uuid::Uuid;

fn resolve_test_db_url() -> Option<String> {
    for key in ["OX_TEST_DATABASE_URL", "OX_DATABASE_URL", "DATABASE_URL"] {
        if let Ok(v) = std::env::var(key)
            && !v.is_empty()
        {
            return Some(v);
        }
    }
    None
}

async fn connect_store() -> Option<PostgresStore> {
    let url = resolve_test_db_url()?;
    let store = PostgresStore::connect(&url, 4)
        .await
        .expect("connect to test DB");
    store.migrate().await.expect("apply migrations");
    Some(store)
}

fn gl(s: &str) -> GraphLabel {
    GraphLabel::new(s).expect("graph label")
}

fn glossary_term(id: &str, term: &str, description: &str) -> GlossaryTermDef {
    GlossaryTermDef {
        id: GlossaryTermId::new(id),
        term: LocalizedText::new(term),
        display_name: LocalizedText::default(),
        description: LocalizedText::new(description),
        examples: Vec::new(),
        aliases: Vec::new(),
        related_terms: Vec::new(),
        category: None,
        governance: ox_ontology::glossary::TermGovernance::default(),
        valid_from: None,
        valid_to: None,
        lifecycle: ox_ontology::glossary::TermLifecycle::default(),
        concept_id: None,
    }
}

fn node_type(id: &str, label: &str, description: &str) -> NodeTypeDef {
    NodeTypeDef {
        id: id.into(),
        label: gl(label),
        description: LocalizedText::new(description),
        ..Default::default()
    }
}

fn edge_type(id: &str, label: &str, description: &str) -> EdgeTypeDef {
    EdgeTypeDef {
        id: id.into(),
        label: gl(label),
        description: LocalizedText::new(description),
        ..Default::default()
    }
}

/// Frozen golden fixture. Keep small — the gate runs on every PR
/// behind a live PG, so wallclock matters.
fn build_golden_ontology(lineage_id: &str) -> OntologyIR {
    let mut ir = OntologyIR::new(
        lineage_id.to_string(),
        "Eval Golden Fixture".to_string(),
        LocalizedText::default(),
        1u32,
        vec![
            node_type(
                "nt_customer",
                "Customer",
                "Person or organisation that buys our products and services",
            ),
            node_type(
                "nt_order",
                "Order",
                "Sales order placed by a customer for one or more products",
            ),
            node_type(
                "nt_product",
                "Product",
                "Physical or digital good listed in the catalogue",
            ),
            node_type(
                "nt_employee",
                "Employee",
                "Internal staff member of the organisation",
            ),
        ],
        vec![
            edge_type(
                "et_placed",
                "PLACED",
                "Customer placed an order — origin of every sales transaction",
            ),
            edge_type(
                "et_contains",
                "CONTAINS",
                "Order contains a product line — fan-out to product catalogue",
            ),
            edge_type(
                "et_assigned",
                "ASSIGNED",
                "Employee assigned to handle the order workflow",
            ),
        ],
        vec![],
    );
    ir.add_glossary_term(glossary_term(
        "gt_vip",
        "VIP",
        "Top-tier customer with elevated service level agreements",
    ))
    .expect("add gt_vip");
    ir.add_glossary_term(glossary_term(
        "gt_premium",
        "Premium",
        "Higher-priced product tier with extended warranty coverage",
    ))
    .expect("add gt_premium");
    ir.add_glossary_term(glossary_term(
        "gt_return",
        "Return",
        "Customer-initiated reversal of a completed order",
    ))
    .expect("add gt_return");
    ir.add_glossary_term(glossary_term(
        "gt_cohort",
        "Cohort",
        "Group of customers sharing a defining attribute for analysis",
    ))
    .expect("add gt_cohort");
    ir
}

/// Per-case threshold tuple. The gate fails when any axis dips
/// below the floor — any one floor breached = regression.
struct GoldenCase {
    name: &'static str,
    question: &'static str,
    expected_ids: &'static [&'static str],
    top_k: u32,
    /// Minimum precision@k. Allow misses on top-K so the floor
    /// reflects "at least one expected id ranked in the working
    /// set" without forcing the blend to clear every adjacent
    /// noise.
    min_precision: f64,
    min_recall: f64,
    min_mrr: f64,
    min_ndcg: f64,
}

fn golden_cases() -> &'static [GoldenCase] {
    // Logical ids match what `OntologyNavigationStore::search_entry_points`
    // surfaces — `entity_kind:logical_id`, where `logical_id` is the
    // canonical id (NodeTypeDef.id / EdgeTypeDef.id /
    // GlossaryTermId), not the human-facing label.
    //
    // Floors calibrated against the label-boosted blend (migration
    // 0012 adds a `label` column; the SQL adds
    // `similarity(label, query)` to the score). Single-word
    // queries that match a label exactly should now rank the
    // structural row first — `mrr = 1.0` is the gate's expected
    // floor for the unambiguous cases.
    &[
        GoldenCase {
            name: "node_type_label_match",
            question: "customer",
            expected_ids: &["node_type:nt_customer"],
            top_k: 5,
            min_precision: 0.15,
            min_recall: 1.0,
            // Label-boost guarantees the literal `Customer` node
            // outranks any glossary term that mentions "customer"
            // in its description. MRR == 1.0 = expected_id at
            // rank 1.
            min_mrr: 1.0,
            min_ndcg: 0.95,
        },
        GoldenCase {
            name: "node_type_partial_match",
            question: "order placed",
            expected_ids: &["node_type:nt_order", "edge_type:et_placed"],
            top_k: 5,
            min_precision: 0.30,
            min_recall: 1.0,
            min_mrr: 0.50,
            min_ndcg: 0.60,
        },
        GoldenCase {
            // Short multi-word queries stress the trigram weight.
            // Label-boost helps both expected anchors but the
            // ranking between them depends on alphabetic /
            // similarity tie-breaks — keep the recall floor
            // strict (both anchors must land in top-K) and don't
            // over-constrain MRR.
            name: "glossary_term_match",
            question: "vip premium",
            expected_ids: &["glossary_term:gt_vip", "glossary_term:gt_premium"],
            top_k: 5,
            min_precision: 0.30,
            min_recall: 0.90,
            min_mrr: 0.50,
            min_ndcg: 0.60,
        },
        GoldenCase {
            name: "edge_type_relationship",
            question: "contains",
            expected_ids: &["edge_type:et_contains"],
            top_k: 5,
            min_precision: 0.20,
            min_recall: 1.0,
            min_mrr: 1.0,
            min_ndcg: 0.95,
        },
    ]
}

/// Provision a fresh user + workspace via system bypass — the test
/// owns the row, so the suffixed slugs avoid colliding with other
/// integration tests sharing the DB.
async fn seed_workspace(store: &PostgresStore) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let short = suffix[..8].to_string();
    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let user_email = format!("eval-golden-{short}@example.com");
        let provider_sub = format!("eval-golden-sub-{short}");
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, name, provider, provider_sub, role) \
             VALUES ($1, 'Eval Golden User', 'test', $2, 'designer') \
             RETURNING id",
        )
        .bind(&user_email)
        .bind(&provider_sub)
        .fetch_one(pool)
        .await
        .expect("insert user");

        let ws_slug = format!("eval-golden-ws-{short}");
        let workspace_id: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces (name, slug, owner_id) \
             VALUES ('Eval Golden Workspace', $1, $2) \
             RETURNING id",
        )
        .bind(&ws_slug)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("insert workspace");
        workspace_id
    })
    .await
}

/// Commits the golden ontology under the active workspace, returns
/// the materialised version's id (the navigation store's keying
/// column).
async fn commit_golden(store: &PostgresStore, workspace_id: Uuid) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let short = suffix[..8].to_string();
    let lineage_id = format!("eval-golden-lineage-{short}");
    let ir = build_golden_ontology(&lineage_id);
    PostgresStore::with_workspace(workspace_id, || async {
        let identity = store
            .create_ontology(
                &format!("eval-golden-{short}"),
                &serde_json::json!({"default": "", "translations": {}}),
                &serde_json::json!({"default": "Eval Golden", "translations": {}}),
                Some(&lineage_id),
            )
            .await
            .expect("create ontology identity");
        let snap = store
            .commit_version(
                identity.id,
                &ir,
                "1",
                None,
                "eval-golden-test",
                "seed golden",
            )
            .await
            .expect("commit golden v1");
        snap.id
    })
    .await
}

/// Drive every golden case through the navigation store, asserting
/// the per-axis floors. Failures collect into one diagnostic
/// message so the operator sees every regression in one go.
async fn run_golden_cases(store: &PostgresStore, version_id: Uuid, workspace_id: Uuid) {
    let mut failures: Vec<String> = Vec::new();
    for case in golden_cases() {
        let opts =
            EntryPointSearchOptions::new(version_id, case.question, case.top_k);
        let hits = PostgresStore::with_workspace(workspace_id, || async {
            store
                .search_entry_points(opts.clone())
                .await
                .expect("search_entry_points")
        })
        .await;
        let actual_ids: Vec<String> = hits
            .iter()
            .map(|h| format!("{}:{}", h.entity_kind, h.logical_id))
            .collect();
        let expected: Vec<String> =
            case.expected_ids.iter().map(|s| (*s).to_string()).collect();
        let m = score_retrieval_metrics(&actual_ids, &expected, case.top_k as usize);
        let mut case_fail: Vec<String> = Vec::new();
        if m.precision_at_k < case.min_precision {
            case_fail.push(format!(
                "precision@k {:.3} < floor {:.3}",
                m.precision_at_k, case.min_precision
            ));
        }
        if m.recall_at_k < case.min_recall {
            case_fail.push(format!(
                "recall@k {:.3} < floor {:.3}",
                m.recall_at_k, case.min_recall
            ));
        }
        if m.mrr < case.min_mrr {
            case_fail.push(format!("mrr {:.3} < floor {:.3}", m.mrr, case.min_mrr));
        }
        if m.ndcg_at_k < case.min_ndcg {
            case_fail.push(format!(
                "ndcg@k {:.3} < floor {:.3}",
                m.ndcg_at_k, case.min_ndcg
            ));
        }
        if !case_fail.is_empty() {
            failures.push(format!(
                "[{}] q={:?} actual={:?} :: {}",
                case.name,
                case.question,
                actual_ids,
                case_fail.join(" | ")
            ));
        }
    }
    if !failures.is_empty() {
        panic!(
            "eval_retrieval_golden — {} case(s) regressed below threshold:\n  {}",
            failures.len(),
            failures.join("\n  "),
        );
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn golden_retrieval_meets_quality_floor() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = seed_workspace(&store).await;
    let version_id = commit_golden(&store, workspace_id).await;
    run_golden_cases(&store, version_id, workspace_id).await;
}
