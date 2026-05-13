//! `EvaluationSafetyJudgeApi` impl for [`DefaultBrain`].
//!
//! Mirrors the RAGAS `judge.rs` shape — JSON-rendered inputs into
//! the prompt, structured-output round-trip, returns the
//! `EvaluationSafetyJudgement` shape directly. Different prompt
//! template (`evaluation_safety_judge`), different rubric, but the
//! capture / latency / fingerprint pipeline is identical.

use std::collections::HashMap;

use async_trait::async_trait;
use entelix::ExecutionContext;

use ox_core::error::OxResult;

use crate::model_resolver::operation;
use crate::*;

#[async_trait]
impl EvaluationSafetyJudgeApi for DefaultBrain {
    async fn judge_safety_evaluation_case(
        &self,
        question: &str,
        actual: &serde_json::Value,
        ctx: &ExecutionContext,
    ) -> OxResult<(EvaluationSafetyJudgement, CallProvenance)> {
        // Safety judging doesn't reference `expected` — the rubric
        // is "is this answer safe", not "does this answer match a
        // golden". Prompt receives just question + actual. JSON
        // serialisation kept lossy-tolerant (`null` on failure)
        // so a malformed `actual` payload still reaches the LLM
        // for triage rather than aborting the judge.
        let actual_str = serde_json::to_string(actual).unwrap_or_else(|_| "null".to_string());

        let mut vars: HashMap<&str, &str> = HashMap::new();
        vars.insert("question", question);
        vars.insert("actual", actual_str.as_str());

        self.call_structured_traced(
            operation::EVALUATION_SAFETY_JUDGE,
            None,
            operation::EVALUATION_SAFETY_JUDGE,
            &vars,
            "Judging evaluation case (safety axes)",
            ctx,
        )
        .await
    }
}
