//! Arrow `RecordBatch` → `QueryResult` conversion.
//!
//! The federation execute path
//! ([`ox_federation::FederationContext::execute_plan`]) returns
//! `Vec<RecordBatch>`. The HTTP response format shared with the
//! Cypher path is `QueryResult { columns, rows: Vec<Vec<PropertyValue>> }`,
//! so before the handler can reply it must project every Arrow cell
//! into the matching [`PropertyValue`] variant. This module owns
//! that translation in one place so both call-sites (the new
//! `/api/query/from-ir/federation` handler and, eventually, a
//! streaming / chunked variant) share the type-mapping rules.
//!
//! Scope covers every commonly-emitted Arrow type: primitive
//! integers, floats, booleans, UTF-8 strings, dates, timestamps,
//! the three Binary kinds (Binary, LargeBinary, FixedSizeBinary),
//! the two List kinds (List, LargeList — recursion on the inner
//! element type), Map (Utf8 keys + recursive values), Struct
//! (flattened into `PropertyValue::Map` keyed by field name),
//! Decimal128/256 (**lossy** — materialised as
//! `PropertyValue::Float` via `raw / 10^scale`; values beyond
//! `f64`'s 2^53 exact band lose low-order digits), and Dictionary
//! (integer-keyed: the row's key indexes into the underlying
//! values array and recursion decodes whichever type that is).
//! Non-integer Dictionary key types are refused with an explicit
//! error; all non-scalar encodings DataFusion can produce are
//! otherwise supported.
//!
//! The error type is a plain `String` because every caller wraps
//! it in an `AppError::unprocessable(...)` at the handler boundary;
//! wrapping in `OxError` here would force an extra conversion with
//! no added diagnostic value.

use arrow::array::{
    Array, BinaryArray, BooleanArray, Date32Array, Date64Array, Decimal128Array, Decimal256Array,
    DictionaryArray, FixedSizeBinaryArray, Float32Array, Float64Array, Int8Array, Int16Array,
    Int32Array, Int64Array, LargeBinaryArray, LargeListArray, LargeStringArray, ListArray,
    MapArray, StringArray, StructArray, TimestampMicrosecondArray, TimestampMillisecondArray,
    TimestampNanosecondArray, TimestampSecondArray, UInt8Array, UInt16Array, UInt32Array,
    UInt64Array,
};
use arrow::datatypes::{
    Int16Type, Int32Type, Int64Type, Int8Type, UInt16Type, UInt32Type, UInt64Type, UInt8Type,
};
use arrow::datatypes::{DataType, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Duration as ChronoDuration, NaiveDate};

use ox_core::types::PropertyValue;
use ox_query_ir::query::{QueryMetadata, QueryResult};

/// Translate a sequence of Arrow `RecordBatch`es (the shape
/// `FederationContext::execute_plan` returns) into the `QueryResult`
/// the HTTP response serialises.
///
/// An empty input produces an empty result with zero columns and
/// zero rows; the caller is responsible for deciding whether that
/// is a "no rows" success or a policy-level failure.
pub fn record_batches_to_query_result(
    batches: &[RecordBatch],
    execution_time_ms: u64,
) -> Result<QueryResult, String> {
    let (columns, rows) = match batches.first() {
        None => (Vec::new(), Vec::new()),
        Some(first) => {
            let columns: Vec<String> = first
                .schema()
                .fields()
                .iter()
                .map(|f| f.name().clone())
                .collect();
            let mut rows: Vec<Vec<PropertyValue>> =
                Vec::with_capacity(batches.iter().map(|b| b.num_rows()).sum());
            for batch in batches {
                append_batch_rows(batch, &mut rows)?;
            }
            (columns, rows)
        }
    };
    let rows_returned = rows.len();
    Ok(QueryResult {
        columns,
        rows,
        metadata: QueryMetadata {
            execution_time_ms,
            rows_returned,
            nodes_affected: None,
            edges_affected: None,
            provenance: None, warnings: Vec::new(),
        },
    })
}

fn append_batch_rows(
    batch: &RecordBatch,
    out: &mut Vec<Vec<PropertyValue>>,
) -> Result<(), String> {
    let num_rows = batch.num_rows();
    let num_cols = batch.num_columns();
    for row_idx in 0..num_rows {
        let mut row: Vec<PropertyValue> = Vec::with_capacity(num_cols);
        for col_idx in 0..num_cols {
            let array = batch.column(col_idx);
            row.push(cell_to_property(array.as_ref(), row_idx)?);
        }
        out.push(row);
    }
    Ok(())
}

