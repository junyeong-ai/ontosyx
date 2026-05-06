//! Async evaluation-judge worker.
//!
//! Drains pending case-execute results into RAGAS metrics so the
//! operator UI never blocks on a paid LLM round-trip. The
//! synchronous `POST /api/evaluation/cases/{id}/judge` endpoint
//! still ships for one-off operator-driven judging; this cron is
//! the bulk lane that keeps a streaming dataset's metrics fresh
//! without a human poking each row.
//!
//! Selection criterion (via
//! [`ox_store::EvaluationStore::list_unjudged_cases`]):
//!   - `actual` populated (case-execute landed cleanly),
//!   - `error` clear (failed cases re-run before judging),
//!   - input shape isn't `retrieve_anchors` (those score
//!     deterministically at execute time),
//!   - no existing `evaluation_metrics` row tagged
//!     `metadata.kind = 'judge'` (idempotent — re-tick is a noop).
//!
//! Workspaces fan out from the cross-tenant scan; each case
//! scopes into its own `WORKSPACE_ID` before writing the metric
//! rows so RLS keeps the per-tenant boundary even though the
//! read side runs under SYSTEM_BYPASS.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use ox_brain::Brain;
use ox_store::evaluation::{EvaluationCase, EvaluationContext, scope_evaluation_context};
use ox_store::{EvaluationMetric, Store};

use super::cron::{CronTask, spawn_cron};
use crate::routes::evaluation::ExecuteEvaluationCaseRequest;

/// How often the worker drains the queue. Short enough that a
/// streaming dataset shows metrics within a minute of execute,
/// long enough that an empty queue doesn't burn CPU.
const TICK_INTERVAL: Duration = Duration::from_secs(30);

/// Hard cap on cases drained per tick. A backlog spike (eg. a
/// 10k-row dataset just landed) is processed across multiple
/// ticks instead of all-at-once — keeps the LLM bill bounded
/// per tick, and the worker stays responsive to shutdown.
const PER_TICK_BUDGET: u32 = 50;

struct EvalJudgeWorker {
    store: Arc<dyn Store>,
    brain: Arc<dyn Brain>,
}

#[async_trait]
impl CronTask for EvalJudgeWorker {
    fn name(&self) -> &'static str {
        "eval-judge-worker"
    }

    fn interval(&self) -> Duration {
        TICK_INTERVAL
    }

    fn singleton_key(&self) -> Option<i64> {
        Some(*ox_store::advisory_lock::ADVISORY_LOCK_CRON_EVAL_JUDGE)
    }

    async fn run_once(&self) -> ox_core::error::OxResult<()> {
        run_tick(self.store.as_ref(), self.brain.as_ref()).await
    }
}

pub fn spawn_eval_judge_worker(
    store: Arc<dyn Store>,
    brain: Arc<dyn Brain>,
    pool: ox_store::PgPool,
    cancel: CancellationToken,
) {
    spawn_cron(
        Arc::new(EvalJudgeWorker { store, brain }),
        Some(pool),
        cancel,
    );
}

async fn run_tick(store: &dyn Store, brain: &dyn Brain) -> ox_core::error::OxResult<()> {
    // Drain both rubrics independently. A case missing only one
    // rubric won't re-judge on the other — the existence check
    // is rubric-specific, so the LLM bill stays bounded by the
    // backlog plus retries on actual failures.
    let mut ragas_judged = 0usize;
    let mut ragas_failures = 0usize;
    let ragas_cases = store.list_unjudged_cases("judge", PER_TICK_BUDGET).await?;
    for case in ragas_cases {
        match judge_one_ragas(store, brain, &case).await {
            Ok(_) => ragas_judged += 1,
            Err(e) => {
                ragas_failures += 1;
                warn!(
                    case_id = %case.id,
                    workspace_id = %case.workspace_id,
                    error = %e,
                    "eval-judge worker: per-case RAGAS judge failed",
                );
            }
        }
    }

    let mut safety_judged = 0usize;
    let mut safety_failures = 0usize;
    let safety_cases = store
        .list_unjudged_cases("safety_judge", PER_TICK_BUDGET)
        .await?;
    for case in safety_cases {
        match judge_one_safety(store, brain, &case).await {
            Ok(_) => safety_judged += 1,
            Err(e) => {
                safety_failures += 1;
                warn!(
                    case_id = %case.id,
                    workspace_id = %case.workspace_id,
                    error = %e,
                    "eval-judge worker: per-case safety judge failed",
                );
            }
        }
    }

    if ragas_judged + ragas_failures + safety_judged + safety_failures > 0 {
        info!(
            ragas_judged = ragas_judged,
            ragas_failures = ragas_failures,
            safety_judged = safety_judged,
            safety_failures = safety_failures,
            "eval-judge worker tick complete",
        );
    }
    Ok(())
}

