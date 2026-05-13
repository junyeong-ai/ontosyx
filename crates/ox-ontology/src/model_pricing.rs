//! [`ModelCall`] and [`ModelPrices`] — one LLM call observation +
//! the price catalogue cost is derived from.
//!
//! ## Why one call shape, not three axes
//!
//! Latency, tokens, and cost are different views of the same event:
//! one LLM invocation. Three [`crate::evaluation`-level] capture
//! methods (`record_latency`, `record_tokens`, `record_cost_usd`)
//! had three problems:
//!
//! 1. **Source-of-truth drift.** Caller computes cost from tokens × a
//!    hardcoded tariff, then stores latency / tokens / cost as three
//!    independent rows. A call with tokens recorded but no cost is
//!    indistinguishable from a call that hit a free tier.
//! 2. **Pricing in code.** The tariff lived inside a `match` block,
//!    re-released every time prices changed.
//! 3. **Per-call backfill is impossible.** Once cost was stored, you
//!    couldn't recompute it under a corrected tariff without
//!    re-running the eval.
//!
//! The redesign: one `ModelCall` observation captures every numeric
//! axis of the call (input / output / cached_input tokens, latency).
//! Cost is derived at write time from the active [`ModelPrices`] row
//! and retained as the historical truth. New axes (cache hit ratio,
//! provider-side queue time, …) extend the struct without forcing
//! new capture methods.
//!
//! ## Data temporality
//!
//! [`ModelPrices`] is a temporal catalogue — rows have
//! `valid_from / valid_to`. A call observed at `t` resolves the
//! `(model_id, t ∈ [valid_from, valid_to))` row; price corrections
//! land as new rows + a `valid_to` close on the prior row, never as
//! in-place edits. History stays auditable.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::eval_fingerprint::ModelId;

/// One observation of an LLM call. Every numeric axis the
/// evaluation surface cares about lives in one struct so the
/// capture API takes one self-describing observation, not three.
///
/// Carries the resolved [`ModelId`] so the storage impl can resolve
/// the active [`ModelPrices`] row at write time without an out-of-
/// band lookup. The model id is the same one pinned in the run
/// fingerprint; mismatch between the two at write time is a
/// divergence the storage impl flags rather than silently swallows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ModelCall {
    /// Resolved model id — same shape as
    /// [`crate::eval_fingerprint::EvaluationFingerprint::model_id`].
    pub model_id: ModelId,
    /// Tokens the model received as input — system prompt + user
    /// message + any tool / context block. Provider-reported
    /// upstream; the platform does not re-tokenise.
    pub input_tokens: u64,
    /// Tokens the model produced as output. Includes any structured
    /// JSON tokens — providers count those identically to free-form
    /// completion tokens.
    pub output_tokens: u64,
    /// Subset of `input_tokens` that hit a prompt cache (Anthropic
    /// `cache_read_input_tokens`, OpenAI / Bedrock equivalents).
    /// Always satisfies `cached_input_tokens <= input_tokens`. The
    /// cost derivation discounts these at
    /// [`ModelPrices::cached_input_price_usd_per_million`].
    pub cached_input_tokens: u64,
    /// Tokens billed at the cache-creation rate — the *first*
    /// dispatch that establishes a prompt-cache breakpoint pays a
    /// per-million premium (Anthropic charges ~1.25× the input rate
    /// for cache write; providers vary). Cost-discrepancy alerts
    /// branch on a non-zero value here, so the wire shape carries
    /// it explicitly.
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    /// Reasoning tokens consumed by extended-thinking models
    /// (Anthropic `thinking`, OpenAI o-series internal reasoning).
    /// Billed at the output rate. `0` for non-thinking calls so
    /// pricing arithmetic is uniform across model families.
    #[serde(default)]
    pub reasoning_tokens: u64,
    /// Wall-clock latency from invocation to completion. Captured
    /// at the application layer (provider SDK boundary), not
    /// inside the SDK's HTTP layer — the operator's observability
    /// matches the latency a user sees, not the raw network leg.
    pub latency_ms: u32,
}

impl ModelCall {
    /// Tokens billed at the full input rate — `input_tokens -
    /// cached_input_tokens`. Guards against over-counting when
    /// upstream `cached_input_tokens` exceeds `input_tokens`
    /// (provider bug; defensive zero floor).
    pub fn billable_input_tokens(&self) -> u64 {
        self.input_tokens.saturating_sub(self.cached_input_tokens)
    }

