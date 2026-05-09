//! Integration test entry — single test binary statically linking the
//! ox-store dependency graph once across every focus module.
//!
//! Each `mod` below names a focus area; inside the file lives that area's
//! tests. Most tests are `#[ignore]`d and gated on a live
//! PostgreSQL behind `OX_TEST_DATABASE_URL`:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test integration -- --ignored
//! ```
//!
//! The fast-path tests such as advisory-lock constant uniqueness run
//! unconditionally on every PR.

mod integration {
    mod advisory_lock;
    mod community_summaries;
    mod data_sources;
    mod draft_cluster_checkpoint;
    mod eval_retrieval_golden;
    mod idempotency;
    mod jwt_revocation;
    mod load_checkpoint;
    mod pool_rls_session;
    mod rls_enforcement;
    mod rls_invariants;
    mod source_mapping_artifact;
    mod temporal;
    mod workspace_context_guard;
}
