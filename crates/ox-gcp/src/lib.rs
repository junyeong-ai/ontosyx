//! GCP integration utilities for Ontosyx.
//!
//! Currently scoped to Application Default Credentials (ADC) dispatch.
//! Ontosyx talks to GCP from two places — the BigQuery data-source
//! adapter (`ox-source`) and the optional Secret Manager resolver
//! (`ox-api`'s `gcp-sm` feature) — and both have to traverse the same
//! ADC chain. Centralising the dispatch here keeps the `yup-oauth2`
//! and `hyper-util` dependencies out of those crates and gives future
//! GCP integrations (GCS, Pub/Sub, IAM) a single seam to plug into.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod auth;
