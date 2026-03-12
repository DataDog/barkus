//! Mutation engine for structure-aware fuzzing (ADR-0004).
//!
//! Two-level architecture:
//! - **Level 1 (tape-level):** cheap, structure-preserving operators that modify raw tape bytes
//!   (point mutation, range re-randomize, splice).
//! - **Level 2 (structure-aware):** operators that use [`TapeMap`](crate::tape::map::TapeMap) and
//!   AST metadata to make targeted, semantically meaningful changes (subtree regeneration, toggle
//!   optional, perturb repetition).
//!
//! Key types:
//! - [`MutationMeta`] — per-input analysis combining `TapeMap` with AST-derived data (subtree
//!   sizes, depths, production indexes).
//! - [`FragmentDb`] — reservoir-sampled pool of tape fragments per production, used by splice
//!   mutations.
//! - [`MutationKind`] — enum of all mutation operators, returned by [`ops::mutate`] to indicate
//!   which operator was applied.

pub mod fragment_db;
pub mod meta;
pub mod ops;

pub use fragment_db::FragmentDb;
pub use meta::MutationMeta;
pub use ops::MutationKind;
