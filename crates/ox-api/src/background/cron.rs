//! `CronTask` — shared shape for every periodic background job.
//!
//! Before this module, each cron (`stale_concepts`, `quality_baseline`,
//! `clarification_evict`) hand-rolled an identical `tokio::interval`
//! loop with `select! { cancel, tick }` + tracing boilerplate.
//! Three copies meant a style change had to land in three places
//! and a fourth cron meant copy-pasting ~30 lines.
//!
//! The trait captures the three decisions that actually differ per
//! cron — name, interval, what to do each tick — and leaves the
//! scheduling skeleton to the shared [`spawn_cron`]. Fire-on-start
//! is a boolean that defaults to `true` so the common case
//! ("reconcile on boot so a fresh cluster sees its first output
//! without a 24h wait") is the default. Rare jobs like the
//! clarification-tracker evict that should NOT fire immediately
//! override it.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ox_core::error::OxResult;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// One periodic background task. Implementors carry their own
/// dependencies (a `Store`, a shared tracker, etc.) as fields on
/// the impl struct so the trait surface stays tiny and the
/// scheduler doesn't need to thread them.
#[async_trait]
pub trait CronTask: Send + Sync + 'static {
    /// Human-readable name used in shutdown / failure logs and as
    /// a tracing span field. Keep it short and dashed — it shows
    /// up in per-line log output.
    fn name(&self) -> &'static str;

    /// Interval between successive `run_once` invocations.
    fn interval(&self) -> Duration;

    /// Whether the first `run_once` fires immediately on spawn.
    /// Most jobs want `true` so a fresh deploy sees output without
    /// waiting one full interval; jobs whose correct behaviour
    /// depends on accumulated state (e.g. an evict sweep on a
    /// just-initialised tracker has nothing to do) override to
    /// `false` to skip the initial tick.
    fn fire_on_start(&self) -> bool {
        true
    }

    /// Singleton coordination key. When `Some`, every tick first
    /// tries to acquire `pg_try_advisory_lock(key)`; only the
    /// holding replica runs `run_once`, the rest skip until the
    /// next interval. Use for sweeps that race on shared writes
    /// (stale-concept marker bumps, baseline rollups, soft-delete
    /// compactions) — without singleton coordination every
    /// horizontal replica runs the same job concurrently. Returning
    /// `None` (default) lets every replica run on every tick — the
    /// right shape for jobs that are either side-effect-free reads
    /// or naturally idempotent under contention.
    fn singleton_key(&self) -> Option<i64> {
        None
    }

    /// Run the job once. Errors are logged at WARN by the
    /// scheduler and do not stop the loop — a cron is best-effort
    /// and a single failed sweep shouldn't wedge the interval.
    async fn run_once(&self) -> OxResult<()>;
}

/// Spawn a cron task under the system scope. `SYSTEM_BYPASS` and
/// `WORKSPACE_ID` task-locals are set by `spawn_system`; individual
/// tasks enter per-workspace scopes inside `run_once` when they
/// need RLS-scoped writes.
///
/// When the task declares a `singleton_key` AND `pool` is `Some`,
/// the tick first tries to acquire that PostgreSQL advisory lock;
/// the holding replica runs `run_once` and the rest skip. The skip
/// is silent — racing replicas are an expected, healthy state, not
/// a warning surface. `pool: None` is the test-only path: no
/// singleton coordination, every spawn always runs.
pub fn spawn_cron(
    task: Arc<dyn CronTask>,
    pool: Option<ox_store::PgPool>,
    cancel: CancellationToken,
) {
    let task_for_spawn = Arc::clone(&task);
    ox_context::spawn_system(async move {
        let mut ticker = tokio::time::interval(task_for_spawn.interval());
        if !task_for_spawn.fire_on_start() {
            // `tokio::time::interval` fires immediately on first
            // tick; consuming it up-front shifts the schedule so
            // the real first sweep lands after one interval.
            ticker.tick().await;
        }
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!(cron = task_for_spawn.name(), "cron task shutting down");
                    break;
                }
                _ = ticker.tick() => {
                    let outcome = match (task_for_spawn.singleton_key(), &pool) {
                        (Some(key), Some(p)) => {
                            ox_store::advisory_lock::try_advisory_lock(
                                p,
                                key,
                                || task_for_spawn.run_once(),
                            )
                            .await
                            .map(|opt| opt.is_some())
                        }
                        _ => task_for_spawn.run_once().await.map(|_| true),
                    };
                    if let Err(e) = outcome {
                        warn!(
                            cron = task_for_spawn.name(),
                            error = %e,
                            "cron run failed — continuing to next tick"
                        );
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingTask {
        interval: Duration,
        fire_on_start: bool,
        ticks: Arc<AtomicUsize>,
        fail: bool,
    }

    #[async_trait]
    impl CronTask for CountingTask {
        fn name(&self) -> &'static str {
            "counting"
        }
        fn interval(&self) -> Duration {
            self.interval
        }
        fn fire_on_start(&self) -> bool {
            self.fire_on_start
        }
        async fn run_once(&self) -> OxResult<()> {
            self.ticks.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(ox_core::error::OxError::Runtime {
                    message: "intentional failure".into(),
                });
            }
            Ok(())
        }
    }

    #[test]
    fn fire_on_start_defaults_to_true() {
        // Default-override semantics — a task that doesn't override
        // `fire_on_start` gets `true`. This is the scheduler's most
        // common call site; if the default flips accidentally,
        // stale_concepts and quality_baseline silently lose their
        // immediate-startup-sweep property.
        struct Bare;
        #[async_trait]
        impl CronTask for Bare {
            fn name(&self) -> &'static str {
                "bare"
            }
            fn interval(&self) -> Duration {
                Duration::from_secs(3600)
            }
            async fn run_once(&self) -> OxResult<()> {
                Ok(())
            }
        }
        assert!(Bare.fire_on_start());
    }

    #[tokio::test]
    async fn run_once_records_each_invocation() {
        // Direct call on the impl — no scheduler, no time. Proves
        // the trait body contract: run_once increments the counter
        // once per call and surfaces Err when the task asks it to.
        let ticks = Arc::new(AtomicUsize::new(0));
        let task = CountingTask {
            interval: Duration::from_secs(60),
            fire_on_start: true,
            ticks: Arc::clone(&ticks),
            fail: false,
        };
        assert!(task.run_once().await.is_ok());
        assert!(task.run_once().await.is_ok());
        assert_eq!(ticks.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn run_once_propagates_err() {
        let ticks = Arc::new(AtomicUsize::new(0));
        let task = CountingTask {
            interval: Duration::from_secs(60),
            fire_on_start: true,
            ticks: Arc::clone(&ticks),
            fail: true,
        };
        let result = task.run_once().await;
        assert!(result.is_err());
        // Scheduler will log + continue — the trait body still
        // records its tick on the way out, so the counter moved.
        assert_eq!(ticks.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn spawn_cron_accepts_trait_object_and_graceful_shutdown() {
        // Schedule the task with a generous interval, cancel
        // immediately, and assert the scheduler neither panics nor
        // holds the runtime open — graceful-shutdown semantics.
        let ticks = Arc::new(AtomicUsize::new(0));
        let task = Arc::new(CountingTask {
            interval: Duration::from_secs(3600),
            fire_on_start: false,
            ticks: Arc::clone(&ticks),
            fail: false,
        });
        let cancel = CancellationToken::new();
        spawn_cron(task, None, cancel.clone());
        cancel.cancel();
        // Yield so the spawned task observes the cancel.
        tokio::task::yield_now().await;
    }
}
