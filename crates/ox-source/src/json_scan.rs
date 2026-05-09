//! Shared JSON-scan fallback for `DataSourceAdapter::scan` implementors.
//!
//! Sibling to [`crate::text_scan`]: `text_scan` handles SQL
//! adapters that cast every cell to its text representation on the
//! server side, `json_scan` handles adapters whose native data form
//! is a [`serde_json::Value`] (inline JSON payloads today; Mongo /
//! CouchDB adapters in the future).
//!
//! The key difference from `text_scan` is that every JSON cell
//! already carries its type at the value level: `Value::Bool`,
//! `Value::Number`, `Value::String`. Dispatching directly on these
//! avoids the round-trip through a normalised `String`, which
//! matters for two reasons:
//!
//! - **Allocation pressure**: a 100 k-row × 10-column scan goes
//!   from 1 M `.to_string()` calls down to zero — every JSON value
//!   borrows into the builder without a detour.
//! - **Precision**: JSON integers outside f64's exact range
//!   (`> 2^53`) round-trip through `as_f64` would lose precision.
//!   Dispatching via `Number::as_i64` first preserves precision for
//!   the i64 range; only then does the code fall back to `as_f64`
//!   for fractional or oversized values.
//!
//! Builders themselves are the same shared factory — `make_builder`
//! is re-exported from [`crate::text_scan::make_builder`]. The two
//! fallback paths share the Arrow type policy (Bool / Int64 /
//! Float64 / Utf8 fallback); only the cell-level decoding differs.

use arrow::array::{ArrayBuilder, BooleanBuilder, Float64Builder, Int64Builder, StringBuilder};
use arrow::datatypes::DataType;
use serde_json::Value;

use ox_core::error::{OxError, OxResult};

/// Append a JSON cell to its builder by dispatching on the
/// [`Value`] variant directly. See module-level doc for the
/// rationale vs. going through `append_text_cell`.
///
/// - `None` and `Some(Value::Null)` both append null.
/// - Type mismatches between the Arrow column and the JSON value
///   fall back to null rather than erroring — a single dirty row
///   does not poison the scan. This matches
///   [`crate::text_scan::append_text_cell`]'s contract.
/// - String-typed Arrow columns accept any JSON shape: strings
///   pass through, scalars stringify, arrays / objects serialise
///   back to JSON.
///
/// `adapter` is a short tag (`"json"`, etc.) that appears in the
/// error message if the builder / DataType pair is inconsistent
/// with what [`crate::text_scan::make_builder`] would have
/// produced — always a bug in the calling adapter, not a data
/// problem, so the error is a `Runtime` variant.
pub fn append_json_cell(
    adapter: &str,
    builder: &mut dyn ArrayBuilder,
    dt: &DataType,
    value: Option<&Value>,
) -> OxResult<()> {
    match dt {
        DataType::Boolean => append_bool(adapter, builder, value),
        DataType::Int64 | DataType::Int32 | DataType::Int16 | DataType::Int8 => {
            append_int(adapter, builder, value)
        }
        DataType::Float64 | DataType::Float32 => append_float(adapter, builder, value),
        _ => append_string(adapter, builder, value),
    }
}

fn append_bool(
    adapter: &str,
    builder: &mut dyn ArrayBuilder,
    value: Option<&Value>,
) -> OxResult<()> {
    let b = builder
        .as_any_mut()
        .downcast_mut::<BooleanBuilder>()
        .ok_or_else(|| builder_mismatch(adapter, "bool"))?;
    match value {
        None | Some(Value::Null) => b.append_null(),
        Some(Value::Bool(v)) => b.append_value(*v),
        // Tolerate string-encoded booleans (`"true"` / `"1"`), matching
        // the text_scan dialect. This is the same token set
        // text_scan uses — dialect drift across adapters stays
        // centralised in one place (if it ever expands, update both).
        Some(Value::String(s)) => match s.to_ascii_lowercase().as_str() {
            "t" | "true" | "1" | "yes" => b.append_value(true),
            "f" | "false" | "0" | "no" => b.append_value(false),
            _ => b.append_null(),
        },
        // Numeric 0 / 1 interpreted as boolean. JSON sources
        // commonly emit booleans as integers.
        Some(Value::Number(n)) => match n.as_i64() {
            Some(0) => b.append_value(false),
            Some(1) => b.append_value(true),
            _ => b.append_null(),
        },
        Some(Value::Array(_)) | Some(Value::Object(_)) => b.append_null(),
    }
    Ok(())
}

