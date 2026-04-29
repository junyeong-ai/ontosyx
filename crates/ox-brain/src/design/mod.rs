//! Ontology-design subsurface — the LLM-facing input shape and the
//! prompt-section formatters that turn it into the rendered template
//! variables the design_ontology TOML consumes.
//!
//! The single entry the rest of the platform sees is
//! [`DesignOntologyInput`]. Callers populate the four optional
//! domain-context slices (`glossary_terms`, `code_systems`,
//! `ambiguity_hints`, `existing_ontology`) when the workspace already
//! holds those artefacts; the LLM then prefers the existing terms
//! over inventing parallel ones for the same concept.
//!
//! The `format::*` helpers render each slice into a single section
//! string. Empty slices produce empty strings so the prompt template
//! collapses naturally without conditional template syntax.

pub mod attribution;
pub mod format;
pub mod input;
pub mod llm_output;
pub mod prompt_economy;

pub use attribution::DesignOntologyOutput;
pub use format::{
    render_ambiguity_section, render_code_systems_section, render_existing_ontology_section,
    render_glossary_section,
};
pub use input::DesignOntologyInput;
pub use llm_output::{
    LLM_OUTPUT_PROPERTY_BUDGET, LlmDesignOutput, LlmEdgeType, LlmNodeType, LlmProperty,
    into_input_ontology, merge_llm_outputs,
};
pub use prompt_economy::{
    DEFAULT_BATCH_PROMPT_BUDGET_CHARS, DEFAULT_DESIGN_PROMPT_BUDGET_CHARS,
    DEFAULT_REFINE_PROMPT_BUDGET_CHARS, PromptBudget, PromptBudgetError, PropertySignal,
    assert_within_budget, render_property_signals,
};
