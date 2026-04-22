//! Evaluation framework for NL-to-QueryIR translation quality.
//!
//! Provides deterministic, offline evaluation of the query translation pipeline.
//! All fixtures are embedded in code — no external DB or LLM required for
//! defining test cases.
//!
//! Usage:
//! ```ignore
//! let cases = eval_fixtures::ecommerce_eval_cases();
//! let runner = EvalRunner::new(brain, compiler);
//! let summary = runner.run_all(&cases).await;
//! println!("{summary}");
//! ```

mod cases;
mod fixtures;
mod runner;

pub use cases::{EvalCase, EvalCategory, ExpectedOp};
pub use fixtures::{ecommerce_eval_cases, ecommerce_ontology};
pub use runner::{
    CategoryResult, EvalResult, EvalRunner, EvalSummary, extract_edge_labels, extract_node_labels,
};
