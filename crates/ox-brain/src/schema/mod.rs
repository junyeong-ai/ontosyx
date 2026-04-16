//! Schema complexity analysis for structured output quality thresholds.
//!
//! branchforge 0.9 handles all provider-specific schema transformations internally
//! (inlining $ref, enforcing additionalProperties:false, stripping unsupported
//! keywords, tagged union flattening). This module only provides complexity
//! counting used to decide whether a schema is suitable for structured output
//! mode, or should fall back to plain JSON mode for quality reasons.

mod diagnostics;

// --- Public re-exports ---

pub use diagnostics::{count_optional_params, count_total_properties};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_plan_schema_within_optional_params_limit() {
        let schema = schemars::schema_for!(ox_core::load_plan::LoadPlan);
        let value = schema.to_value();
        let count = count_optional_params(&value);
        let total = count_total_properties(&value);
        eprintln!("LoadPlan optional params: {count}, total properties: {total}");
        assert!(
            count <= 24,
            "LoadPlan has {count} optional params (limit 24)"
        );
    }

    #[test]
    fn match_query_ir_within_structured_output_limits() {
        let schema = schemars::schema_for!(ox_core::match_query_ir::MatchQueryIR);
        let value = schema.to_value();
        let optional = count_optional_params(&value);
        let total = count_total_properties(&value);
        eprintln!("MatchQueryIR optional params: {optional}, total properties: {total}");
        assert!(
            optional <= 24,
            "MatchQueryIR has {optional} optional params (limit 24)"
        );
        assert!(
            total <= 50,
            "MatchQueryIR has {total} total properties (limit 50)"
        );
    }
}
