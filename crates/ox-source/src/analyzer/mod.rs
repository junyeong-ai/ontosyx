mod ambiguous;
mod exclusions;
mod fk_inference;
mod pii;
mod report;
#[cfg(test)]
mod test_utils;

pub use report::{
    apply_pii_masking, build_analysis_report, build_design_context, enrich_with_repo,
};
