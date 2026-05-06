//! Online evaluation sampling — automatic capture of production
//! chat traffic into the evaluation surface.
//!
//! At configurable rate (default 0.0 = disabled, env
//! `OX_EVAL_SAMPLING_RATE` overrides), every successful chat
//! completion drops a sample into a workspace-pinned
//! `live_chat_samples` evaluation run. The async judge worker
//! (see `background::eval_judge`) then drains those into RAGAS
//! metrics in the background, so the metric loop reflects real
//! user traffic, not just operator-curated datasets.
//!
//! The sampler is best-effort:
//!
//! - The probabilistic gate is the first check; on miss we
//!   short-circuit before touching the store.
//! - Every store error is logged and swallowed — the user-facing
//!   chat already finished cleanly, the sampler is observability
//!   tagged onto the side, not load-bearing.
//! - Run discovery uses `find_evaluation_run_by_name`; the run
//!   is lazily created on first sample per workspace.
//!
//! Off the chat hot path: sampling fires in a spawned task so
//! the SSE stream completion is never delayed by a sample
//! upsert.

use std::sync::Arc;

use rand::RngExt;
use tracing::{debug, warn};
use uuid::Uuid;

use ox_store::evaluation::{EvaluationCase, EvaluationRun, EvaluationRunStatus};
use ox_store::Store;

/// Canonical run name for the per-workspace live-sample stream.
/// Operator-driven runs are admin-named; this one is system-
/// reserved and the FE renders it specially (auto-imported,
/// not editable).
pub const LIVE_CHAT_SAMPLES_RUN_NAME: &str = "live_chat_samples";

/// Bounded sampling rate in `[0.0, 1.0]`. `0.0` disables the
/// sampler entirely (no store reads, no RNG draws). Values above
/// `1.0` clamp to `1.0` so an operator typo (`100` for "100%")
/// doesn't crash the sampler.
#[derive(Debug, Clone, Copy)]
pub struct SamplingConfig {
    pub rate: f64,
}

impl SamplingConfig {
    pub fn from_rate(rate: f64) -> Self {
        Self {
            rate: rate.clamp(0.0, 1.0),
        }
    }

    pub fn enabled(&self) -> bool {
        self.rate > 0.0
    }
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self::from_rate(0.0)
    }
}