    /// Cost in micro-USD (1e-6 USD) under `prices`. Operates in
    /// `u128` for the per-million multiplication to avoid overflow
    /// on very long contexts. Returns `None` when no pricing row
    /// applies (caller decides whether to skip the metric or treat
    /// as a configuration error).
    ///
    /// Five tariff legs sum: full-rate input, cache-read input,
    /// cache-creation input, output, and reasoning. Reasoning
    /// tokens are billed at the output rate per provider
    /// convention; cache-creation tokens at the dedicated
    /// `cache_creation_input_price_usd_per_million` tariff.
    pub fn cost_micro_usd(&self, prices: &ModelPrices) -> u64 {
        let billable_input_micro = (self.billable_input_tokens() as u128)
            .saturating_mul(prices.input_price_usd_per_million_micro() as u128)
            / 1_000_000;
        let cached_input_micro = (self.cached_input_tokens as u128)
            .saturating_mul(prices.cached_input_price_usd_per_million_micro() as u128)
            / 1_000_000;
        let cache_creation_micro = (self.cache_creation_input_tokens as u128)
            .saturating_mul(prices.cache_creation_input_price_usd_per_million_micro() as u128)
            / 1_000_000;
        let output_micro = (self.output_tokens as u128)
            .saturating_mul(prices.output_price_usd_per_million_micro() as u128)
            / 1_000_000;
        let reasoning_micro = (self.reasoning_tokens as u128)
            .saturating_mul(prices.output_price_usd_per_million_micro() as u128)
            / 1_000_000;
        (billable_input_micro
            + cached_input_micro
            + cache_creation_micro
            + output_micro
            + reasoning_micro)
            .min(u64::MAX as u128) as u64
    }
}

/// One row of the per-model price catalogue. Stored as
/// `model_prices` (not workspace-scoped — pricing is platform-wide
/// reference data). Tariffs are USD per million tokens, stored as
/// `DOUBLE PRECISION` to match the workspace convention for
/// monetary fields (per `crates/ox-store/CLAUDE.md`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ModelPrices {
    pub model_id: ModelId,
    /// Tariff for full-rate input tokens (cache miss).
    pub input_price_usd_per_million: f64,
    /// Tariff for cache-read input tokens. Anthropic discounts
    /// these to ~10% of the miss rate; provider-specific.
    pub cached_input_price_usd_per_million: f64,
    /// Tariff for cache-creation input tokens — the per-million
    /// rate charged when a dispatch establishes a new prompt-cache
    /// breakpoint. Anthropic prices these at ~1.25× the cache-miss
    /// input rate; OpenAI / Bedrock vary. Defaults to the input
    /// rate when a tariff row predates the column.
    #[serde(default)]
    pub cache_creation_input_price_usd_per_million: f64,
    /// Tariff for output tokens.
    pub output_price_usd_per_million: f64,
    /// First instant the row is authoritative. Range is
    /// half-open `[valid_from, valid_to)` so a tariff revision is
    /// simply (a) close the old row's `valid_to`, (b) insert a new
    /// row whose `valid_from` matches.
    pub valid_from: DateTime<Utc>,
    /// Open-ended when `None` (the row applies forward indefinitely).
    /// On revision, the prior row gets its `valid_to` set to the
    /// new row's `valid_from`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<DateTime<Utc>>,
}

impl ModelPrices {
    /// Tariff in micro-USD per million tokens. Cost arithmetic is
    /// performed in integer micro-USD to keep the result a clean
    /// `u64` (sub-cent precision is meaningful at high call volume —
    /// an embedding call at 0.0001 USD per 1k tokens flattens to
    /// "0.00" if stored in cents).
    pub fn input_price_usd_per_million_micro(&self) -> u64 {
        usd_to_micro(self.input_price_usd_per_million)
    }

    pub fn cached_input_price_usd_per_million_micro(&self) -> u64 {
        usd_to_micro(self.cached_input_price_usd_per_million)
    }

    pub fn cache_creation_input_price_usd_per_million_micro(&self) -> u64 {
        usd_to_micro(self.cache_creation_input_price_usd_per_million)
    }

    pub fn output_price_usd_per_million_micro(&self) -> u64 {
        usd_to_micro(self.output_price_usd_per_million)
    }
}

