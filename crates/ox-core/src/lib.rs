#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

//! `ox-core` — primitives and shared infrastructure for the Ontosyx
//! platform.
//!
//! This crate holds only what every other crate in the workspace
//! needs: `OxError`, validated identifier
//! newtypes (`GraphLabel`, `PropertyKey`, `VariableName`), the
//! `define_id_newtype!` macro, localisation, the prompt-version
//! wrapper, and the low-level schema snapshot types (`SourceSchema`,
//! `SourceProfile`) the introspection layer produces.
//!
//! Domain types — ontology model, query IR, canvas patterns, LLM
//! wire formats, analysis reports — live in `ox-ontology` and
//! `ox-query-ir`. The layering is enforced by
//! `deny.toml::bans.deny`.

// ---------------------------------------------------------------------------
// Module declarations
// ---------------------------------------------------------------------------

pub mod diagnostic;
pub mod pgvector;
pub mod error;
pub mod graph_label;
pub mod i18n;
pub mod id;
pub mod prompt_version;
pub mod property_key;
pub mod source_schema;
pub mod source_scope;
pub mod types;
pub mod variable_name;

// ---------------------------------------------------------------------------
// Re-exports — Infrastructure (no domain types re-exported from here any
// more; everything ontology/query-shaped lives in the sibling crates).
// ---------------------------------------------------------------------------

pub use diagnostic::{
    DiagnosticBuilder, DiagnosticMessage, diag, is_valid_diagnostic_code, join_messages,
};
pub use error::{ErrorContext, OxError};
pub use graph_label::GraphLabel;
pub use i18n::{
    ADMIN_LOCALE_FALLBACK_DEFAULT, LLM_LOCALE_FALLBACK_DEFAULT, LanguageTag, LocaleError,
    LocalizedText, PRIMARY_LOCALE_DEFAULT, admin_locale_fallback_default_tags,
    display_name_with_fallback, llm_locale_fallback_default_tags,
};
pub use prompt_version::PromptVersion;
pub use property_key::PropertyKey;
pub use source_schema::{SchemaFingerprint, SourceProfile, SourceSchema, TableSummary};
pub use source_scope::{AnalysisScope, AnalyzeSelection, DeferredTable, TableSelection};
pub use types::{escape_cypher_identifier, is_valid_graph_identifier, sanitize_variable};
pub use variable_name::VariableName;

pub use error::OxResult;
