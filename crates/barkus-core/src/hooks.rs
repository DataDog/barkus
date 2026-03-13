//! Semantic hook trait for domain-specific generation overrides (ADR-0011).
//!
//! The [`SemanticHooks`] trait allows domain layers (e.g., `barkus-sql`) to intercept and
//! override generation decisions at two levels:
//!
//! - **Production hooks** (`on_production`): override an entire production expansion when
//!   the production has a `semantic_hook` attribute.
//! - **Token pool hooks** (`on_token_pool`): override token pool expansion, e.g., to substitute
//!   real identifiers for mechanically generated ones.
//!
//! Additionally, `enter_production` / `exit_production` fire on every production entry/exit
//! for scope tracking.
//!
//! The `()` type implements `SemanticHooks` with all no-ops, and monomorphization eliminates
//! all hook call sites when `()` is used — making hooks zero-cost for non-semantic generation.

use crate::ir::ids::{PoolId, ProductionId};

/// Trait for intercepting generation decisions with domain-specific logic.
///
/// All methods have default no-op implementations. Implement only the methods your domain needs.
pub trait SemanticHooks {
    /// Called when expanding a production with a `semantic_hook` attribute.
    ///
    /// `hook_name` is the value of the `semantic_hook` attribute.
    /// `tape_byte` is the raw byte consumed from the tape for this decision.
    /// `prod_id` identifies the production being expanded.
    ///
    /// Return `Some(bytes)` to override the expansion (bytes are emitted as a terminal node),
    /// or `None` to fall through to normal grammar-driven expansion.
    fn on_production(
        &mut self,
        _hook_name: &str,
        _tape_byte: u8,
        _prod_id: ProductionId,
    ) -> Option<Vec<u8>> {
        None
    }

    /// Called when expanding a `TerminalKind::TokenPool`.
    ///
    /// `pool_id` identifies which token pool (typically a lexer rule).
    /// `tape_byte` is the raw byte consumed from the tape for this decision.
    ///
    /// Return `Some(bytes)` to override the pool expansion,
    /// or `None` to fall through to mechanical expansion from the pool's alternatives.
    fn on_token_pool(&mut self, _pool_id: PoolId, _tape_byte: u8) -> Option<Vec<u8>> {
        None
    }

    /// Called on entry to every production expansion (before alternatives are chosen).
    ///
    /// Use this for scope tracking (e.g., entering a FROM clause).
    fn enter_production(&mut self, _prod_id: ProductionId) {}

    /// Called on exit from every production expansion (after all children are expanded).
    ///
    /// Use this for scope tracking (e.g., exiting a FROM clause).
    fn exit_production(&mut self, _prod_id: ProductionId) {}
}

/// Zero-cost default: all hooks are no-ops. Monomorphization eliminates all call sites.
impl SemanticHooks for () {}
