pub(crate) mod analysis;
pub(crate) mod decisions;
pub(crate) mod edit;
pub(crate) mod extend;
pub(crate) mod helpers;
pub(crate) mod lifecycle;
pub(crate) mod preview;
pub(crate) mod refinement;
pub mod revisions;
pub mod scope;
pub(crate) mod streaming;
pub mod types;

// Re-export handler functions for use in the router
pub(crate) use analysis::{reanalyze_modeled_ontology_draft, reanalyze_ontology_draft};
pub(crate) use decisions::update_decisions;
pub(crate) use edit::edit_ontology_draft;
pub(crate) use extend::extend_ontology_draft;
pub(crate) use lifecycle::{
    compile_load, complete_ontology_draft, create_ontology_draft, delete_ontology_draft, deploy_schema,
    execute_load_from_source, generate_load_plan, get_ontology_draft, list_ontology_drafts,
};
pub(crate) use preview::preview_source;
pub(crate) use refinement::{apply_reconcile, ontology_draft, refine_ontology_draft};
pub(crate) use revisions::{
    diff_canonical, diff_current, diff_revisions, get_revision, list_revisions, migrate_schema,
    rebase_draft, restore_revision,
};
pub(crate) use scope::{defer_scope_tables, include_scope_tables};
pub(crate) use streaming::{design_ontology_draft_stream, refine_ontology_draft_stream};