/// Read the sampling rate from env. `OX_EVAL_SAMPLING_RATE` is the
/// canonical knob; absent / unparseable falls back to disabled.
/// Picking a fresh `SamplingConfig` per request keeps the rate
/// hot-swappable without a server restart (read once, no cache).
pub fn sampling_config_from_env() -> SamplingConfig {
    let rate = std::env::var("OX_EVAL_SAMPLING_RATE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    SamplingConfig::from_rate(rate)
}

/// Probabilistic gate. Pulled out as a free function so unit
/// tests can pin the boundary behaviour
/// (`rate=0.0 → never`, `rate=1.0 → always`) without touching the
/// store or RNG.
fn passes_gate(config: SamplingConfig) -> bool {
    if !config.enabled() {
        return false;
    }
    if config.rate >= 1.0 {
        return true;
    }
    let mut rng = rand::rng();
    rng.random::<f64>() < config.rate
}

/// Sample shape persisted into the evaluation surface. Mirrors
/// the `ExecuteEvaluationCaseRequest::Explain` envelope so the
/// async judge worker can pick it up without a per-shape branch.
#[derive(Debug, Clone)]
pub struct ChatSampleInput {
    pub workspace_id: Uuid,
    pub question: String,
    pub answer: String,
    pub model_id: String,
}

/// Try to record a chat sample. Returns `Ok(true)` when the
/// sample landed (passed the gate + persisted), `Ok(false)` when
/// the gate denied it, and `Err` only when the store-write path
/// fails after the gate passed (caller logs + drops; this is
/// best-effort observability, not part of the request contract).
pub async fn try_record_chat_sample(
    store: Arc<dyn Store>,
    config: SamplingConfig,
    sample: ChatSampleInput,
) -> ox_core::error::OxResult<bool> {
    if !passes_gate(config) {
        return Ok(false);
    }

    // Lookup-or-create the workspace's live-samples run inside the
    // workspace scope so RLS authorises the read + write. The
    // caller passes the workspace explicitly because the spawned
    // sampler task lives outside the request middleware's scope.
    let workspace_id = sample.workspace_id;
    ox_store::WORKSPACE_ID
        .scope(workspace_id, async move {
            let run = ensure_live_samples_run(store.as_ref(), workspace_id).await?;
            land_sample_case(store.as_ref(), &run, sample).await
        })
        .await?;
    Ok(true)
}

/// Lookup-or-create the per-workspace `live_chat_samples` run.
/// First sample for a workspace pays the create round-trip;
/// every subsequent sample pays just the lookup. A future
/// `OnceCell<Uuid>` cache would amortise that, but the volume
/// is bounded by `rate * traffic` and the lookup is single-row
/// indexed.
async fn ensure_live_samples_run(
    store: &dyn Store,
    workspace_id: Uuid,
) -> ox_core::error::OxResult<EvaluationRun> {
    if let Some(existing) = store
        .find_evaluation_run_by_name(LIVE_CHAT_SAMPLES_RUN_NAME)
        .await?
    {
        return Ok(existing);
    }
    // First sample for this workspace — materialise the run.
    // Status stays `Running` so the operator UI surfaces it as
    // an active stream. `metadata.kind = 'online_sample'` lets
    // the FE render a "live sample" pill on the row.
    let run = EvaluationRun {
        id: Uuid::now_v7(),
        workspace_id,
        ontology_version_id: None,
        dataset_id: None,
        name: LIVE_CHAT_SAMPLES_RUN_NAME.to_string(),
        description: "Auto-captured chat samples for the production metric loop. \
            Cases land here at the configured sampling rate; the async judge \
            worker scores them in the background."
            .to_string(),
        status: EvaluationRunStatus::Running,
        started_at: chrono::Utc::now(),
        completed_at: None,
        metadata: serde_json::json!({
            "kind": "online_sample",
        }),
    };
    store.create_evaluation_run(&run).await
}

async fn land_sample_case(
    store: &dyn Store,
    run: &EvaluationRun,
    sample: ChatSampleInput,
) -> ox_core::error::OxResult<()> {
    let case_id = Uuid::now_v7();
    let case_key = format!("sample-{}", case_id.simple());
    // Mirror `ExecuteEvaluationCaseRequest::Explain` exactly so
    // the async judge worker dispatches without a per-source
    // branch. `actual` ships the captured chat answer + model.
    let input = serde_json::json!({
        "kind": "explain",
        "question": sample.question,
    });
    let actual = serde_json::json!({
        "content": sample.answer,
        "model": sample.model_id,
    });
    let case = EvaluationCase {
        id: case_id,
        run_id: run.id,
        workspace_id: run.workspace_id,
        case_key,
        input,
        expected: None,
        actual: Some(actual),
        error: None,
        latency_ms: None,
        metadata: serde_json::json!({
            "source": "online_sampler",
        }),
        created_at: chrono::Utc::now(),
    };
    store.upsert_evaluation_case(&case).await.map(|_| ())
}

/// Convenience wrapper: spawn the sampler off the request hot
/// path. The chat handler calls this on `AgentEvent::Complete`
/// and continues immediately; the sampler runs in the
/// background and logs failures. Workspace scope is captured
/// into the spawned task by the caller's `WsScope`.
pub fn spawn_sample(
    store: Arc<dyn Store>,
    ws_scope: crate::spawn_scoped::WsScope,
    config: SamplingConfig,
    sample: ChatSampleInput,
) {
    if !config.enabled() {
        return;
    }
    crate::spawn_scoped::spawn_with_ws(ws_scope, async move {
        match try_record_chat_sample(store, config, sample).await {
            Ok(true) => debug!("eval sampler: chat sample recorded"),
            Ok(false) => {}
            Err(e) => warn!(error = %e, "eval sampler: failed to record chat sample"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_zero_disables_sampler() {
        let cfg = SamplingConfig::from_rate(0.0);
        assert!(!cfg.enabled());
        assert!(!passes_gate(cfg));
    }

    #[test]
    fn rate_one_always_passes_gate() {
        let cfg = SamplingConfig::from_rate(1.0);
        assert!(cfg.enabled());
        for _ in 0..100 {
            assert!(passes_gate(cfg));
        }
    }

    #[test]
    fn rate_above_one_clamps_to_one() {
        let cfg = SamplingConfig::from_rate(7.5);
        assert!((cfg.rate - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rate_below_zero_clamps_to_zero() {
        let cfg = SamplingConfig::from_rate(-0.5);
        assert_eq!(cfg.rate, 0.0);
        assert!(!cfg.enabled());
    }

    #[test]
    fn rate_half_lands_within_3_sigma() {
        // Stochastic but the spread on n=2000 with p=0.5 is
        // ~22 hits at 3σ — comfortably within the bounds we
        // assert. Not a full distribution test, but enough to
        // catch a bug like "always returns true".
        let cfg = SamplingConfig::from_rate(0.5);
        let mut hits = 0u32;
        for _ in 0..2000 {
            if passes_gate(cfg) {
                hits += 1;
            }
        }
        assert!(hits >= 900 && hits <= 1100, "hits={hits}");
    }

    #[test]
    fn from_env_handles_unset_or_garbage() {
        // Unset → 0.0. Any garbage parse → 0.0. Test in a
        // serial scope by setting + unsetting.
        // SAFETY: tests run single-threaded under the default
        // cargo harness for env mutation.
        unsafe {
            std::env::remove_var("OX_EVAL_SAMPLING_RATE");
        }
        assert_eq!(sampling_config_from_env().rate, 0.0);
        unsafe {
            std::env::set_var("OX_EVAL_SAMPLING_RATE", "not-a-number");
        }
        assert_eq!(sampling_config_from_env().rate, 0.0);
        unsafe {
            std::env::set_var("OX_EVAL_SAMPLING_RATE", "0.05");
        }
        assert!((sampling_config_from_env().rate - 0.05).abs() < 1e-9);
        unsafe {
            std::env::remove_var("OX_EVAL_SAMPLING_RATE");
        }
    }
}