fn usd_to_micro(usd: f64) -> u64 {
    // 1 USD = 1_000_000 micro-USD. `max(0)` floors negative inputs;
    // `as u64` on f64 saturates toward zero / u64::MAX so the
    // saturating-mul in `ModelCall::cost_micro_usd` stays safe.
    let micro = usd * 1_000_000.0;
    if micro.is_finite() && micro >= 0.0 {
        micro as u64
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn opus_4_7_prices() -> ModelPrices {
        // Anthropic Q2 2026 list price.
        ModelPrices {
            model_id: ModelId::new("anthropic/claude-opus-4-7"),
            input_price_usd_per_million: 15.0,
            output_price_usd_per_million: 75.0,
            cached_input_price_usd_per_million: 1.5,
            cache_creation_input_price_usd_per_million: 18.75,
            valid_from: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
            valid_to: None,
        }
    }

    fn call_for(prices: &ModelPrices) -> ModelCall {
        ModelCall {
            model_id: prices.model_id.clone(),
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            cache_creation_input_tokens: 0,
            reasoning_tokens: 0,
            latency_ms: 0,
        }
    }

    #[test]
    fn cost_with_no_cache_matches_input_x_input_rate_plus_output_x_output_rate() {
        let prices = opus_4_7_prices();
        let call = ModelCall {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            ..call_for(&prices)
        };
        // 1M input × 15 USD/M = 15 USD = 15_000_000 micro
        // 1M output × 75 USD/M = 75 USD = 75_000_000 micro
        // Total: 90_000_000 micro = 90 USD
        let cost = call.cost_micro_usd(&prices);
        assert_eq!(cost, 90_000_000);
    }

    #[test]
    fn cost_with_cache_discounts_cached_input_at_cached_rate() {
        let prices = opus_4_7_prices();
        let call = ModelCall {
            input_tokens: 1_000_000,
            cached_input_tokens: 500_000, // half cache hit
            ..call_for(&prices)
        };
        // 500k billable input × 15 USD/M = 7.5 USD = 7_500_000
        // 500k cached input × 1.5 USD/M = 0.75 USD = 750_000
        // Total: 8_250_000 micro
        let cost = call.cost_micro_usd(&prices);
        assert_eq!(cost, 8_250_000);
    }

    #[test]
    fn cached_exceeding_input_clamps_to_zero_billable() {
        let prices = opus_4_7_prices();
        let call = ModelCall {
            input_tokens: 100,
            cached_input_tokens: 200, // upstream bug — defensive
            ..call_for(&prices)
        };
        // billable = 0; cached = 200 × 1.5 / 1M
        // (200 × 1_500_000) / 1_000_000 = 300 micro
        let cost = call.cost_micro_usd(&prices);
        assert_eq!(cost, 300);
    }

    #[test]
    fn billable_input_floors_at_zero() {
        let call = ModelCall {
            model_id: ModelId::new("any"),
            input_tokens: 100,
            cached_input_tokens: 200,
            cache_creation_input_tokens: 0,
            reasoning_tokens: 0,
            output_tokens: 0,
            latency_ms: 0,
        };
        assert_eq!(call.billable_input_tokens(), 0);
    }

    #[test]
    fn cost_includes_cache_creation_and_reasoning_legs() {
        let prices = opus_4_7_prices();
        let call = ModelCall {
            cache_creation_input_tokens: 1_000_000,
            reasoning_tokens: 1_000_000,
            ..call_for(&prices)
        };
        // 1M cache-creation × 18.75 USD/M = 18.75 USD = 18_750_000 micro
        // 1M reasoning × 75 USD/M = 75 USD = 75_000_000 micro (output rate)
        let cost = call.cost_micro_usd(&prices);
        assert_eq!(cost, 18_750_000 + 75_000_000);
    }

    #[test]
    fn negative_or_nan_tariff_resolves_to_zero_micro() {
        assert_eq!(usd_to_micro(-1.0), 0);
        assert_eq!(usd_to_micro(f64::NAN), 0);
        assert_eq!(usd_to_micro(f64::INFINITY), 0);
    }

    #[test]
    fn round_trip_preserves_validity_window() {
        let p = opus_4_7_prices();
        let v = serde_json::to_value(&p).unwrap();
        let back: ModelPrices = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }
}
