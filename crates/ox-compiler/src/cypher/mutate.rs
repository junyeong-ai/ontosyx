use ox_core::error::OxResult;
use ox_core::query_ir::{MutateOp, PropertyAssignment};

use super::expr::compile_expr;
use super::params::{ParamCollector, escape_identifier};

pub(super) fn compile_mutate_op(op: &MutateOp, pc: &mut ParamCollector) -> OxResult<String> {
    Ok(match op {
        MutateOp::CreateNode {
            variable,
            label,
            properties,
        } => {
            let props = compile_assignments(properties, pc)?;
            let escaped_label = escape_identifier(label);
            format!("CREATE ({variable}:{escaped_label} {{{props}}})")
        }

        MutateOp::CreateEdge {
            variable,
            label,
            source,
            target,
            properties,
        } => {
            let var = variable.as_deref().unwrap_or("");
            let props = if properties.is_empty() {
                String::new()
            } else {
                format!(" {{{}}}", compile_assignments(properties, pc)?)
            };
            let escaped_label = escape_identifier(label);
            format!("CREATE ({source})-[{var}:{escaped_label}{props}]->({target})")
        }

        MutateOp::MergeNode {
            variable,
            label,
            match_properties,
            on_create,
            on_match,
        } => {
            let match_props = compile_assignments(match_properties, pc)?;
            let escaped_label = escape_identifier(label);
            let mut stmt = format!("MERGE ({variable}:{escaped_label} {{{match_props}}})");
            if !on_create.is_empty() {
                stmt.push_str(&format!(
                    "\n  ON CREATE SET {}",
                    compile_set_assignments(variable, on_create, pc)?
                ));
            }
            if !on_match.is_empty() {
                stmt.push_str(&format!(
                    "\n  ON MATCH SET {}",
                    compile_set_assignments(variable, on_match, pc)?
                ));
            }
            stmt
        }

        MutateOp::MergeEdge {
            variable,
            label,
            source,
            target,
            match_properties,
            on_create,
            on_match,
        } => {
            let var = variable.as_deref().unwrap_or("r");
            let match_props = if match_properties.is_empty() {
                String::new()
            } else {
                format!(" {{{}}}", compile_assignments(match_properties, pc)?)
            };
            let escaped_label = escape_identifier(label);
            let mut stmt =
                format!("MERGE ({source})-[{var}:{escaped_label}{match_props}]->({target})");
            if !on_create.is_empty() {
                stmt.push_str(&format!(
                    "\n  ON CREATE SET {}",
                    compile_set_assignments(var, on_create, pc)?
                ));
            }
            if !on_match.is_empty() {
                stmt.push_str(&format!(
                    "\n  ON MATCH SET {}",
                    compile_set_assignments(var, on_match, pc)?
                ));
            }
            stmt
        }

        MutateOp::SetProperty {
            variable,
            property,
            value,
        } => format!(
            "SET {variable}.{} = {}",
            escape_identifier(property),
            compile_expr(value, pc)?
        ),

        MutateOp::Delete { variable, detach } => {
            if *detach {
                format!("DETACH DELETE {variable}")
            } else {
                format!("DELETE {variable}")
            }
        }

        MutateOp::RemoveProperty { variable, property } => {
            format!("REMOVE {variable}.{}", escape_identifier(property))
        }

        MutateOp::RemoveLabel { variable, label } => {
            format!("REMOVE {variable}:{}", escape_identifier(label))
        }
    })
}

pub(super) fn compile_assignments(
    assignments: &[PropertyAssignment],
    pc: &mut ParamCollector,
) -> OxResult<String> {
    let items = assignments
        .iter()
        .map(|a| {
            Ok(format!(
                "{}: {}",
                escape_identifier(&a.property),
                compile_expr(&a.value, pc)?
            ))
        })
        .collect::<OxResult<Vec<_>>>()?;
    Ok(items.join(", "))
}

pub(super) fn compile_set_assignments(
    variable: &str,
    assignments: &[PropertyAssignment],
    pc: &mut ParamCollector,
) -> OxResult<String> {
    let items = assignments
        .iter()
        .map(|a| {
            Ok(format!(
                "{variable}.{} = {}",
                escape_identifier(&a.property),
                compile_expr(&a.value, pc)?
            ))
        })
        .collect::<OxResult<Vec<_>>>()?;
    Ok(items.join(", "))
}
