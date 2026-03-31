use ox_core::query_ir::{AggFunction, Expr, OrderClause, Projection, SortDirection};
use ox_core::types::PropertyValue;

use super::params::{ParamCollector, escape_identifier};
use super::pattern::compile_pattern;
use super::query::compile_op;

pub(super) fn compile_expr(expr: &Expr, pc: &mut ParamCollector) -> String {
    match expr {
        Expr::Literal { value } => compile_value(value, pc),

        Expr::Property { variable, field } => match field {
            Some(f) => format!("{variable}.{}", escape_identifier(f)),
            None => variable.clone(),
        },

        Expr::Comparison { left, op, right } => {
            format!(
                "{} {op} {}",
                compile_expr(left, pc),
                compile_expr(right, pc)
            )
        }

        Expr::Logical { left, op, right } => {
            format!(
                "({} {op} {})",
                compile_expr(left, pc),
                compile_expr(right, pc)
            )
        }

        Expr::Not { inner } => format!("NOT ({})", compile_expr(inner, pc)),

        Expr::In { expr, values } => {
            let vals: Vec<String> = values.iter().map(|v| compile_value(v, pc)).collect();
            format!("{} IN [{}]", compile_expr(expr, pc), vals.join(", "))
        }

        Expr::IsNull { expr, negated } => {
            if *negated {
                format!("{} IS NOT NULL", compile_expr(expr, pc))
            } else {
                format!("{} IS NULL", compile_expr(expr, pc))
            }
        }

        Expr::StringOp { left, op, right } => {
            format!(
                "{} {op} {}",
                compile_expr(left, pc),
                compile_expr(right, pc)
            )
        }

        Expr::FunctionCall { function, args } => {
            let args_str: Vec<String> = args.iter().map(|a| compile_expr(a, pc)).collect();
            format!("{function}({})", args_str.join(", "))
        }

        Expr::Exists { pattern } => {
            format!("EXISTS {{ MATCH {} }}", compile_pattern(pattern, pc))
        }

        Expr::Case {
            operand,
            when_clauses,
            else_result,
        } => {
            let mut parts = Vec::new();
            parts.push("CASE".to_string());
            if let Some(op) = operand {
                parts.push(compile_expr(op, pc));
            }
            for wc in when_clauses {
                parts.push(format!(
                    "WHEN {} THEN {}",
                    compile_expr(&wc.condition, pc),
                    compile_expr(&wc.result, pc),
                ));
            }
            if let Some(els) = else_result {
                parts.push(format!("ELSE {}", compile_expr(els, pc)));
            }
            parts.push("END".to_string());
            parts.join(" ")
        }

        Expr::Subquery {
            query,
            import_variables,
        } => {
            // Compile as COUNT { WITH vars ... } subquery expression
            let mut inner_parts = Vec::new();
            if !import_variables.is_empty() {
                inner_parts.push(format!("WITH {}", import_variables.join(", ")));
            }
            // compile_op may fail; in expr context we panic on error (IR should be valid)
            compile_op(&query.operation, &mut inner_parts, pc)
                .expect("subquery compilation should not fail");
            if !query.order_by.is_empty() {
                inner_parts.push(compile_order_by(&query.order_by, pc));
            }
            if let Some(skip) = query.skip {
                inner_parts.push(format!("SKIP {skip}"));
            }
            if let Some(limit) = query.limit {
                inner_parts.push(format!("LIMIT {limit}"));
            }
            format!("COUNT {{ {} }}", inner_parts.join(" "))
        }
    }
}

/// Compile a PropertyValue into a parameterized placeholder or inline literal.
/// NULL stays inline (Cypher `null`). Date/DateTime/Duration stay as inline Cypher
/// function calls because neo4rs cannot bind these types as parameters.
pub(super) fn compile_value(value: &PropertyValue, pc: &mut ParamCollector) -> String {
    match value {
        PropertyValue::Null => "null".to_string(),
        PropertyValue::Date(v) => format!("date('{v}')"),
        PropertyValue::DateTime(v) => format!("datetime('{v}')"),
        PropertyValue::Duration(v) => {
            if !v.starts_with('P') || v.len() < 2 {
                return "null".to_string();
            }
            format!("duration('{v}')")
        }
        PropertyValue::Bytes(_) => "null".to_string(), // Cypher doesn't support bytes natively
        other => pc.push(other.clone()),
    }
}

pub(super) fn compile_projection(proj: &Projection, pc: &mut ParamCollector) -> String {
    match proj {
        Projection::Field {
            variable,
            field,
            alias,
        } => {
            let base = format!("{variable}.{}", escape_identifier(field));
            alias
                .as_ref()
                .map(|a| format!("{base} AS {a}"))
                .unwrap_or(base)
        }
        Projection::Variable { variable, alias } => alias
            .as_ref()
            .map(|a| format!("{variable} AS {a}"))
            .unwrap_or_else(|| variable.clone()),
        Projection::Expression { expr, alias } => {
            format!("{} AS {alias}", compile_expr(expr, pc))
        }
        Projection::Aggregation {
            function,
            argument,
            alias,
            distinct,
        } => {
            let target = compile_projection(argument, pc);
            let func_str = compile_agg_function(function, &target, *distinct);
            format!("{func_str} AS {alias}")
        }
        Projection::AllProperties { variable } => format!("{variable} {{.*}}"),
    }
}

pub(super) fn compile_order_by(clauses: &[OrderClause], pc: &mut ParamCollector) -> String {
    let items: Vec<String> = clauses
        .iter()
        .map(|c| {
            // For aliased projections (aggregation, expression), use the alias in ORDER BY
            let order_ref = match &c.projection {
                Projection::Aggregation { alias, .. } => alias.clone(),
                Projection::Expression { alias, .. } => alias.clone(),
                Projection::Field { alias: Some(a), .. } => a.clone(),
                Projection::Field {
                    variable,
                    field,
                    alias: None,
                } => format!("{variable}.{}", escape_identifier(field)),
                Projection::Variable { alias: Some(a), .. } => a.clone(),
                // Bare variable names from LLM may reference RETURN aliases;
                // use escape_identifier to ensure they're valid Cypher identifiers.
                Projection::Variable { variable, alias: None } => escape_identifier(variable),
                other => compile_projection(other, pc),
            };
            match c.direction {
                SortDirection::Asc => order_ref,
                SortDirection::Desc => format!("{order_ref} DESC"),
            }
        })
        .collect();
    format!("ORDER BY {}", items.join(", "))
}

pub(super) fn compile_agg_function(function: &AggFunction, target: &str, distinct: bool) -> String {
    let func_name = match function {
        AggFunction::Count => "count",
        AggFunction::Sum => "sum",
        AggFunction::Avg => "avg",
        AggFunction::Min => "min",
        AggFunction::Max => "max",
        AggFunction::Collect => "collect",
        AggFunction::StdDev => "stDev",
        AggFunction::Percentile => "percentileCont",
        AggFunction::CollectList => "collect",
    };
    let dist = if distinct { "DISTINCT " } else { "" };
    format!("{func_name}({dist}{target})")
}