fn append_int(
    adapter: &str,
    builder: &mut dyn ArrayBuilder,
    value: Option<&Value>,
) -> OxResult<()> {
    let b = builder
        .as_any_mut()
        .downcast_mut::<Int64Builder>()
        .ok_or_else(|| builder_mismatch(adapter, "int"))?;
    match value {
        None | Some(Value::Null) => b.append_null(),
        // Priority order preserves precision: exact i64 first, then
        // u64 values that fit in i64, then f64 whole numbers, then
        // string parse. Anything else → null.
        Some(Value::Number(n)) => {
            if let Some(v) = n.as_i64() {
                b.append_value(v);
            } else if let Some(u) = n.as_u64()
                && let Ok(v) = i64::try_from(u)
            {
                b.append_value(v);
            } else if let Some(f) = n.as_f64()
                && f.fract() == 0.0
                && f >= i64::MIN as f64
                && f <= i64::MAX as f64
            {
                b.append_value(f as i64);
            } else {
                b.append_null();
            }
        }
        Some(Value::String(s)) => match s.parse::<i64>() {
            Ok(v) => b.append_value(v),
            Err(_) => b.append_null(),
        },
        Some(Value::Bool(_)) | Some(Value::Array(_)) | Some(Value::Object(_)) => b.append_null(),
    }
    Ok(())
}

fn append_float(
    adapter: &str,
    builder: &mut dyn ArrayBuilder,
    value: Option<&Value>,
) -> OxResult<()> {
    let b = builder
        .as_any_mut()
        .downcast_mut::<Float64Builder>()
        .ok_or_else(|| builder_mismatch(adapter, "float"))?;
    match value {
        None | Some(Value::Null) => b.append_null(),
        Some(Value::Number(n)) => match n.as_f64() {
            Some(f) => b.append_value(f),
            None => b.append_null(),
        },
        Some(Value::String(s)) => match s.parse::<f64>() {
            Ok(f) => b.append_value(f),
            Err(_) => b.append_null(),
        },
        Some(Value::Bool(_)) | Some(Value::Array(_)) | Some(Value::Object(_)) => b.append_null(),
    }
    Ok(())
}

fn append_string(
    adapter: &str,
    builder: &mut dyn ArrayBuilder,
    value: Option<&Value>,
) -> OxResult<()> {
    let b = builder
        .as_any_mut()
        .downcast_mut::<StringBuilder>()
        .ok_or_else(|| builder_mismatch(adapter, "string"))?;
    match value {
        None | Some(Value::Null) => b.append_null(),
        // Strings pass through by reference — no allocation.
        Some(Value::String(s)) => b.append_value(s.as_str()),
        // Scalars stringify to their JSON representation.
        Some(Value::Bool(v)) => b.append_value(if *v { "true" } else { "false" }),
        Some(Value::Number(n)) => b.append_value(n.to_string()),
        // Structured values serialise back to JSON. `to_string()`
        // on a Value is documented to be infallible for any
        // already-parsed tree, so this is the one place we accept
        // an allocation — the alternative is losing structural
        // information.
        Some(v @ (Value::Array(_) | Value::Object(_))) => b.append_value(v.to_string()),
    }
    Ok(())
}

