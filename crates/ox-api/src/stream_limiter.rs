//! Per-user concurrent-chat-stream limiter.
//!
//! A single agent session keeps a long-lived SSE connection open and burns
//! LLM tokens for the duration. Without a cap a malicious or buggy client
//! can open dozens of streams in parallel and exhaust either the Anthropic
//! rate limit, the workspace's budget, or the process's file-descriptor
//! ceiling — sometimes all three. The Ontosyx rate limiter caps request
//! throughput but not concurrency; this module fills that gap.
//!
//! The design is a `DashMap<user_key, Arc<Semaphore>>` with permits
//! equal to `max_concurrent_streams_per_user`. Acquiring a permit is
//! cheap (one atomic CAS + a conditional wait). The permit is held by a
//! `StreamSlot` RAII guard; dropping the guard releases the permit
//! automatically, so a client that disconnects mid-stream always
//! relinquishes its slot.
//!
//! `max_concurrent_streams_per_user = 0` disables the limiter — useful
//! in local development where a single human runs hot-reload loops and
//! might hit the default 5 permits briefly.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Per-user stream budget. Lives inside `AppState`, shared across all
/// chat-stream handlers via `Arc`.
pub struct StreamLimiter {
    max_per_user: u32,
    slots: DashMap<String, Arc<Semaphore>>,
}

impl StreamLimiter {
    /// Build a limiter. `max_per_user = 0` disables the check.
    pub fn new(max_per_user: u32) -> Self {
        Self {
            max_per_user,
            slots: DashMap::new(),
        }
    }

    /// Try to acquire a stream slot for `user_key`. Returns `None` when
    /// the user's budget is already exhausted; returns a `StreamSlot`
    /// guard when a permit was granted. The permit is released when the
    /// guard is dropped — either normally (stream completes) or when the
    /// client disconnects and the handler future is cancelled.
    ///
    /// Pass the JWT `sub` for interactive users and the API-key label
    /// (`apikey:<label>`) for machine principals so different keys share
    /// neither the budget nor the collision risk.
    pub fn try_acquire(&self, user_key: &str) -> Option<StreamSlot> {
        if self.max_per_user == 0 {
            // Disabled — return a guard tied to no semaphore.
            return Some(StreamSlot { _permit: None });
        }

        let sem = self
            .slots
            .entry(user_key.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.max_per_user as usize)))
            .clone();

        let permit = sem.try_acquire_owned().ok()?;
        Some(StreamSlot {
            _permit: Some(permit),
        })
    }

    /// Upper bound on concurrent streams per user. Surface as telemetry
    /// or in error messages so clients know the limit without inspecting
    /// config.
    pub fn max_per_user(&self) -> u32 {
        self.max_per_user
    }
}

/// RAII guard holding one stream slot. Dropping releases the permit.
pub struct StreamSlot {
    _permit: Option<OwnedSemaphorePermit>,
}
