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
//! After Phase 3-B (2026-04-20) this crate holds only what every other
//! crate in the workspace needs: `OxError`, validated identifier
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

pub mod error;
pub mod graph_label;
pub mod i18n;
pub mod id;
pub mod prompt_version;
pub mod property_key;
pub mod source_schema;
pub mod types;
pub mod variable_name;

// ---------------------------------------------------------------------------
// Re-exports — Infrastructure (no domain types re-exported from here any
// more; everything ontology/query-shaped lives in the sibling crates).
// ---------------------------------------------------------------------------

pub use error::{ErrorContext, OxError};
pub use graph_label::GraphLabel;
pub use i18n::{LanguageTag, LocaleError, LocalizedText};
pub use prompt_version::PromptVersion;
pub use property_key::PropertyKey;
pub use source_schema::{SourceProfile, SourceSchema};
pub use types::{escape_cypher_identifier, is_valid_graph_identifier, sanitize_variable};
pub use variable_name::VariableName;

pub use error::OxResult;
