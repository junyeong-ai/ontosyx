//! `EvaluationJudge` impl for [`DefaultBrain`].

use std::collections::HashMap;

use async_trait::async_trait;

use ox_core::error::OxResult;

use crate::*;

#[async_trait]
impl EvaluationJudge for DefaultBrain {
    async fn judge_evaluation_case(
        &self,
        question: &str,
        expected: Option<&serde_json::Value>,
        actual: &serde_json::Value,
    ) -> OxResult<EvaluationJudgement> {
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

        self.call_structured(
            "evaluation_judge",
            None,
            "evaluation_judge",
            &vars,
            "Judging evaluation case",
        )
        .await
    }
}
