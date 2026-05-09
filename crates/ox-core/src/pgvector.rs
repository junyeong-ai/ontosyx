//! pgvector text wire format encoder.
//!
//! pgvector exposes vector literals in the canonical postgres text
//! representation `[v1,v2,...,vN]`. This is the form sqlx ships
//! when binding a string parameter cast to `vector` at the
//! server side (`$N::vector`). Every workspace crate that talks
//! to a `vector(N)` column — `ox-store` for retrieval surfaces,
//! `ox-memory` for the semantic memory store — encodes through
//! [`format_vector`] so the wire shape is byte-identical across
//! the platform.
//!
//! Dimensions are not validated here; the cast at bind time
//! (`$N::vector`) lets postgres reject mismatches with a
//! structured error (`ERROR: expected N dimensions, not M`).
//! Callers stay decoupled from the column-side dimension knob.

/// Encode a `&[f32]` as a pgvector text literal.
///
/// Output shape: `[v1,v2,...]` — square brackets, comma
/// separator, no whitespace, every component formatted via
/// `f32::Display`. Empty slice yields `"[]"`.
///
/// The capacity heuristic (8 chars/value + 2 for the brackets)
/// fits typical `f32::Display` output (`-0.123456` etc.) without
/// a re-grow on the common path.
#[must_use]
pub fn format_vector(values: &[f32]) -> String {
    let mut s = String::with_capacity(values.len() * 8 + 2);
    s.push('[');
    let mut first = true;
    for v in values {
        if !first {
            s.push(',');
        }
        first = false;
        s.push_str(&v.to_string());
    }
    s.push(']');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_vector_renders_as_empty_brackets() {
        assert_eq!(format_vector(&[]), "[]");
    }

    #[test]
    fn single_component_renders_without_comma() {
        assert_eq!(format_vector(&[0.5]), "[0.5]");
    }

    #[test]
    fn multi_component_uses_comma_separator_no_space() {
        assert_eq!(format_vector(&[1.0, -2.5, 3.25]), "[1,-2.5,3.25]");
    }

    #[test]
    fn negative_zero_round_trips() {
        // f32 distinguishes 0 / -0 in the bit pattern;
        // `Display` prints `-0` for negative zero, which
        // pgvector accepts as the same value.
        assert_eq!(format_vector(&[-0.0]), "[-0]");
    }
}