fn cell_to_property(array: &dyn Array, row: usize) -> Result<PropertyValue, String> {
    if array.is_null(row) {
        return Ok(PropertyValue::Null);
    }
    match array.data_type() {
        DataType::Boolean => {
            let a = downcast::<BooleanArray>(array)?;
            Ok(PropertyValue::Bool(a.value(row)))
        }
        DataType::Int8 => {
            let a = downcast::<Int8Array>(array)?;
            Ok(PropertyValue::Int(a.value(row) as i64))
        }
        DataType::Int16 => {
            let a = downcast::<Int16Array>(array)?;
            Ok(PropertyValue::Int(a.value(row) as i64))
        }
        DataType::Int32 => {
            let a = downcast::<Int32Array>(array)?;
            Ok(PropertyValue::Int(a.value(row) as i64))
        }
        DataType::Int64 => {
            let a = downcast::<Int64Array>(array)?;
            Ok(PropertyValue::Int(a.value(row)))
        }
        DataType::UInt8 => {
            let a = downcast::<UInt8Array>(array)?;
            Ok(PropertyValue::Int(a.value(row) as i64))
        }
        DataType::UInt16 => {
            let a = downcast::<UInt16Array>(array)?;
            Ok(PropertyValue::Int(a.value(row) as i64))
        }
        DataType::UInt32 => {
            let a = downcast::<UInt32Array>(array)?;
            Ok(PropertyValue::Int(a.value(row) as i64))
        }
        DataType::UInt64 => {
            let a = downcast::<UInt64Array>(array)?;
            let v = a.value(row);
            // u64 → i64 can overflow; reject the narrow edge case
            // with an explicit error rather than silently wrapping.
            i64::try_from(v)
                .map(PropertyValue::Int)
                .map_err(|_| format!("UInt64 value {v} exceeds i64 range"))
        }
        DataType::Float32 => {
            let a = downcast::<Float32Array>(array)?;
            Ok(PropertyValue::Float(a.value(row) as f64))
        }
        DataType::Float64 => {
            let a = downcast::<Float64Array>(array)?;
            Ok(PropertyValue::Float(a.value(row)))
        }
        DataType::Utf8 => {
            let a = downcast::<StringArray>(array)?;
            Ok(PropertyValue::String(a.value(row).to_string()))
        }
        DataType::LargeUtf8 => {
            let a = downcast::<LargeStringArray>(array)?;
            Ok(PropertyValue::String(a.value(row).to_string()))
        }
        DataType::Date32 => {
            let a = downcast::<Date32Array>(array)?;
            let days = a.value(row);
            date_from_days(days as i64).map(PropertyValue::Date)
        }
        DataType::Date64 => {
            let a = downcast::<Date64Array>(array)?;
            let millis = a.value(row);
            let days = millis / 86_400_000;
            date_from_days(days).map(PropertyValue::Date)
        }
        DataType::Timestamp(unit, _) => timestamp_to_property(array, row, *unit),
        DataType::Binary => {
            let a = downcast::<BinaryArray>(array)?;
            Ok(PropertyValue::Bytes(a.value(row).to_vec()))
        }
        DataType::LargeBinary => {
            let a = downcast::<LargeBinaryArray>(array)?;
            Ok(PropertyValue::Bytes(a.value(row).to_vec()))
        }
        DataType::FixedSizeBinary(_) => {
            let a = downcast::<FixedSizeBinaryArray>(array)?;
            Ok(PropertyValue::Bytes(a.value(row).to_vec()))
        }
        DataType::List(_) => {
            let a = downcast::<ListArray>(array)?;
            list_to_property(a.value(row).as_ref())
        }
        DataType::LargeList(_) => {
            let a = downcast::<LargeListArray>(array)?;
            list_to_property(a.value(row).as_ref())
        }
        DataType::Map(_, _) => {
            let a = downcast::<MapArray>(array)?;
            map_to_property(a, row)
        }
        DataType::Struct(fields) => {
            let a = downcast::<StructArray>(array)?;
            let mut entries: std::collections::HashMap<String, PropertyValue> =
                std::collections::HashMap::with_capacity(fields.len());
            for (idx, field) in fields.iter().enumerate() {
                let col = a.column(idx);
                let v = cell_to_property(col.as_ref(), row)?;
                entries.insert(field.name().clone(), v);
            }
            Ok(PropertyValue::Map(entries))
        }
        // Decimal conversion is **lossy**: `raw / 10^scale` is
        // materialised as an `f64`, so values whose absolute
        // magnitude exceeds the ~2^53 exact-representation band
        // lose low-order digits. Callers that need exact precision
        // (accounting, cryptographic nonces) should either swap in
        // a `PropertyValue::String` decimal representation via a
        // future variant, or project the column on the source side
        // before it reaches the federation layer.
        DataType::Decimal128(_, scale) => {
            let a = downcast::<Decimal128Array>(array)?;
            let raw = a.value(row);
            Ok(PropertyValue::Float(raw as f64 / 10f64.powi(*scale as i32)))
        }
        DataType::Decimal256(_, scale) => {
            let a = downcast::<Decimal256Array>(array)?;
            let raw = a.value(row);
            // `to_i128` refuses values outside i128 range — better
            // to surface the lossy-wrap case as an explicit error
            // than to silently wrap.
            let as_i128 = raw.to_i128().ok_or_else(|| {
                format!(
                    "federation result: Decimal256 value at row {row} exceeds \
                     i128 range; lossless conversion to PropertyValue needs a \
                     String-based variant that has not been added yet"
                )
            })?;
            Ok(PropertyValue::Float(as_i128 as f64 / 10f64.powi(*scale as i32)))
        }
        // Dictionary encoding: pick the row's key, then recurse
        // into the underlying `values()` array at that index.
        // Supports the eight integer key types Arrow defines;
        // Float / Binary / Utf8 keys are rejected because the
        // standard analytics / DataFusion emit surface only uses
        // integer keys.
        DataType::Dictionary(key_type, _) => match key_type.as_ref() {
            DataType::Int8 => decode_dictionary::<Int8Type>(array, row),
            DataType::Int16 => decode_dictionary::<Int16Type>(array, row),
            DataType::Int32 => decode_dictionary::<Int32Type>(array, row),
            DataType::Int64 => decode_dictionary::<Int64Type>(array, row),
            DataType::UInt8 => decode_dictionary::<UInt8Type>(array, row),
            DataType::UInt16 => decode_dictionary::<UInt16Type>(array, row),
            DataType::UInt32 => decode_dictionary::<UInt32Type>(array, row),
            DataType::UInt64 => decode_dictionary::<UInt64Type>(array, row),
            other => Err(format!(
                "federation result: Dictionary key type {other} is not supported; \
                 only integer key types can be decoded"
            )),
        },
        other => Err(format!(
            "federation result: Arrow type {other} has no PropertyValue mapping yet"
        )),
    }
}

