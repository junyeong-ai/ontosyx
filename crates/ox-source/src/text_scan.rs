//! Shared text-scan fallback for `DataSourceAdapter::scan` implementors.
//!
//! Several adapters (PostgreSQL, MySQL, BigQuery, inline CSV/JSON) cast
//! every projected column to a text representation on the source side,
//! then parse each cell back into a typed Arrow builder on the Rust
//! side. The parse logic is identical across dialects — every path we
//! support today renders bools as `t|true|1|yes` / `f|false|0|no`,
//! integers as base-10 digits, floats with a dot. This module owns
//! that shared logic.
//!
//! The name `text_scan` is deliberate: this is the **fallback** path.
//! Adapters that gain a typed extractor (typed sqlx decoder, a
//! typed BigQuery getter) should prefer that for precision-sensitive
//! Arrow types (Decimal, Timestamp) when they arrive; `text_scan`
//! keeps covering dialects / cell types that have no typed path.

use arrow::array::{
    ArrayBuilder, BooleanBuilder, Float64Builder, Int64Builder, StringBuilder,
};
use arrow::datatypes::DataType;

use ox_core::error::{OxError, OxResult};

/// Per-Arrow-type builder factory. Temporal / binary / unknown
/// types fall back to `StringBuilder` so DataFusion can cast at
/// query time — matches the Arrow schema produced by
/// `crate::normalize::describe_to_arrow_schema`.
pub fn make_builder(dt: &DataType) -> Box<dyn ArrayBuilder> {
    match dt {
        DataType::Boolean => Box::new(BooleanBuilder::new()),
        DataType::Int64 | DataType::Int32 | DataType::Int16 | DataType::Int8 => {
            Box::new(Int64Builder::new())
        }
        DataType::Float64 | DataType::Float32 => Box::new(Float64Builder::new()),
        _ => Box::new(StringBuilder::new()),
    }
}