fn builder_mismatch(adapter: &str, kind: &str) -> OxError {
    OxError::Runtime {
        message: format!("{adapter} scan: builder type mismatch ({kind})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_scan::make_builder;
    use arrow::array::{Array, BooleanArray, Float64Array, Int64Array, StringArray};
    use serde_json::json;

    fn finish(mut b: Box<dyn ArrayBuilder>) -> arrow::array::ArrayRef {
        b.finish()
    }

    // --- Bool column ---

    #[test]
    fn bool_accepts_native_bool_and_encoded_variants() {
        let mut b = make_builder(&DataType::Boolean);
        // Native.
        append_json_cell("t", b.as_mut(), &DataType::Boolean, Some(&json!(true))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Boolean, Some(&json!(false))).unwrap();
        // String-encoded (text_scan dialect).
        append_json_cell("t", b.as_mut(), &DataType::Boolean, Some(&json!("true"))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Boolean, Some(&json!("F"))).unwrap();
        // Numeric-encoded.
        append_json_cell("t", b.as_mut(), &DataType::Boolean, Some(&json!(1))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Boolean, Some(&json!(0))).unwrap();
        // Rejected shapes.
        append_json_cell("t", b.as_mut(), &DataType::Boolean, Some(&json!(42))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Boolean, Some(&json!(null))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Boolean, None).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Boolean, Some(&json!([1, 2]))).unwrap();

        let arr = finish(b);
        let arr = arr.as_any().downcast_ref::<BooleanArray>().unwrap();
        assert_eq!(arr.len(), 10);
        assert!(arr.is_valid(0) && arr.value(0)); // true
        assert!(arr.is_valid(1) && !arr.value(1)); // false
        assert!(arr.is_valid(2) && arr.value(2)); // "true"
        assert!(arr.is_valid(3) && !arr.value(3)); // "F"
        assert!(arr.is_valid(4) && arr.value(4)); // 1
        assert!(arr.is_valid(5) && !arr.value(5)); // 0
        assert!(arr.is_null(6)); // 42 — not a bool-ish int
        assert!(arr.is_null(7)); // null
        assert!(arr.is_null(8)); // None
        assert!(arr.is_null(9)); // array
    }

    // --- Int64 column ---

    #[test]
    fn int_preserves_i64_precision_beyond_f64_exact_range() {
        // 2^53 + 1 — outside f64's exact integer range. If we went
        // through as_f64, this would round to 2^53 and lose one.
        let big = 9_007_199_254_740_993_i64;
        let mut b = make_builder(&DataType::Int64);
        append_json_cell("t", b.as_mut(), &DataType::Int64, Some(&json!(big))).unwrap();
        let arr = finish(b);
        let arr = arr.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(arr.value(0), big);
    }

    #[test]
    fn int_accepts_whole_float_and_string_forms() {
        let mut b = make_builder(&DataType::Int64);
        append_json_cell("t", b.as_mut(), &DataType::Int64, Some(&json!(42))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Int64, Some(&json!(3.0))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Int64, Some(&json!("100"))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Int64, Some(&json!(3.5))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Int64, Some(&json!("nope"))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Int64, Some(&json!(true))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Int64, None).unwrap();

        let arr = finish(b);
        let arr = arr.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(arr.value(0), 42);
        assert_eq!(arr.value(1), 3);
        assert_eq!(arr.value(2), 100);
        assert!(arr.is_null(3)); // fractional
        assert!(arr.is_null(4)); // unparseable string
        assert!(arr.is_null(5)); // bool — not an int
        assert!(arr.is_null(6)); // None
    }

    // --- Float64 column ---

    #[test]
    fn float_accepts_number_and_string_forms() {
        let mut b = make_builder(&DataType::Float64);
        append_json_cell("t", b.as_mut(), &DataType::Float64, Some(&json!(3.125))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Float64, Some(&json!(42))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Float64, Some(&json!("2.5"))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Float64, Some(&json!("NaN-ish"))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Float64, Some(&json!(null))).unwrap();

        let arr = finish(b);
        let arr = arr.as_any().downcast_ref::<Float64Array>().unwrap();
        assert!((arr.value(0) - 3.125).abs() < 1e-9);
        assert!((arr.value(1) - 42.0).abs() < 1e-9);
        assert!((arr.value(2) - 2.5).abs() < 1e-9);
        assert!(arr.is_null(3));
        assert!(arr.is_null(4));
    }

    // --- Utf8 fallback ---

    #[test]
    fn string_fallback_preserves_structural_values_as_json() {
        let mut b = make_builder(&DataType::Utf8);
        append_json_cell("t", b.as_mut(), &DataType::Utf8, Some(&json!("hello"))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Utf8, Some(&json!(true))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Utf8, Some(&json!(42))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Utf8, Some(&json!([1, 2]))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Utf8, Some(&json!({"k": "v"}))).unwrap();
        append_json_cell("t", b.as_mut(), &DataType::Utf8, None).unwrap();

        let arr = finish(b);
        let arr = arr.as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(arr.value(0), "hello");
        assert_eq!(arr.value(1), "true");
        assert_eq!(arr.value(2), "42");
        assert_eq!(arr.value(3), "[1,2]");
        assert_eq!(arr.value(4), r#"{"k":"v"}"#);
        assert!(arr.is_null(5));
    }

    #[test]
    fn builder_type_mismatch_surfaces_runtime_error() {
        let mut int_builder: Box<dyn ArrayBuilder> = Box::new(Int64Builder::new());
        let err = append_json_cell(
            "json",
            int_builder.as_mut(),
            &DataType::Boolean,
            Some(&json!(true)),
        )
        .expect_err("mismatched builder must error");
        assert!(
            matches!(err, OxError::Runtime { ref message } if message.contains("json") && message.contains("bool"))
        );
    }
}
