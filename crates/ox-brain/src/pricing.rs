//! Pricing-shape conversion between ontosyx's `ModelPrices` domain
//! record and entelix's [`PricingTable`].
//!
//! The conversion lives here (not in ox-store) because it bridges
//! two SDK shapes. ox-store owns the persistence query
//! (`PostgresStore::list_active_model_prices`); the orchestration
//! that loads → converts → wires the resulting `PricingTable` into
//! `entelix::CostMeter` lives at the application root
//! (`ox-api::main`). This module is the pure-function adapter
//! between those steps.
//!
//! Ontosyx prices are USD-per-million-tokens; entelix expects
//! USD-per-1000-tokens. The conversion is a single `/ 1000`
//! division per axis, done in `Decimal` to avoid float drift.

use entelix::{ModelPricing, PricingTable};
use rust_decimal::Decimal;

use ox_core::error::{OxError, OxResult};
use ox_ontology::ModelPrices;

/// Convert an ontosyx pricing row to entelix's per-1k shape.
///
/// Reasoning tokens are billed at the output rate per provider
/// convention; entelix's `ModelPricing::cost_for` folds reasoning
/// into the `output_tokens` axis upstream, so no separate
/// reasoning field is needed here.
///
/// Returns `Err` when any per-million rate fails to convert to
/// `Decimal` (NaN, infinity, or magnitude beyond `Decimal`'s
/// range). A silent fallback to zero would mask catalogue
/// corruption: a `0.0` per-million row is a valid "free model"
/// declaration, indistinguishable from a load-time conversion
/// failure once the value lands in the `PricingTable`.
pub fn convert_pricing(prices: &ModelPrices) -> OxResult<ModelPricing> {
    let model_id = prices.model_id.as_str();
    let convert = |axis: &str, usd_per_million: f64| -> OxResult<Decimal> {
        Decimal::try_from(usd_per_million)
            .map(|v| v / Decimal::from(1000))
            .map_err(|err| OxError::Validation {
                field: format!("model_prices[{model_id}].{axis}"),
                message: format!("invalid USD-per-million tariff `{usd_per_million}`: {err}"),
            })
    };
    Ok(ModelPricing::new(
        convert(
            "input_price_usd_per_million",
            prices.input_price_usd_per_million,
        )?,
        convert(
            "output_price_usd_per_million",
            prices.output_price_usd_per_million,
        )?,
        convert(
            "cached_input_price_usd_per_million",
            prices.cached_input_price_usd_per_million,
        )?,
        convert(
            "cache_creation_input_price_usd_per_million",
            prices.cache_creation_input_price_usd_per_million,
        )?,
    ))
}

/// Build a complete [`PricingTable`] from a snapshot of currently-
/// active price rows. Keys are the on-the-wire `model_id` strings
/// the codecs send to providers, matching the lookup `CostMeter`
/// performs against `ModelResponse::model`. Aborts on the first
/// catalogue row that fails to convert — boot-fail is the
/// operator-actionable signal that the `model_prices` table holds
/// non-finite or out-of-range data.
pub fn pricing_table_from(prices: &[ModelPrices]) -> OxResult<PricingTable> {
    let mut table = PricingTable::new();
    for row in prices {
        table.set(row.model_id.as_str(), convert_pricing(row)?);
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_ontology::ModelId;

    fn row(id: &str, input: f64, output: f64, cached: f64, cache_creation: f64) -> ModelPrices {
        ModelPrices {
            model_id: ModelId::new(id),
            input_price_usd_per_million: input,
            cached_input_price_usd_per_million: cached,
            cache_creation_input_price_usd_per_million: cache_creation,
            output_price_usd_per_million: output,
            valid_from: chrono::Utc::now(),
            valid_to: None,
        }
    }

    #[test]
    fn convert_pricing_divides_by_thousand() {
        let r = row("test/m", 15_000.0, 75_000.0, 1500.0, 18_750.0);
        let p = convert_pricing(&r).unwrap();
        assert_eq!(p.input_per_1k, Decimal::from(15));
        assert_eq!(p.output_per_1k, Decimal::from(75));
    }

    #[test]
    fn pricing_table_keys_match_wire_model_id() {
        let rows = vec![
            row(
                "anthropic/claude-opus-4-7",
                15_000.0,
                75_000.0,
                1500.0,
                18_750.0,
            ),
            row("openai/gpt-4o", 2500.0, 10_000.0, 1250.0, 2500.0),
        ];
        let table = pricing_table_from(&rows).unwrap();
        assert_eq!(table.len(), 2);
        assert!(table.get("anthropic/claude-opus-4-7").is_some());
        assert!(table.get("openai/gpt-4o").is_some());
        assert!(table.get("nonexistent").is_none());
    }

    #[test]
    fn pricing_table_empty_for_empty_input() {
        let table = pricing_table_from(&[]).unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn convert_pricing_rejects_nan() {
        let r = row("test/m", f64::NAN, 1.0, 1.0, 1.0);
        let err = convert_pricing(&r).unwrap_err();
        match err {
            OxError::Validation { field, message } => {
                assert!(field.contains("input_price_usd_per_million"));
                assert!(message.contains("NaN"));
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn convert_pricing_rejects_infinity() {
        let r = row("test/m", 1.0, f64::INFINITY, 1.0, 1.0);
        assert!(convert_pricing(&r).is_err());
    }

    #[test]
    fn convert_pricing_accepts_explicit_zero() {
        let r = row("free/model", 0.0, 0.0, 0.0, 0.0);
        let p = convert_pricing(&r).unwrap();
        assert_eq!(p.input_per_1k, Decimal::ZERO);
        assert_eq!(p.output_per_1k, Decimal::ZERO);
    }
}