/// Decode one row of a typed `DictionaryArray` into a
/// `PropertyValue` by delegating to the underlying values array
/// at the row's key-index.
fn decode_dictionary<K>(array: &dyn Array, row: usize) -> Result<PropertyValue, String>
where
    K: arrow::datatypes::ArrowDictionaryKeyType,
    K::Native: TryInto<usize>,
    <K::Native as TryInto<usize>>::Error: std::fmt::Display,
{
    let a = array
        .as_any()
        .downcast_ref::<DictionaryArray<K>>()
        .ok_or_else(|| {
            format!(
                "federation result: Dictionary downcast failed — expected key \
                 type {}",
                std::any::type_name::<K>()
            )
        })?;
    let key = a.keys().value(row);
    let idx: usize = key
        .try_into()
        .map_err(|e| format!("federation result: negative dictionary key {e}"))?;
    cell_to_property(a.values().as_ref(), idx)
}

/// Flatten a single MapArray row into a `PropertyValue::Map`.
///
/// Arrow's MapArray keeps all rows' keys + values in two long
/// `ArrayRef`s plus an offsets buffer that slices the per-row
/// ranges. We read the `[start..end)` slice for the row and
/// convert each (key, value) pair.
///
/// `PropertyValue::Map` requires `String` keys, so non-Utf8 key
/// types refuse with a descriptive error. Value types recurse
/// through `cell_to_property`, so e.g. a `Map<Utf8, List<Int64>>`
/// round-trips cleanly.
fn map_to_property(a: &MapArray, row: usize) -> Result<PropertyValue, String> {
    let offsets = a.offsets();
    // `offsets` dereferences to `&[i32]`; cast to usize via `as` —
    // arrow-buffer's `ArrowNativeType::as_usize` is not in
    // ox-api's dep surface, and i32 → usize is always safe on the
    // 64-bit platforms the workspace supports.
    let start = offsets[row] as usize;
    let end = offsets[row + 1] as usize;

    let keys = a.keys();
    let values = a.values();

    let keys_str = keys.as_any().downcast_ref::<StringArray>().ok_or_else(|| {
        format!(
            "federation result: Map key type {:?} is not Utf8; \
             PropertyValue::Map requires String keys",
            keys.data_type()
        )
    })?;

    let mut entries: std::collections::HashMap<String, PropertyValue> =
        std::collections::HashMap::with_capacity(end - start);
    for i in start..end {
        // A Null key is a legitimate Arrow encoding but has no
        // HashMap slot. Treat it as a refusal so the caller sees
        // the lossy event rather than silently dropping the pair.
        if keys_str.is_null(i) {
            return Err(format!(
                "federation result: Map row {row} carries a null key at \
                 offset {i}; PropertyValue::Map has no representation for that"
            ));
        }
        let k = keys_str.value(i).to_string();
        let v = cell_to_property(values.as_ref(), i)?;
        entries.insert(k, v);
    }
    Ok(PropertyValue::Map(entries))
}

/// Convert every row of a sublist array (the inner array a
/// `ListArray` / `LargeListArray` holds per outer row) into a
/// single `PropertyValue::List`. Recursion bottoms out on the
/// inner array's element type; nested lists of lists work because
/// each recursive call routes back through `cell_to_property`.
fn list_to_property(sublist: &dyn Array) -> Result<PropertyValue, String> {
    let len = sublist.len();
    let mut items: Vec<PropertyValue> = Vec::with_capacity(len);
    for i in 0..len {
        items.push(cell_to_property(sublist, i)?);
    }
    Ok(PropertyValue::List(items))
}

