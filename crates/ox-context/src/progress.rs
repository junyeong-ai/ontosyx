//! `ProgressReporter` — request-scoped progress channel piggybacking
//! on [`entelix::ExecutionContext::add_extension`].
//!
//! Brain operations and tools emit step-level progress
//! (`schema_discovery`, `llm_primary`, `verified_query_lookup`, …) so
//! the SSE chat handler can render real-time UI updates. The channel
//! is *opt-in*: when no [`ProgressReporter`] is attached to the
//! context, every emit short-circuits to a no-op so non-streaming
//! callers (background jobs, eval-case execution, unit tests) pay
//! nothing.
//!
//! ## Wiring
//!
//! ```ignore
//! // ox-api SSE handler:
//! let sink: Arc<dyn ProgressSink> = Arc::new(SseProgressSink::new(tx));
//! let ctx = ExecutionContext::default()
//!     .add_extension(ProgressReporter::new(sink));
//!
//! // ox-brain Brain method:
//! ctx.progress("schema_discovery").started();
//! let elapsed = … ;
//! ctx.progress("schema_discovery").completed(elapsed);
//! ```

use std::sync::Arc;

use entelix::{AgentContext, ExecutionContext};

/// One progress emission. `step` names the logical phase; the
/// duration tracks how long the phase took at the call site (the
/// reporter does not measure on its own — operators time the phase
/// and pass `duration_ms` so the same handle can serve sync /
/// streaming / batched callers without timing skew).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum ProgressEvent {
    /// Phase started. No duration yet.
    Started {
        /// Phase identifier — small `snake_case` string operators key on.
        step: String,
    },
    /// Phase finished successfully.
    Completed {
        /// Phase identifier (matches the prior `Started`).
        step: String,
        /// Wall-clock milliseconds the phase took.
        duration_ms: u64,
        /// Optional structured payload — operators surface intermediate
        /// data (cache outcome, label set, retry count) for the FE to
        /// render alongside the progress bar.
        payload: Option<serde_json::Value>,
    },
    /// Phase failed. The `payload` typically carries the error
    /// message so the FE can render the failure inline rather than
    /// waiting for a terminal error response.
    Failed {
        /// Phase identifier.
        step: String,
        /// Wall-clock milliseconds before failure.
        duration_ms: u64,
        /// Optional structured payload — typically `{"error": "<msg>"}`.
        payload: Option<serde_json::Value>,
    },
}

/// Sink that consumes [`ProgressEvent`] emissions. The SSE handler
/// implements this to forward events as wire frames; tests and
/// background jobs use [`NoopProgressSink`] (or simply attach no
/// reporter at all).
pub trait ProgressSink: Send + Sync + 'static {
    /// Receive one event. Errors are the sink's responsibility — the
    /// reporter is fire-and-forget and never blocks the caller.
    fn emit(&self, event: ProgressEvent);
}

/// Sink that drops every event silently. Useful in test harnesses
/// that exercise the Brain path without spinning up an SSE channel.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopProgressSink;

impl ProgressSink for NoopProgressSink {
    fn emit(&self, _event: ProgressEvent) {}
}

/// Wrapper holding the operator-supplied [`ProgressSink`]. Stored on
/// [`ExecutionContext::add_extension`] so any Brain / tool method
/// reachable from the same request finds the sink without an
/// explicit parameter.
#[derive(Clone)]
pub struct ProgressReporter {
    sink: Arc<dyn ProgressSink>,
}

impl ProgressReporter {
    /// Wrap a [`ProgressSink`] for attachment to an
    /// [`ExecutionContext`].
    #[must_use]
    pub fn new(sink: Arc<dyn ProgressSink>) -> Self {
        Self { sink }
    }

    /// Wrap an arbitrary concrete sink. Equivalent to
    /// `ProgressReporter::new(Arc::new(sink))` with the `Arc` boxing
    /// hidden so call sites stay compact.
    #[must_use]
    pub fn from_sink<S>(sink: S) -> Self
    where
        S: ProgressSink,
    {
        Self {
            sink: Arc::new(sink),
        }
    }

    fn emit(&self, event: ProgressEvent) {
        self.sink.emit(event);
    }
}

impl std::fmt::Debug for ProgressReporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgressReporter")
            .field("sink", &"<dyn ProgressSink>")
            .finish()
    }
}

/// Single progress phase handle. Construct via
/// [`ProgressContextExt::progress`]; finish with [`Self::completed`]
/// (or [`Self::completed_with`]) on success, [`Self::failed`] on
/// error.
///
/// The handle holds a clone of the reporter (`None` when the context
/// has no reporter attached) so emit sites are zero-cost beyond the
/// `Arc` clone when the reporter is wired and a single `is_some`
/// branch when it is not.
pub struct ProgressHandle {
    reporter: Option<ProgressReporter>,
    step: String,
}

impl ProgressHandle {
    fn new(reporter: Option<ProgressReporter>, step: String) -> Self {
        Self { reporter, step }
    }

    /// Mark the phase as started. Operators time their own phase via
    /// `std::time::Instant::now()` and pass the elapsed measurement to
    /// the matching [`Self::completed`] / [`Self::failed`] call —
    /// keeps the reporter free of timing concerns and lets retries /
    /// nested phases produce honest measurements.
    pub fn started(self) {
        if let Some(reporter) = self.reporter {
            reporter.emit(ProgressEvent::Started { step: self.step });
        }
    }