async fn judge_one_ragas(
    store: &dyn Store,
    brain: &dyn Brain,
    case: &EvaluationCase,
) -> ox_core::error::OxResult<()> {
    // Pull the question off the discriminated input envelope.
    // `list_unjudged_cases` already filters out retrieve_anchors,
    // so the only judgeable shapes here are translate_query +
    // explain — both expose `question`. A serde failure means
    // the input shape drifted out from under us; log and skip
    // (the case stays unjudged, surfaces in the dashboard's
    // "schema mismatch" pane).
    let parsed: ExecuteEvaluationCaseRequest = match serde_json::from_value(case.input.clone()) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                case_id = %case.id,
                error = %e,
                "eval-judge worker: case input does not match a known shape; skipping",
            );
            return Ok(());
        }
    };
    let question = parsed.question().to_string();
    let actual = case.actual.as_ref().ok_or_else(|| {
        ox_core::error::OxError::Runtime {
            message: "list_unjudged_cases returned a case without `actual`".into(),
        }
    })?;

    // Bind the evaluation scope so capture-side latency / token
    // metrics for the judge call land alongside the rubric axes.
    // `latency_ms.evaluation_judge` becomes a sibling axis.
    let ctx = EvaluationContext {
        run_id: case.run_id,
        case_key: case.case_key.clone(),
        case_id: case.id,
    };
    let judgement = scope_evaluation_context(ctx, async {
        brain
            .judge_evaluation_case(&question, case.expected.as_ref(), actual)
            .await
    })
    .await?;

    // Per-axis metric rows. Mirrors the synchronous handler's
    // shape exactly so the dashboard / diff surfaces don't need
    // to distinguish "judged sync" vs "judged async". Workspace
    // scope wraps the writes so RLS stays in force on this
    // SYSTEM_BYPASS path.
    let now = Utc::now();
    ox_store::WORKSPACE_ID
        .scope(case.workspace_id, async {
            for (name, score, reasoning) in judgement.axes() {
                let metric = EvaluationMetric {
                    id: Uuid::now_v7(),
                    case_id: case.id,
                    workspace_id: case.workspace_id,
                    name: name.to_string(),
                    score,
                    reasoning: Some(reasoning.to_string()),
                    metadata: serde_json::json!({
                        "kind": "judge",
                        "run_id": case.run_id,
                        "case_key": case.case_key,
                        "source": "async_worker",
                    }),
                    created_at: now,
                };
                store.upsert_evaluation_metric(&metric).await?;
            }
            Ok::<(), ox_core::error::OxError>(())
        })
        .await?;
    Ok(())
}

/// Drains a single case through the safety judge. Mirrors the
/// RAGAS path's shape so the dashboard / diff surfaces don't
/// distinguish sync vs async judging — same `metadata.kind =
/// "safety_judge"` envelope the synchronous endpoint writes.
async fn judge_one_safety(
    store: &dyn Store,
    brain: &dyn Brain,
    case: &EvaluationCase,
) -> ox_core::error::OxResult<()> {
    let parsed: ExecuteEvaluationCaseRequest = match serde_json::from_value(case.input.clone()) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                case_id = %case.id,
                error = %e,
                "eval-judge worker (safety): case input does not match a known shape; skipping",
            );
            return Ok(());
        }
    };
    let question = parsed.question().to_string();
    let actual = case.actual.as_ref().ok_or_else(|| {
        ox_core::error::OxError::Runtime {
            message: "list_unjudged_cases returned a case without `actual`".into(),
        }
    })?;

    let ctx = EvaluationContext {
        run_id: case.run_id,
        case_key: case.case_key.clone(),
        case_id: case.id,
    };
    let judgement = scope_evaluation_context(ctx, async {
        brain.judge_safety_evaluation_case(&question, actual).await
    })
    .await?;

    let now = Utc::now();
    ox_store::WORKSPACE_ID
        .scope(case.workspace_id, async {
            for (name, score, reasoning) in judgement.axes() {
                let metric = EvaluationMetric {
                    id: Uuid::now_v7(),
                    case_id: case.id,
                    workspace_id: case.workspace_id,
                    name: name.to_string(),
                    score,
                    reasoning: Some(reasoning.to_string()),
                    metadata: serde_json::json!({
                        "kind": "safety_judge",
                        "run_id": case.run_id,
                        "case_key": case.case_key,
                        "source": "async_worker",
                    }),
                    created_at: now,
                };
                store.upsert_evaluation_metric(&metric).await?;
            }
            Ok::<(), ox_core::error::OxError>(())
        })
        .await?;
    Ok(())
}
