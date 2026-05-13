//! `EvaluationJudge` impl for [`DefaultBrain`].

use std::collections::HashMap;

use async_trait::async_trait;
use entelix::ExecutionContext;

use ox_core::error::OxResult;

use crate::model_resolver::operation;
use crate::*;

#[async_trait]
impl EvaluationJudge for DefaultBrain {
    async fn judge_evaluation_case(
        &self,
        question: &str,
        expected: Option<&serde_json::Value>,
        actual: &serde_json::Value,
        ctx: &ExecutionContext,
    ) -> OxResult<(EvaluationJudgement, CallProvenance)> {
        // The judge prompt receives JSON-rendered values directly.
        // `expected` ships as the literal `null` token when absent
        // so the prompt can branch (`expected != null` → match
        // shape; otherwise judge from the question alone).
        let expected_str = match expected {
            Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };
        let actual_str = serde_json::to_string(actual).unwrap_or_else(|_| "null".to_string());

        let mut vars: HashMap<&str, &str> = HashMap::new();
        vars.insert("question", question);
        vars.insert("expected", expected_str.as_str());
        vars.insert("actual", actual_str.as_str());

        // `call_structured_traced` returns the typed judgement +
        // the per-call `CallProvenance` (prompt id + version +
        // render hash + model id). The caller stamps a
        // `ProvenanceCapture` from the latter before persisting
        // the judge's metric rows; every judged metric ends up
        // pointing at the audit row that produced it.
        self.call_structured_traced(
            operation::EVALUATION_JUDGE,
            None,
            operation::EVALUATION_JUDGE,
            &vars,
            "Judging evaluation case",
            ctx,
        )
        .await
    }
}