/// Append a text cell to its builder by parsing the text
/// representation the source produced.
///
/// - `None` always appends a null.
/// - Parse failures on typed columns (Bool / Int / Float) also
///   append null, so a single dirty row doesn't poison the scan.
/// - Bool tokens accepted: `t|true|1|yes` → true,
///   `f|false|0|no` → false. Any other string on a Boolean column
///   appends null.
///
/// `adapter` is a short tag (`"postgres"`, `"mysql"`, …) that
/// appears in the error message if the builder / DataType pair
/// is inconsistent with what [`make_builder`] would have produced.
/// That is always a bug in the calling adapter — not a runtime
/// data problem — so the error is a `Runtime` variant, not a
/// validation failure.
pub fn append_text_cell(
    adapter: &str,
    builder: &mut dyn ArrayBuilder,
    dt: &DataType,
    value: Option<&str>,
) -> OxResult<()> {
    match dt {
        DataType::Boolean => {
            let b = builder
                .as_any_mut()
                .downcast_mut::<BooleanBuilder>()
                .ok_or_else(|| builder_mismatch(adapter, "bool"))?;
            match value {
                None => b.append_null(),
                Some(v) => match v.to_ascii_lowercase().as_str() {
                    "t" | "true" | "1" | "yes" => b.append_value(true),
                    "f" | "false" | "0" | "no" => b.append_value(false),
                    _ => b.append_null(),
                },
            }
        }
        DataType::Int64 | DataType::Int32 | DataType::Int16 | DataType::Int8 => {
            let b = builder
                .as_any_mut()
                .downcast_mut::<Int64Builder>()
                .ok_or_else(|| builder_mismatch(adapter, "int"))?;
            match value.and_then(|v| v.parse::<i64>().ok()) {
                Some(n) => b.append_value(n),
                None => b.append_null(),
            }
        }
        DataType::Float64 | DataType::Float32 => {
            let b = builder
                .as_any_mut()
                .downcast_mut::<Float64Builder>()
                .ok_or_else(|| builder_mismatch(adapter, "float"))?;
            match value.and_then(|v| v.parse::<f64>().ok()) {
                Some(n) => b.append_value(n),
                None => b.append_null(),
            }
        }
        _ => {
            let b = builder
                .as_any_mut()
                .downcast_mut::<StringBuilder>()
                .ok_or_else(|| builder_mismatch(adapter, "string"))?;
            match value {
                None => b.append_null(),
                Some(v) => b.append_value(v),
            }
        }
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
    use arrow::array::{Array, BooleanArray, Float64Array, Int64Array, StringArray};

    fn finish(mut b: Box<dyn ArrayBuilder>) -> arrow::array::ArrayRef {
        b.finish()
    }

    #[test]
    fn bool_tokens_cover_every_accepted_shape() {
        let mut b = make_builder(&DataType::Boolean);
        for tok in &["t", "TRUE", "1", "Yes"] {
            append_text_cell("test", b.as_mut(), &DataType::Boolean, Some(tok)).unwrap();
        }
        for tok in &["F", "false", "0", "no"] {
            append_text_cell("test", b.as_mut(), &DataType::Boolean, Some(tok)).unwrap();
        }
        append_text_cell("test", b.as_mut(), &DataType::Boolean, Some("garbage")).unwrap();
        append_text_cell("test", b.as_mut(), &DataType::Boolean, None).unwrap();

        let arr = finish(b);
        let arr = arr.as_any().downcast_ref::<BooleanArray>().unwrap();
        assert_eq!(arr.len(), 10);
        for i in 0..4 {
            assert!(arr.is_valid(i) && arr.value(i));
        }
        for i in 4..8 {
            assert!(arr.is_valid(i) && !arr.value(i));
        }
        assert!(arr.is_null(8));
        assert!(arr.is_null(9));
    }

    #[test]
    fn int_parse_failure_appends_null() {
        let mut b = make_builder(&DataType::Int64);
        append_text_cell("test", b.as_mut(), &DataType::Int64, Some("42")).unwrap();
        append_text_cell("test", b.as_mut(), &DataType::Int64, Some("not-a-number"))
            .unwrap();
        append_text_cell("test", b.as_mut(), &DataType::Int64, None).unwrap();

        let arr = finish(b);
        let arr = arr.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(arr.value(0), 42);
        assert!(arr.is_null(1));
        assert!(arr.is_null(2));
    }

    #[test]
    fn float_parse_failure_appends_null() {
        let mut b = make_builder(&DataType::Float64);
        append_text_cell("test", b.as_mut(), &DataType::Float64, Some("3.125")).unwrap();
        append_text_cell("test", b.as_mut(), &DataType::Float64, Some("NaN-ish"))
            .unwrap();

        let arr = finish(b);
        let arr = arr.as_any().downcast_ref::<Float64Array>().unwrap();
        assert!((arr.value(0) - 3.125).abs() < 1e-9);
        assert!(arr.is_null(1));
    }

    #[test]
    fn string_fallback_holds_raw_text() {
        let mut b = make_builder(&DataType::Utf8);
        append_text_cell("test", b.as_mut(), &DataType::Utf8, Some("hello")).unwrap();
        append_text_cell("test", b.as_mut(), &DataType::Utf8, None).unwrap();

        let arr = finish(b);
        let arr = arr.as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(arr.value(0), "hello");
        assert!(arr.is_null(1));
    }

    #[test]
    fn builder_type_mismatch_surfaces_runtime_error() {
        // Force the wrong builder for a Bool DataType to exercise the
        // mismatch error path — a misuse bug from a caller, not a
        // data problem.
        let mut int_builder: Box<dyn ArrayBuilder> = Box::new(Int64Builder::new());
        let err = append_text_cell(
            "postgres",
            int_builder.as_mut(),
            &DataType::Boolean,
            Some("true"),
        )
        .expect_err("mismatched builder must error");
        assert!(matches!(err, OxError::Runtime { ref message } if message.contains("postgres") && message.contains("bool")));
    }
}
