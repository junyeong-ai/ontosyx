//! Per-trait `DefaultBrain` implementations.
//!
//! The trait definitions and the `DefaultBrain` struct itself live
//! in the parent crate root so each per-trait impl can register
//! against them via `impl Trait for DefaultBrain` without an orphan-
//! rule conflict.

mod designer;
mod editor;
mod explainer;
mod judge;
mod metadata;
mod repo;
mod safety_judge;
mod translator;
