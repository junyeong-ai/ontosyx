//! Closed-set wire-enum macro.
//!
//! Every closed-set enum that travels over the wire (HTTP / SQL
//! / log) carries the same five-method shape:
//! `ALL` + `as_str(self) const fn` + `from_wire_str` +
//! `all_wire_strings` + `impl Display`. Hand-rolling these per
//! enum was ~50 lines of boilerplate per variant family and
//! invited pattern drift — one enum forgetting `from_wire_str`,
//! another using a `Result` instead of `Option` return, and so
//! on.
//!
//! [`wire_enum!`] is the single source of truth for that
//! shape. A new closed-set enum is one declaration:
//!
//! ```ignore
//! wire_enum! {
//!     /// Doc string carries through to the generated enum.
//!     pub enum MyKind {
//!         FirstVariant => "first_variant",
//!         SecondVariant => "second_variant",
//!     }
//! }
//! ```
//!
//! The generated enum derives `Debug + Clone + Copy +
//! PartialEq + Eq + Hash + Serialize + Deserialize +
//! utoipa::ToSchema`. Wire strings are explicit per-variant
//! (`Variant => "literal"`) rather than relying on serde's
//! `rename_all = "snake_case"` — keeping the human-readable
//! tag and the typed variant in lock-step at the declaration
//! site, and letting the macro hand the same literal to both
//! `serde(rename = "...")` and the `as_str` arm.
//!
//! Per-enum extras (e.g. `EvaluationRunStatus::is_terminal`,
//! `NotificationLogEventType::from_subscription`) live in
//! separate `impl` blocks alongside the macro invocation —
//! the macro only produces the shared shape.
//!
//! Tests still live next to each enum's call site; they
//! exercise the variant set the enum actually owns. The
//! macro guarantees the shape; the test guarantees the
//! contents.

/// Generate the closed-set wire-enum shape — see the module
/// doc for the canonical example.
#[macro_export]
macro_rules! wire_enum {
    (
        $(#[$enum_meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident => $wire:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash,
            ::serde::Serialize, ::serde::Deserialize,
            ::utoipa::ToSchema,
        )]
        $vis enum $name {
            $(
                $(#[$variant_meta])*
                #[serde(rename = $wire)]
                #[schema(rename = $wire)]
                $variant
            ),+
        }

        impl $name {
            /// Every variant in declaration order. Single source
            /// of truth for `from_wire_str` + `all_wire_strings`.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// Stable wire string. Match the FE catalog key the
            /// callers render against.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire,)+
                }
            }

            /// Inverse of [`Self::as_str`]. Returns `None` on an
            /// unrecognised tag — callers decide whether that is
            /// a corruption error or a forward-compat skip.
            pub fn from_wire_str(s: &str) -> Option<Self> {
                match s {
                    $($wire => Some(Self::$variant),)+
                    _ => None,
                }
            }

            /// Wire-string bag for SQL `= ANY($N::text[])` binds,
            /// FE catalogue rendering, and parity audits.
            pub fn all_wire_strings() -> Vec<&'static str> {
                Self::ALL.iter().copied().map(Self::as_str).collect()
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

#[cfg(test)]
mod tests {
    // The macro itself is exercised by every call-site test
    // (every closed-set enum that uses it). What we pin here is
    // the macro-generated shape on a tiny test enum so a future
    // change to the macro that drops a method or breaks the
    // wire-string round-trip fails fast in this crate, before
    // a downstream enum even compiles.

    crate::wire_enum! {
        pub enum WireEnumProbe {
            First => "first",
            Second => "second_variant",
        }
    }

    #[test]
    fn macro_emits_all_variants_in_declaration_order() {
        assert_eq!(WireEnumProbe::ALL.len(), 2);
        assert_eq!(WireEnumProbe::ALL[0], WireEnumProbe::First);
        assert_eq!(WireEnumProbe::ALL[1], WireEnumProbe::Second);
    }

    #[test]
    fn macro_round_trips_through_wire_str() {
        for v in WireEnumProbe::ALL.iter().copied() {
            assert_eq!(WireEnumProbe::from_wire_str(v.as_str()), Some(v));
        }
        assert_eq!(WireEnumProbe::from_wire_str("nope"), None);
    }

    #[test]
    fn macro_serde_uses_explicit_wire_literals_not_snake_case_inference() {
        // The macro sets `#[serde(rename = "...")]` per variant
        // explicitly so the wire literal at the declaration is
        // also the serde tag. A future change that swaps to
        // `rename_all = "snake_case"` would silently lose the
        // ability to use non-snake_case literals — pinned here.
        let v = WireEnumProbe::Second;
        assert_eq!(serde_json::to_string(&v).unwrap(), "\"second_variant\"");
        let back: WireEnumProbe = serde_json::from_str("\"second_variant\"").unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn macro_display_matches_as_str() {
        assert_eq!(format!("{}", WireEnumProbe::First), "first");
    }

    #[test]
    fn macro_all_wire_strings_matches_as_str_per_variant() {
        let wire: Vec<&'static str> = WireEnumProbe::all_wire_strings();
        let from_as_str: Vec<&'static str> = WireEnumProbe::ALL
            .iter()
            .copied()
            .map(WireEnumProbe::as_str)
            .collect();
        assert_eq!(wire, from_as_str);
    }
}
