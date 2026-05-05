//! Lower a `QueryIR::Expr` to a DataFusion `Expr`.
//!
//! Every variant the lowering refuses returns
//! [`FederationError::Unsupported`] with a descriptive message —
//! there is no silent truncation.
//!
//! Supported today:
//! - `Expr::Literal { value }` — PropertyValue → ScalarValue for
//!   Null/Bool/Int/Float/String.
//! - `Expr::Property { variable, field }` — resolves to a qualified
//!   `col("<variable>.<field>")`. Qualification is load-bearing the
//!   moment more than one scan is active (JOIN lowering);
//!   single-scan plans already alias the table by the variable, so
//!   `c.name` resolves identically to bare `name` there.
//! - `Expr::Comparison` with Eq / Neq / Lt / Lte / Gt / Gte.
//! - `Expr::Logical` with And / Or (Xor is rejected — DataFusion
//!   has no native XOR).
//! - `Expr::Not`.
//! - `Expr::IsNull { negated }`.
//! - `Expr::In` with a literal list. Lowers to `col IN (…)`.
//! - `Expr::StringOp` for StartsWith / EndsWith / Contains.
//!
//! Deliberately rejected (by variant, with message):
//! - `Expr::Param` — bind-parameter surface is not yet implemented.
//! - `Expr::FunctionCall` — built-ins / UDFs are not yet implemented.
//! - `Expr::Exists` / `Expr::Subquery` / `Expr::Case` — advanced
//!   Cypher, lowering is non-trivial and not yet implemented.
//! - `StringOp::Regex` — DataFusion has regex, but the semantics of
//!   Cypher `=~` diverge on backreferences; defer until we have a
//!   deliberate test plan.

use chrono::NaiveDate;
use datafusion::arrow::datatypes::DataType;
use datafusion::logical_expr::{Expr as DfExpr, col, lit};
use datafusion::scalar::ScalarValue;

use ox_core::types::PropertyValue;
use ox_query_ir::query::{ComparisonOp, Expr, LogicalOp, StringOp};

use crate::error::{FederationError, FederationResult};

/// Entry point — translate a `QueryIR::Expr` into a DataFusion
/// `Expr`. Pure function; no state.
pub fn expr_to_df(expr: &Expr) -> FederationResult<DfExpr> {
    match expr {
        Expr::Literal { value } => Ok(DfExpr::Literal(property_value_to_scalar(value)?, None)),
        Expr::Property { variable, field } => match field {
            Some(f) => Ok(col(format!("{}.{}", variable.as_str(), f.as_str()))),
            None => Err(FederationError::unsupported(
                "ExprLowering: bare variable reference (Property without field) \
                 has no single-column meaning in SQL — reference a specific field",
            )),
        },
        Expr::Comparison { left, op, right } => {
            let l = expr_to_df(left)?;
            let r = expr_to_df(right)?;
            Ok(apply_comparison(l, *op, r))
        }
        Expr::Logical { left, op, right } => {
            let l = expr_to_df(left)?;
            let r = expr_to_df(right)?;
            match op {
                LogicalOp::And => Ok(l.and(r)),
                LogicalOp::Or => Ok(l.or(r)),
                LogicalOp::Xor => Err(FederationError::unsupported(
                    "ExprLowering: LogicalOp::Xor has no DataFusion native \
                     equivalent; rewrite as `(a OR b) AND NOT (a AND b)` at \
                     the query-IR level",
                )),
            }
        }
        Expr::Not { inner } => Ok(DfExpr::Not(Box::new(expr_to_df(inner)?))),
        Expr::IsNull { expr, negated } => {
            let inner = expr_to_df(expr)?;
            Ok(if *negated {
                DfExpr::IsNotNull(Box::new(inner))
            } else {
                DfExpr::IsNull(Box::new(inner))
            })
        }
        Expr::In { expr, values } => {
            let subject = expr_to_df(expr)?;
            let list = values
                .iter()
                .map(|v| property_value_to_scalar(v).map(|s| DfExpr::Literal(s, None)))
                .collect::<FederationResult<Vec<_>>>()?;
            Ok(subject.in_list(list, false))
        }
        Expr::StringOp { left, op, right } => {
            let l = expr_to_df(left)?;
            let pattern = extract_string_literal(right, op)?;
            match op {
                StringOp::StartsWith => Ok(l.like(lit(format!("{pattern}%")))),
                StringOp::EndsWith => Ok(l.like(lit(format!("%{pattern}")))),
                StringOp::Contains => Ok(l.like(lit(format!("%{pattern}%")))),
                StringOp::Regex => Err(FederationError::unsupported(
                    "ExprLowering: StringOp::Regex (Cypher `=~`) lowering is \
                     deferred — Cypher and SQL regex dialects differ on \
                     backreferences; a dedicated test plan will pin the \
                     semantics before enabling",
                )),
            }
        }
        Expr::Param { name } => Err(FederationError::unsupported(format!(
            "ExprLowering: Expr::Param {{ name = '{name}' }} — parameter \
             bindings are not yet implemented"
        ))),
        Expr::FunctionCall { function, .. } => Err(FederationError::unsupported(format!(
            "ExprLowering: Expr::FunctionCall {{ function = {function:?} }} \
             is not yet implemented"
        ))),
        Expr::Exists { .. } => Err(FederationError::unsupported(
            "ExprLowering: Expr::Exists is not yet implemented",
        )),
        Expr::Case { .. } => Err(FederationError::unsupported(
            "ExprLowering: Expr::Case is not yet implemented",
        )),
        Expr::Subquery { .. } => Err(FederationError::unsupported(
            "ExprLowering: Expr::Subquery is not yet implemented",
        )),
    }
}

