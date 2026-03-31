use ox_core::error::OxResult;
use ox_core::query_ir::{ChainStep, GraphPattern, PathElement, PropertyFilter};
use ox_core::types::Direction;

use super::expr::{compile_expr, compile_projection};
use super::params::{ParamCollector, escape_identifier};
use super::query::compile_op;

pub(super) fn compile_pattern(pattern: &GraphPattern, pc: &mut ParamCollector) -> String {
    match pattern {
        GraphPattern::Node {
            variable,
            label,
            property_filters,
        } => compile_node_ref_inline(variable, label, property_filters, pc),

        GraphPattern::Relationship {
            variable,
            label,
            source,
            target,
            direction,
            property_filters,
            var_length,
        } => {
            let var = variable.as_deref().unwrap_or("");
            let lbl = label
                .as_deref()
                .map(|l| format!(":{}", escape_identifier(l)))
                .unwrap_or_default();
            let props = compile_inline_props(property_filters, pc);
            let vl = var_length
                .as_ref()
                .map(|vl| match (vl.min, vl.max) {
                    (Some(min), Some(max)) => format!("*{min}..{max}"),
                    (Some(min), None) => format!("*{min}.."),
                    (None, Some(max)) => format!("*..{max}"),
                    (None, None) => "*".to_string(),
                })
                .unwrap_or_default();
            let rel = format!("[{var}{lbl}{vl}{props}]");
            let rel_str = format_direction_pattern(&rel, direction);
            format!("({source}){rel_str}({target})")
        }

        GraphPattern::Path { elements } => {
            let mut out = String::new();
            for elem in elements {
                match elem {
                    PathElement::Node { variable, label } => {
                        let lbl = label
                            .as_deref()
                            .map(|l| format!(":{}", escape_identifier(l)))
                            .unwrap_or_default();
                        out.push_str(&format!("({variable}{lbl})"));
                    }
                    PathElement::Edge {
                        variable,
                        label,
                        direction,
                    } => {
                        let var = variable.as_deref().unwrap_or("");
                        let lbl = label
                            .as_deref()
                            .map(|l| format!(":{}", escape_identifier(l)))
                            .unwrap_or_default();
                        let rel = format!("[{var}{lbl}]");
                        out.push_str(&format_direction_pattern(&rel, direction));
                    }
                }
            }
            out
        }
    }
}

pub(super) fn compile_node_ref_inline(
    variable: &str,
    label: &Option<String>,
    property_filters: &[PropertyFilter],
    pc: &mut ParamCollector,
) -> String {
    let lbl = label
        .as_deref()
        .map(|l| format!(":{}", escape_identifier(l)))
        .unwrap_or_default();
    let props = compile_inline_props(property_filters, pc);
    format!("({variable}{lbl}{props})")
}

pub(super) fn compile_inline_props(filters: &[PropertyFilter], pc: &mut ParamCollector) -> String {
    if filters.is_empty() {
        return String::new();
    }
    let props: Vec<String> = filters
        .iter()
        .map(|f| {
            format!(
                "{}: {}",
                escape_identifier(&f.property),
                compile_expr(&f.value, pc)
            )
        })
        .collect();
    format!(" {{{}}}", props.join(", "))
}

pub(super) fn format_direction_pattern(rel: &str, direction: &Direction) -> String {
    match direction {
        Direction::Outgoing => format!("-{rel}->"),
        Direction::Incoming => format!("<-{rel}-"),
        Direction::Both => format!("-{rel}-"),
    }
}

pub(super) fn compile_chain_step(
    step: &ChainStep,
    parts: &mut Vec<String>,
    pc: &mut ParamCollector,
) -> OxResult<()> {
    if !step.pass_through.is_empty() {
        parts.push(format!(
            "WITH {}",
            step.pass_through
                .iter()
                .map(|p| compile_projection(p, pc))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    compile_op(&step.operation, parts, pc)
}