    /// Mark the phase as completed. `duration_ms` is the operator's
    /// own measurement so retries / fallbacks / nested phases produce
    /// honest timings.
    pub fn completed(self, duration_ms: u64) {
        if let Some(reporter) = self.reporter {
            reporter.emit(ProgressEvent::Completed {
                step: self.step,
                duration_ms,
                payload: None,
            });
        }
    }

    /// Same as [`Self::completed`] with an attached structured
    /// payload — typically the phase's intermediate output the FE
    /// renders inline.
    pub fn completed_with(self, duration_ms: u64, payload: serde_json::Value) {
        if let Some(reporter) = self.reporter {
            reporter.emit(ProgressEvent::Completed {
                step: self.step,
                duration_ms,
                payload: Some(payload),
            });
        }
    }

    /// Mark the phase as failed. `duration_ms` is the elapsed time
    /// before the failure surfaced.
    pub fn failed(self, duration_ms: u64) {
        if let Some(reporter) = self.reporter {
            reporter.emit(ProgressEvent::Failed {
                step: self.step,
                duration_ms,
                payload: None,
            });
        }
    }

    /// Same as [`Self::failed`] with an attached structured
    /// payload — typically `{"error": "<message>"}` for the FE to
    /// surface inline.
    pub fn failed_with(self, duration_ms: u64, payload: serde_json::Value) {
        if let Some(reporter) = self.reporter {
            reporter.emit(ProgressEvent::Failed {
                step: self.step,
                duration_ms,
                payload: Some(payload),
            });
        }
    }
}

/// Extension trait that exposes a `progress(step)` builder on every
/// request-scope context the SDK surfaces.
///
/// Implemented for [`entelix::ExecutionContext`] (the layer-side
/// carrier) and [`entelix::AgentContext<D>`] (the tool-side
/// carrier), so brain calls and tool dispatches both reach the
/// reporter through one trait. Consumers `use
/// ox_context::ProgressContextExt;` to call `ctx.progress("step")`.
pub trait ProgressContextExt {
    /// Open a new [`ProgressHandle`] bound to `step`. Returns a
    /// no-op handle when the context carries no
    /// [`ProgressReporter`] extension.
    fn progress(&self, step: impl Into<String>) -> ProgressHandle;
}

impl ProgressContextExt for ExecutionContext {
    fn progress(&self, step: impl Into<String>) -> ProgressHandle {
        let reporter = self
            .extension::<ProgressReporter>()
            .map(|arc| (*arc).clone());
        ProgressHandle::new(reporter, step.into())
    }
}

impl<D> ProgressContextExt for AgentContext<D> {
    fn progress(&self, step: impl Into<String>) -> ProgressHandle {
        // `AgentContext::extension` forwards to the wrapped
        // `ExecutionContext::extension`, so tool-side calls pick up
        // the same reporter the brain-side calls saw — single
        // source of truth, attached once per request at the route
        // boundary.
        let reporter = self
            .extension::<ProgressReporter>()
            .map(|arc| (*arc).clone());
        ProgressHandle::new(reporter, step.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    struct CapturingSink {
        events: Arc<Mutex<Vec<ProgressEvent>>>,
    }

    impl ProgressSink for CapturingSink {
        fn emit(&self, event: ProgressEvent) {
            self.events.lock().push(event);
        }
    }

    fn sink() -> (Arc<Mutex<Vec<ProgressEvent>>>, ProgressReporter) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let reporter = ProgressReporter::from_sink(CapturingSink {
            events: Arc::clone(&events),
        });
        (events, reporter)
    }

    #[test]
    fn progress_without_reporter_is_noop() {
        let ctx = ExecutionContext::default();
        ctx.progress("phase").started();
        ctx.progress("phase").completed(0);
        // No panic, no crash — the absence of a reporter must not
        // change observable behaviour for non-streaming callers.
    }

    #[test]
    fn started_completed_failed_round_trip() {
        let (events, reporter) = sink();
        let ctx = ExecutionContext::default().add_extension(reporter);

        ctx.progress("phase_one").started();
        ctx.progress("phase_one").completed(42);

        ctx.progress("phase_two").started();
        ctx.progress("phase_two")
            .failed_with(7, serde_json::json!({"error": "boom"}));

        let recorded = events.lock().clone();
        assert_eq!(recorded.len(), 4);
        assert!(matches!(recorded[0], ProgressEvent::Started { ref step } if step == "phase_one"));
        assert!(
            matches!(recorded[1], ProgressEvent::Completed { ref step, duration_ms: 42, .. } if step == "phase_one")
        );
        assert!(matches!(recorded[2], ProgressEvent::Started { ref step } if step == "phase_two"));
        assert!(
            matches!(&recorded[3], ProgressEvent::Failed { step, duration_ms: 7, payload: Some(p) }
                if step == "phase_two" && p["error"] == "boom")
        );
    }

    #[test]
    fn completed_with_payload_carries_outcome() {
        let (events, reporter) = sink();
        let ctx = ExecutionContext::default().add_extension(reporter);

        ctx.progress("verified_query_lookup")
            .completed_with(3, serde_json::json!({"outcome": "hit", "vq_id": "abc"}));

        let recorded = events.lock().clone();
        assert_eq!(recorded.len(), 1);
        let ProgressEvent::Completed {
            payload: Some(p), ..
        } = &recorded[0]
        else {
            panic!("expected Completed with payload, got {:?}", recorded[0]);
        };
        assert_eq!(p["outcome"], "hit");
        assert_eq!(p["vq_id"], "abc");
    }
}
