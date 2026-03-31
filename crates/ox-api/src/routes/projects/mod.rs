pub(crate) mod analysis;
pub(crate) mod decisions;
pub(crate) mod edit;
pub(crate) mod extend;
pub(crate) mod helpers;
pub(crate) mod lifecycle;
pub(crate) mod refinement;
pub mod revisions;
pub(crate) mod streaming;
pub mod types;

// Re-export handler functions for use in the router
pub(crate) use analysis::reanalyze_project;
pub(crate) use decisions::update_decisions;
pub(crate) use extend::extend_project;
pub(crate) use lifecycle::{
    compile_load, complete_project, create_project, delete_project, deploy_schema,
    execute_load_from_source, generate_load_plan, get_project, list_projects,
};
pub(crate) use edit::edit_project;
pub(crate) use refinement::{apply_reconcile, design_project, refine_project};
pub(crate) use revisions::{diff_current, diff_revisions, get_revision, list_revisions, migrate_schema, restore_revision};
pub(crate) use streaming::{design_project_stream, refine_project_stream};