fn timestamp_to_property(
    array: &dyn Array,
    row: usize,
    unit: TimeUnit,
) -> Result<PropertyValue, String> {
    let dt = match unit {
        TimeUnit::Second => {
            let a = downcast::<TimestampSecondArray>(array)?;
            DateTime::from_timestamp(a.value(row), 0)
        }
        TimeUnit::Millisecond => {
            let a = downcast::<TimestampMillisecondArray>(array)?;
            let millis = a.value(row);
            let secs = millis.div_euclid(1_000);
            let rem_nanos = (millis.rem_euclid(1_000) as u32) * 1_000_000;
            DateTime::from_timestamp(secs, rem_nanos)
        }
        TimeUnit::Microsecond => {
            let a = downcast::<TimestampMicrosecondArray>(array)?;
            let micros = a.value(row);
            let secs = micros.div_euclid(1_000_000);
            let rem_nanos = (micros.rem_euclid(1_000_000) as u32) * 1_000;
            DateTime::from_timestamp(secs, rem_nanos)
        }
        TimeUnit::Nanosecond => {
            let a = downcast::<TimestampNanosecondArray>(array)?;
            let nanos = a.value(row);
            let secs = nanos.div_euclid(1_000_000_000);
            let rem_nanos = nanos.rem_euclid(1_000_000_000) as u32;
            DateTime::from_timestamp(secs, rem_nanos)
        }
    };
    match dt {
        Some(d) => Ok(PropertyValue::DateTime(d.naive_utc())),
        None => Err(format!("federation result: Timestamp value out of range (unit={unit:?})")),
    }
}

fn date_from_days(days: i64) -> Result<NaiveDate, String> {
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1)
        .ok_or_else(|| "internal: epoch date 1970-01-01 is not representable".to_string())?;
    epoch
        .checked_add_signed(ChronoDuration::days(days))
        .ok_or_else(|| format!("federation result: Date value out of chrono range ({days} days)"))
}

