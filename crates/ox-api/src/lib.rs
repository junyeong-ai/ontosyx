// Library entrypoint for ox-api.
//
// The HTTP server binary (`ontosyx`) lives in `main.rs`; auxiliary binaries
// (e.g., `dump_openapi`) live under `src/bin/`. All shared modules are
// declared here so they can be reached from every binary via `ox_api::*`.

// Tests inside library modules (e.g., `config.rs`'s env-var section
// tests) use `unwrap`/`expect`/`panic` for setup and assertion
// shortcuts. Scoped to `cfg(test)` so library code is still held to
// the stricter rule for production builds.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

pub mod acl_enforcement;
pub mod agent_event_projection;
pub mod arrow_conversion;
pub mod audit_middleware;
pub mod background;
pub mod collaboration;
pub mod config;
pub mod credential;
pub mod error;
pub mod eval_sampler;
pub mod federation_resolver;
#[cfg(feature = "gcp-sm")]
pub mod gcp_secret_manager;
pub mod idempotency;
pub mod jwt_revocation_cache;
pub mod mcp;
pub mod metrics;
pub mod middleware;
pub mod model_router;
pub mod notifications;
pub mod openapi;
pub mod principal;
pub mod response;
pub mod routes;
pub mod schedule;
pub mod sso;
pub mod state;
pub mod stream_limiter;
pub mod system_config;
#[cfg(test)]
mod test_support;
pub mod tokenizer_publish;
pub mod validation;
pub mod workspace;