fn apply_comparison(l: DfExpr, op: ComparisonOp, r: DfExpr) -> DfExpr {
    match op {
        ComparisonOp::Eq => l.eq(r),
        ComparisonOp::Neq => l.not_eq(r),
        ComparisonOp::Lt => l.lt(r),
        ComparisonOp::Lte => l.lt_eq(r),
        ComparisonOp::Gt => l.gt(r),
        ComparisonOp::Gte => l.gt_eq(r),
    }
}

fn extract_string_literal(right: &Expr, op: &StringOp) -> FederationResult<String> {
    match right {
        Expr::Literal {
            value: PropertyValue::String(s),
        } => Ok(s.clone()),
        _ => Err(FederationError::unsupported(format!(
            "ExprLowering: StringOp::{op:?} requires a string literal on the right; \
             got {right:?}"
        ))),
    }
}

fn property_value_to_scalar(value: &PropertyValue) -> FederationResult<ScalarValue> {
    match value {
        PropertyValue::Null => Ok(ScalarValue::Null),
        PropertyValue::Bool(b) => Ok(ScalarValue::Boolean(Some(*b))),
        PropertyValue::Int(i) => Ok(ScalarValue::Int64(Some(*i))),
        PropertyValue::Float(f) => Ok(ScalarValue::Float64(Some(*f))),
        PropertyValue::String(s) => Ok(ScalarValue::Utf8(Some(s.clone()))),
        PropertyValue::Date(d) => {
            // `NaiveDate` → days since epoch. Derived from `chrono`'s
            // const UNIX_EPOCH so we don't reach for `from_ymd_opt`
            // and an unwrap of a value that's known at the type level.
            let epoch = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH.date_naive();
            let days = (*d - epoch).num_days() as i32;
            Ok(ScalarValue::Date32(Some(days)))
        }
        PropertyValue::DateTime(dt) => Ok(ScalarValue::TimestampMicrosecond(
            Some(dt.and_utc().timestamp_micros()),
            None,
        )),
        PropertyValue::Duration(_)
        | PropertyValue::Bytes(_)
        | PropertyValue::List(_)
        | PropertyValue::Map(_) => Err(FederationError::unsupported(format!(
            "ExprLowering: PropertyValue variant {value:?} has no single-scalar \
             lowering for DataFusion literals today"
        ))),
    }
}

/// Build a DataFusion predicate for an inline property-filter value —
/// the `PropertyFilter::value` on `GraphPattern::Node`. Distinct
/// from `expr_to_df` because a PropertyFilter is structurally a
/// `{field, value}` pair attached to a bound node variable; the
/// variable qualifies the column so multi-scan JOIN plans keep the
/// filter on the correct side of the join.
pub fn property_filter_to_df(
    variable: &ox_core::variable_name::VariableName,
    field: &ox_core::property_key::PropertyKey,
    value: &Expr,
) -> FederationResult<DfExpr> {
    let rhs = expr_to_df(value)?;
    Ok(col(format!("{}.{}", variable.as_str(), field.as_str())).eq(rhs))
}

