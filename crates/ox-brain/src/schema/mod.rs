//! Schema complexity analysis for structured-output quality budgets.
//!
//! entelix codecs handle every provider-specific schema transformation
//! (inlining `$ref`, enforcing `additionalProperties: false`, stripping
//! unsupported keywords, tagged-union flattening) inside `Codec::encode`.
//! This module supplies the *budget* counters the workspace's
//! prompt-budget tests assert against — keeps every LLM-output type
//! sized so the validation-retry loop in
//! `entelix::ChatModel::complete_typed::<T>` converges on the first
//! call rather than burning retry budget on schema-overfit tail
//! cases.

mod diagnostics;

// --- Public re-exports ---

pub use diagnostics::{count_optional_params, count_total_properties};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_plan_schema_within_optional_params_limit() {
        let schema = schemars::schema_for!(ox_ontology::load_plan::LoadPlan);
        let value = schema.to_value();
        let count = count_optional_params(&value);
        assert!(
            count <= 24,
            "LoadPlan has {count} optional params (limit 24)"
        );
    }

    #[test]
    fn structured_match_query_within_structured_output_limits() {
        let schema = schemars::schema_for!(ox_query_ir::structured_match::StructuredMatchQuery);
        let value = schema.to_value();
        let optional = count_optional_params(&value);
        let total = count_total_properties(&value);
        assert!(
            optional <= 24,
            "StructuredMatchQuery has {optional} optional params (limit 24)"
        );
        assert!(
            total <= 50,
            "StructuredMatchQuery has {total} total properties (limit 50)"
        );
    }
}