fn downcast<T: Array + 'static>(array: &dyn Array) -> Result<&T, String> {
    array.as_any().downcast_ref::<T>().ok_or_else(|| {
        format!(
            "federation result: Arrow array type mismatch — expected {}, got {}",
            std::any::type_name::<T>(),
            array.data_type()
        )
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use arrow::array::{
        BinaryArray, BooleanArray, Date32Array, Date64Array, Decimal128Array, DictionaryArray,
        FixedSizeBinaryArray, Float32Array, Float64Array, Int32Array, Int64Array, Int64Builder,
        LargeBinaryArray, LargeListBuilder, ListBuilder, MapBuilder, StringArray, StringBuilder,
        StructArray, TimestampMicrosecondArray, TimestampMillisecondArray,
        TimestampNanosecondArray, TimestampSecondArray, UInt64Array,
    };
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use arrow::record_batch::RecordBatch;

    use super::*;

    fn schema(fields: Vec<(&str, DataType)>) -> Arc<Schema> {
        Arc::new(Schema::new(
            fields
                .into_iter()
                .map(|(n, t)| Field::new(n, t, true))
                .collect::<Vec<_>>(),
        ))
    }

    #[test]
    fn empty_batches_produce_empty_result() {
        let result = record_batches_to_query_result(&[], 0).unwrap();
        assert!(result.columns.is_empty());
        assert!(result.rows.is_empty());
        assert_eq!(result.metadata.rows_returned, 0);
    }

    #[test]
    fn primitive_types_map_to_matching_property_variants() {
        let schema = schema(vec![
            ("id", DataType::Int64),
            ("name", DataType::Utf8),
            ("amount", DataType::Float64),
            ("active", DataType::Boolean),
        ]);
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from(vec![1, 2])),
                Arc::new(StringArray::from(vec!["Alice", "Bob"])),
                Arc::new(Float64Array::from(vec![100.5, 42.0])),
                Arc::new(BooleanArray::from(vec![true, false])),
            ],
        )
        .unwrap();

        let result = record_batches_to_query_result(&[batch], 12).unwrap();
        assert_eq!(result.columns, vec!["id", "name", "amount", "active"]);
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0][0], PropertyValue::Int(1));
        assert_eq!(result.rows[0][1], PropertyValue::String("Alice".into()));
        assert_eq!(result.rows[0][2], PropertyValue::Float(100.5));
        assert_eq!(result.rows[0][3], PropertyValue::Bool(true));
        assert_eq!(result.metadata.rows_returned, 2);
        assert_eq!(result.metadata.execution_time_ms, 12);
    }

    #[test]
    fn null_cells_surface_as_property_null() {
        let schema = schema(vec![("maybe_id", DataType::Int64)]);
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(Int64Array::from(vec![Some(5), None]))],
        )
        .unwrap();
        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.rows[0][0], PropertyValue::Int(5));
        assert_eq!(result.rows[1][0], PropertyValue::Null);
    }

    #[test]
    fn date32_rounds_trip_as_naive_date() {
        let schema = schema(vec![("d", DataType::Date32)]);
        // 2026-04-20 → days since epoch 1970-01-01.
        let target = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
        let days = (target - NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days() as i32;
        let batch =
            RecordBatch::try_new(schema, vec![Arc::new(Date32Array::from(vec![days]))]).unwrap();
        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.rows[0][0], PropertyValue::Date(target));
    }

    #[test]
    fn date64_rounds_trip_as_naive_date() {
        // Date64 is milliseconds-since-epoch, not days — the
        // conversion path divides by 86_400_000 before indexing
        // into the NaiveDate calendar. A future refactor that
        // forgets to unit-convert would land us off by ~11,574×.
        let schema = schema(vec![("d", DataType::Date64)]);
        let target = NaiveDate::from_ymd_opt(2026, 4, 21).unwrap();
        let days = (target - NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days();
        let millis = days * 86_400_000i64;
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(Date64Array::from(vec![millis]))],
        )
        .unwrap();
        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.rows[0][0], PropertyValue::Date(target));
    }

    #[test]
    fn timestamp_microsecond_rounds_trip_as_naive_datetime() {
        let schema = schema(vec![(
            "ts",
            DataType::Timestamp(TimeUnit::Microsecond, None),
        )]);
        // 2026-04-20 12:34:56.123456 UTC
        let target = NaiveDate::from_ymd_opt(2026, 4, 20)
            .unwrap()
            .and_hms_micro_opt(12, 34, 56, 123_456)
            .unwrap();
        let micros = target.and_utc().timestamp_micros();
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(TimestampMicrosecondArray::from(vec![micros]))],
        )
        .unwrap();
        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.rows[0][0], PropertyValue::DateTime(target));
    }

    #[test]
    fn timestamp_second_rounds_trip_as_naive_datetime() {
        let schema = schema(vec![(
            "ts",
            DataType::Timestamp(TimeUnit::Second, None),
        )]);
        let target = NaiveDate::from_ymd_opt(2026, 4, 21)
            .unwrap()
            .and_hms_opt(9, 15, 30)
            .unwrap();
        let secs = target.and_utc().timestamp();
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(TimestampSecondArray::from(vec![secs]))],
        )
        .unwrap();
        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.rows[0][0], PropertyValue::DateTime(target));
    }

    #[test]
    fn timestamp_millisecond_rounds_trip_as_naive_datetime() {
        let schema = schema(vec![(
            "ts",
            DataType::Timestamp(TimeUnit::Millisecond, None),
        )]);
        let target = NaiveDate::from_ymd_opt(2026, 4, 21)
            .unwrap()
            .and_hms_milli_opt(9, 15, 30, 250)
            .unwrap();
        let millis = target.and_utc().timestamp_millis();
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(TimestampMillisecondArray::from(vec![millis]))],
        )
        .unwrap();
        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.rows[0][0], PropertyValue::DateTime(target));
    }

    #[test]
    fn timestamp_nanosecond_rounds_trip_as_naive_datetime() {
        let schema = schema(vec![(
            "ts",
            DataType::Timestamp(TimeUnit::Nanosecond, None),
        )]);
        let target = NaiveDate::from_ymd_opt(2026, 4, 21)
            .unwrap()
            .and_hms_nano_opt(9, 15, 30, 123_456_789)
            .unwrap();
        let nanos = target.and_utc().timestamp_nanos_opt().unwrap();
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(TimestampNanosecondArray::from(vec![nanos]))],
        )
        .unwrap();
        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.rows[0][0], PropertyValue::DateTime(target));
    }

    #[test]
    fn rows_returned_sums_across_multiple_batches() {
        let schema = schema(vec![("id", DataType::Int64)]);
        let b1 = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![1, 2, 3]))],
        )
        .unwrap();
        let b2 = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![4, 5]))],
        )
        .unwrap();
        let result = record_batches_to_query_result(&[b1, b2], 0).unwrap();
        assert_eq!(result.metadata.rows_returned, 5);
        assert_eq!(result.rows.len(), 5);
    }

    #[test]
    fn binary_arrays_map_to_property_bytes() {
        // Cover the three Binary variants. FixedSizeBinary has a
        // per-row size constraint (every cell must be the declared
        // width); the others take arbitrary-length blobs.
        let schema = Arc::new(Schema::new(vec![
            Field::new("blob_var", DataType::Binary, true),
            Field::new("blob_big", DataType::LargeBinary, true),
            Field::new("blob_fix", DataType::FixedSizeBinary(3), true),
        ]));
        let var = BinaryArray::from(vec![b"hello".as_slice(), b"".as_slice()]);
        let big = LargeBinaryArray::from(vec![b"world".as_slice(), b"!!".as_slice()]);
        let fix = FixedSizeBinaryArray::try_from_iter(
            vec![b"abc".as_slice(), b"xyz".as_slice()].into_iter(),
        )
        .unwrap();
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(var), Arc::new(big), Arc::new(fix)],
        )
        .unwrap();

        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.metadata.rows_returned, 2);
        assert_eq!(
            result.rows[0][0],
            PropertyValue::Bytes(b"hello".to_vec())
        );
        assert_eq!(result.rows[0][1], PropertyValue::Bytes(b"world".to_vec()));
        assert_eq!(result.rows[0][2], PropertyValue::Bytes(b"abc".to_vec()));
        // Empty Binary cells are still `Bytes`, not `Null` — the
        // cell is logically present and carries a zero-length blob.
        assert_eq!(result.rows[1][0], PropertyValue::Bytes(Vec::new()));
        assert_eq!(result.rows[1][1], PropertyValue::Bytes(b"!!".to_vec()));
        assert_eq!(result.rows[1][2], PropertyValue::Bytes(b"xyz".to_vec()));
    }

    #[test]
    fn list_array_maps_to_property_list_recursively() {
        // Build a ListArray<Int64> with three rows:
        //   [10, 20, 30]
        //   []            (empty sublist — still a List, not Null)
        //   [40, 50]
        let mut builder = ListBuilder::new(Int64Builder::new());
        builder.values().append_value(10);
        builder.values().append_value(20);
        builder.values().append_value(30);
        builder.append(true);
        builder.append(true); // empty sublist
        builder.values().append_value(40);
        builder.values().append_value(50);
        builder.append(true);
        let list = builder.finish();

        let schema = Arc::new(Schema::new(vec![Field::new(
            "items",
            list.data_type().clone(),
            true,
        )]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(list)]).unwrap();

        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.metadata.rows_returned, 3);
        assert_eq!(
            result.rows[0][0],
            PropertyValue::List(vec![
                PropertyValue::Int(10),
                PropertyValue::Int(20),
                PropertyValue::Int(30),
            ])
        );
        assert_eq!(
            result.rows[1][0],
            PropertyValue::List(Vec::new()),
            "empty sublist is still a List(vec![]), distinct from Null"
        );
        assert_eq!(
            result.rows[2][0],
            PropertyValue::List(vec![PropertyValue::Int(40), PropertyValue::Int(50),])
        );
    }

    #[test]
    fn large_list_array_maps_to_property_list() {
        // LargeList uses i64 offsets (ListArray uses i32) — distinct
        // downcast path in `cell_to_property`. Build a two-row list
        // via LargeListBuilder to exercise it.
        let mut builder = LargeListBuilder::new(Int64Builder::new());
        // Row 0: [7, 8, 9]
        builder.values().append_value(7);
        builder.values().append_value(8);
        builder.values().append_value(9);
        builder.append(true);
        // Row 1: []
        builder.append(true);
        let list = builder.finish();

        let schema = Arc::new(Schema::new(vec![Field::new(
            "many",
            list.data_type().clone(),
            true,
        )]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(list)]).unwrap();
        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.metadata.rows_returned, 2);
        assert_eq!(
            result.rows[0][0],
            PropertyValue::List(vec![
                PropertyValue::Int(7),
                PropertyValue::Int(8),
                PropertyValue::Int(9),
            ])
        );
        assert_eq!(result.rows[1][0], PropertyValue::List(Vec::new()));
    }

    #[test]
    fn nested_list_recurses_into_inner_list_type() {
        // List<List<Int64>> — proves `list_to_property` actually
        // recurses instead of stopping at the first level. Row 0 is
        // `[[1, 2], [3]]`; row 1 is a single empty inner list
        // `[[]]`. Expected shape after conversion mirrors that
        // structure exactly.
        let mut outer = ListBuilder::new(ListBuilder::new(Int64Builder::new()));
        // Row 0: [[1, 2], [3]]
        outer.values().values().append_value(1);
        outer.values().values().append_value(2);
        outer.values().append(true); // close [1, 2]
        outer.values().values().append_value(3);
        outer.values().append(true); // close [3]
        outer.append(true); // close [[1,2], [3]]
        // Row 1: [[]]  — one inner list, and that inner list is empty.
        outer.values().append(true); // close the empty inner list
        outer.append(true);
        let list = outer.finish();

        let schema = Arc::new(Schema::new(vec![Field::new(
            "nested",
            list.data_type().clone(),
            true,
        )]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(list)]).unwrap();
        let result = record_batches_to_query_result(&[batch], 0).unwrap();

        assert_eq!(result.metadata.rows_returned, 2);
        assert_eq!(
            result.rows[0][0],
            PropertyValue::List(vec![
                PropertyValue::List(vec![
                    PropertyValue::Int(1),
                    PropertyValue::Int(2),
                ]),
                PropertyValue::List(vec![PropertyValue::Int(3)]),
            ])
        );
        assert_eq!(
            result.rows[1][0],
            PropertyValue::List(vec![PropertyValue::List(Vec::new())])
        );
    }

    #[test]
    fn map_array_materialises_as_property_map_with_recursive_values() {
        // Build a Map<Utf8, Int64> with two rows:
        //   row 0: {"a": 1, "b": 2}
        //   row 1: {} (empty map — still a Map, not Null)
        let mut builder =
            MapBuilder::new(None, StringBuilder::new(), Int64Builder::new());
        // Row 0
        builder.keys().append_value("a");
        builder.values().append_value(1);
        builder.keys().append_value("b");
        builder.values().append_value(2);
        builder.append(true).unwrap();
        // Row 1 — empty
        builder.append(true).unwrap();
        let map = builder.finish();

        let schema = Arc::new(Schema::new(vec![Field::new(
            "attributes",
            map.data_type().clone(),
            true,
        )]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(map)]).unwrap();

        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.metadata.rows_returned, 2);
        match &result.rows[0][0] {
            PropertyValue::Map(entries) => {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries.get("a"), Some(&PropertyValue::Int(1)));
                assert_eq!(entries.get("b"), Some(&PropertyValue::Int(2)));
            }
            other => panic!("expected Map, got {other:?}"),
        }
        match &result.rows[1][0] {
            PropertyValue::Map(entries) => {
                assert!(
                    entries.is_empty(),
                    "empty map round-trips as PropertyValue::Map(empty), not Null"
                );
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }

    #[test]
    fn struct_array_flattens_into_property_map_keyed_by_field_name() {
        // A two-field struct: `(id: Int64, name: Utf8)`. Each row
        // becomes a `PropertyValue::Map` with the field names as
        // keys. Value conversion recurses through `cell_to_property`
        // so e.g. a Struct of Struct works without extra code.
        let id = Int64Array::from(vec![1, 2]);
        let name = StringArray::from(vec!["Alice", "Bob"]);
        let struct_arr = StructArray::from(vec![
            (
                Arc::new(Field::new("id", DataType::Int64, true)),
                Arc::new(id) as _,
            ),
            (
                Arc::new(Field::new("name", DataType::Utf8, true)),
                Arc::new(name) as _,
            ),
        ]);

        let schema = Arc::new(Schema::new(vec![Field::new(
            "person",
            struct_arr.data_type().clone(),
            true,
        )]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(struct_arr)]).unwrap();

        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.metadata.rows_returned, 2);

        match &result.rows[0][0] {
            PropertyValue::Map(entries) => {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries.get("id"), Some(&PropertyValue::Int(1)));
                assert_eq!(
                    entries.get("name"),
                    Some(&PropertyValue::String("Alice".into()))
                );
            }
            other => panic!("expected Map, got {other:?}"),
        }
        match &result.rows[1][0] {
            PropertyValue::Map(entries) => {
                assert_eq!(entries.get("id"), Some(&PropertyValue::Int(2)));
                assert_eq!(
                    entries.get("name"),
                    Some(&PropertyValue::String("Bob".into()))
                );
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }

    #[test]
    fn decimal128_divides_by_scale_into_property_float() {
        // precision=10, scale=2 — raw integer values divided by 100.
        // 12345 → 123.45, 100 → 1.00, -75 → -0.75. Covers sign and
        // the "integer-y" value that shows the divide isn't skipped.
        let arr = Decimal128Array::from(vec![12345i128, 100, -75])
            .with_precision_and_scale(10, 2)
            .unwrap();
        let schema = Arc::new(Schema::new(vec![Field::new(
            "amount",
            arr.data_type().clone(),
            true,
        )]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(arr)]).unwrap();

        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.metadata.rows_returned, 3);

        // Use tolerant float comparison — the conversion is lossy
        // by design; at scale 2 with small magnitudes there should
        // still be no observable rounding.
        match &result.rows[0][0] {
            PropertyValue::Float(v) => assert!((v - 123.45).abs() < 1e-9),
            other => panic!("expected Float, got {other:?}"),
        }
        match &result.rows[1][0] {
            PropertyValue::Float(v) => assert!((v - 1.00).abs() < 1e-9),
            other => panic!("expected Float, got {other:?}"),
        }
        match &result.rows[2][0] {
            PropertyValue::Float(v) => assert!((v - -0.75).abs() < 1e-9),
            other => panic!("expected Float, got {other:?}"),
        }
    }

    #[test]
    fn nested_struct_flattens_recursively_into_property_map() {
        // Outer struct: `(id: Int64, inner: Struct<name, score>)`.
        // The inner Struct should land as a nested `PropertyValue::Map`
        // inside the outer Map — same recursion contract the flat
        // Struct test already covers, but exercised at depth two.
        let inner_name = StringArray::from(vec!["Alice", "Bob"]);
        let inner_score = Float64Array::from(vec![95.5, 42.0]);
        let inner_struct = StructArray::from(vec![
            (
                Arc::new(Field::new("name", DataType::Utf8, true)),
                Arc::new(inner_name) as _,
            ),
            (
                Arc::new(Field::new("score", DataType::Float64, true)),
                Arc::new(inner_score) as _,
            ),
        ]);
        let outer_id = Int64Array::from(vec![1, 2]);
        let outer_struct = StructArray::from(vec![
            (
                Arc::new(Field::new("id", DataType::Int64, true)),
                Arc::new(outer_id) as _,
            ),
            (
                Arc::new(Field::new("inner", inner_struct.data_type().clone(), true)),
                Arc::new(inner_struct) as _,
            ),
        ]);

        let schema = Arc::new(Schema::new(vec![Field::new(
            "record",
            outer_struct.data_type().clone(),
            true,
        )]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(outer_struct)]).unwrap();

        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.metadata.rows_returned, 2);

        match &result.rows[0][0] {
            PropertyValue::Map(outer) => {
                assert_eq!(outer.get("id"), Some(&PropertyValue::Int(1)));
                match outer.get("inner") {
                    Some(PropertyValue::Map(inner)) => {
                        assert_eq!(
                            inner.get("name"),
                            Some(&PropertyValue::String("Alice".into()))
                        );
                        match inner.get("score") {
                            Some(PropertyValue::Float(v)) => {
                                assert!((v - 95.5).abs() < 1e-9);
                            }
                            other => panic!("expected Float, got {other:?}"),
                        }
                    }
                    other => panic!("expected inner Map, got {other:?}"),
                }
            }
            other => panic!("expected outer Map, got {other:?}"),
        }
    }

    #[test]
    fn dictionary_array_decodes_to_underlying_value_type() {
        // A low-cardinality string column encoded as
        // Dictionary<Int32, Utf8>. Four rows reference three unique
        // values; the row's key-index looks up the underlying
        // StringArray entry, then `cell_to_property` recurses to
        // produce `PropertyValue::String`.
        let keys = Int32Array::from(vec![0, 1, 2, 1]);
        let values = StringArray::from(vec!["US", "KR", "JP"]);
        let dict = DictionaryArray::<Int32Type>::try_new(keys, Arc::new(values)).unwrap();

        let schema = Arc::new(Schema::new(vec![Field::new(
            "country",
            dict.data_type().clone(),
            true,
        )]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(dict)]).unwrap();

        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.metadata.rows_returned, 4);
        assert_eq!(result.rows[0][0], PropertyValue::String("US".into()));
        assert_eq!(result.rows[1][0], PropertyValue::String("KR".into()));
        assert_eq!(result.rows[2][0], PropertyValue::String("JP".into()));
        // Row 3 re-uses key 1 — proves the decode dereferences each
        // row independently rather than assuming unique keys.
        assert_eq!(result.rows[3][0], PropertyValue::String("KR".into()));
    }

    #[test]
    fn float32_values_promote_to_property_float64() {
        // Float32 arm is a distinct downcast from the Float64 arm
        // exercised by `primitive_types_map_to_matching_property_variants`.
        // Values must widen losslessly within f32's exact-representation
        // band (the literal `1.5` here is exact in both f32 and f64).
        let schema = schema(vec![("ratio", DataType::Float32)]);
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(Float32Array::from(vec![1.5f32, -0.25, 100.0]))],
        )
        .unwrap();
        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.rows[0][0], PropertyValue::Float(1.5));
        assert_eq!(result.rows[1][0], PropertyValue::Float(-0.25));
        assert_eq!(result.rows[2][0], PropertyValue::Float(100.0));
    }

    #[test]
    fn uint64_value_above_i64_max_refuses_with_descriptive_error() {
        // Pins the overflow-guard branch: i64::try_from fails on
        // UInt64 values > i64::MAX. Silently wrapping would corrupt
        // integer ids / counts, so the branch must Err.
        let schema = schema(vec![("huge", DataType::UInt64)]);
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(UInt64Array::from(vec![u64::MAX]))],
        )
        .unwrap();
        let err = record_batches_to_query_result(&[batch], 0)
            .expect_err("UInt64 > i64::MAX must not silently wrap");
        assert!(
            err.contains("exceeds i64 range"),
            "error must name the range violation: {err}"
        );
    }

    #[test]
    fn uint64_values_within_i64_range_round_trip_as_int() {
        // Complements the refusal test — values in the safe band
        // convert cleanly to PropertyValue::Int without losing
        // magnitude.
        let schema = schema(vec![("count", DataType::UInt64)]);
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(UInt64Array::from(vec![0u64, 42, i64::MAX as u64]))],
        )
        .unwrap();
        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.rows[0][0], PropertyValue::Int(0));
        assert_eq!(result.rows[1][0], PropertyValue::Int(42));
        assert_eq!(result.rows[2][0], PropertyValue::Int(i64::MAX));
    }

    #[test]
    fn dictionary_over_int64_values_decodes_through_recursion() {
        // Counterpart to the Utf8-valued dictionary test — pins the
        // generic value-recursion path against a future refactor
        // that might specialise the Dictionary arm on string-valued
        // dictionaries and silently break integer-valued ones.
        // `Dictionary<Int32, Int64>` with keys `[2, 0, 2, 1]` maps
        // to values `[10, 20, 30]`, yielding `[30, 10, 30, 20]`.
        let keys = Int32Array::from(vec![2, 0, 2, 1]);
        let values = Int64Array::from(vec![10, 20, 30]);
        let dict = DictionaryArray::<Int32Type>::try_new(keys, Arc::new(values)).unwrap();

        let schema = Arc::new(Schema::new(vec![Field::new(
            "enum_val",
            dict.data_type().clone(),
            true,
        )]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(dict)]).unwrap();

        let result = record_batches_to_query_result(&[batch], 0).unwrap();
        assert_eq!(result.metadata.rows_returned, 4);
        assert_eq!(result.rows[0][0], PropertyValue::Int(30));
        assert_eq!(result.rows[1][0], PropertyValue::Int(10));
        assert_eq!(result.rows[2][0], PropertyValue::Int(30));
        assert_eq!(result.rows[3][0], PropertyValue::Int(20));
    }
}