/// Hint: does `dt` fit the target column type we're about to
/// compare against? Unused today (no type inference yet) but kept
/// here so implicit-cast folding can land in this file.
#[allow(dead_code)]
pub(crate) fn is_numeric(dt: &DataType) -> bool {
    matches!(
        dt,
        DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Float16
            | DataType::Float32
            | DataType::Float64
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::property_key::PropertyKey;
    use ox_core::variable_name::VariableName;

    fn prop(var: &str, field: &str) -> Expr {
        Expr::Property {
            variable: VariableName::new(var).unwrap(),
            field: Some(PropertyKey::new(field).unwrap()),
        }
    }

    fn lit_int(v: i64) -> Expr {
        Expr::Literal {
            value: PropertyValue::Int(v),
        }
    }

    fn lit_str(v: &str) -> Expr {
        Expr::Literal {
            value: PropertyValue::String(v.into()),
        }
    }

    #[test]
    fn literals_lower_to_scalar_matching_variant() {
        match expr_to_df(&lit_int(42)).unwrap() {
            DfExpr::Literal(ScalarValue::Int64(Some(42)), _) => {}
            other => panic!("expected Int64(42), got {other:?}"),
        }
        match expr_to_df(&lit_str("hello")).unwrap() {
            DfExpr::Literal(ScalarValue::Utf8(Some(s)), _) if s == "hello" => {}
            other => panic!("expected Utf8(\"hello\"), got {other:?}"),
        }
    }

    #[test]
    fn comparison_renders_as_binary_expr() {
        let e = Expr::Comparison {
            left: Box::new(prop("n", "age")),
            op: ComparisonOp::Gt,
            right: Box::new(lit_int(21)),
        };
        let df = expr_to_df(&e).unwrap();
        let rendered = format!("{df}");
        assert!(
            rendered.contains("age") && rendered.contains("21") && rendered.contains(">"),
            "got: {rendered}"
        );
    }

    #[test]
    fn logical_and_chains_two_comparisons() {
        let e = Expr::Logical {
            left: Box::new(Expr::Comparison {
                left: Box::new(prop("n", "age")),
                op: ComparisonOp::Gte,
                right: Box::new(lit_int(18)),
            }),
            op: LogicalOp::And,
            right: Box::new(Expr::Comparison {
                left: Box::new(prop("n", "status")),
                op: ComparisonOp::Eq,
                right: Box::new(lit_str("active")),
            }),
        };
        let df = expr_to_df(&e).unwrap();
        let rendered = format!("{df}");
        assert!(rendered.contains("AND"), "got: {rendered}");
    }

    #[test]
    fn logical_xor_is_explicitly_unsupported() {
        let e = Expr::Logical {
            left: Box::new(lit_int(1)),
            op: LogicalOp::Xor,
            right: Box::new(lit_int(2)),
        };
        assert!(matches!(
            expr_to_df(&e),
            Err(FederationError::Unsupported(_))
        ));
    }

    #[test]
    fn starts_with_lowers_to_like_with_trailing_percent() {
        let e = Expr::StringOp {
            left: Box::new(prop("n", "name")),
            op: StringOp::StartsWith,
            right: Box::new(lit_str("Al")),
        };
        let df = expr_to_df(&e).unwrap();
        let rendered = format!("{df}");
        assert!(rendered.contains("LIKE") && rendered.contains("Al%"), "got: {rendered}");
    }

    #[test]
    fn in_list_lowers_to_df_in_list() {
        let e = Expr::In {
            expr: Box::new(prop("n", "status")),
            values: vec![
                PropertyValue::String("active".into()),
                PropertyValue::String("pending".into()),
            ],
        };
        let df = expr_to_df(&e).unwrap();
        let rendered = format!("{df}");
        assert!(
            rendered.contains("IN") && rendered.contains("active") && rendered.contains("pending"),
            "got: {rendered}"
        );
    }

    #[test]
    fn is_null_vs_is_not_null_distinguishes_on_negated_flag() {
        let raw = Expr::IsNull {
            expr: Box::new(prop("n", "deleted_at")),
            negated: false,
        };
        let neg = Expr::IsNull {
            expr: Box::new(prop("n", "deleted_at")),
            negated: true,
        };
        let r1 = format!("{}", expr_to_df(&raw).unwrap());
        let r2 = format!("{}", expr_to_df(&neg).unwrap());
        assert!(r1.contains("IS NULL"));
        assert!(r2.contains("IS NOT NULL"));
    }

    #[test]
    fn regex_is_explicitly_unsupported_until_semantics_are_pinned() {
        let e = Expr::StringOp {
            left: Box::new(prop("n", "code")),
            op: StringOp::Regex,
            right: Box::new(lit_str("^A.*")),
        };
        assert!(matches!(
            expr_to_df(&e),
            Err(FederationError::Unsupported(_))
        ));
    }

    #[test]
    fn bare_variable_reference_is_rejected_with_helpful_message() {
        let e = Expr::Property {
            variable: VariableName::new("n").unwrap(),
            field: None,
        };
        match expr_to_df(&e) {
            Err(FederationError::Unsupported(msg)) => {
                assert!(msg.contains("bare variable"));
            }
            other => panic!("expected Unsupported with hint, got {other:?}"),
        }
    }
}
