// Library entrypoint for ox-api.
//
// The HTTP server binary (`ontosyx`) lives in `main.rs`; auxiliary binaries
// (e.g., `dump_openapi`) live under `src/bin/`. All shared modules are
// declared here so they can be reached from every binary via `ox_api::*`.

pub mod acl_enforcement;
pub mod audit_middleware;
pub mod collaboration;
pub mod config;
pub mod error;
pub mod mcp;
pub mod metrics;
pub mod middleware;
pub mod model_router;
pub mod openapi;
pub mod principal;
pub mod response;
pub mod routes;
pub mod schedule;
pub mod spawn_scoped;
pub mod sso;
pub mod state;
pub mod system_config;
pub mod validation;
pub mod workspace;
pub mod workspace_scope;
