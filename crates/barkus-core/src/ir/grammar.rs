use serde::{Deserialize, Serialize};

use super::ids::{PoolId, ProductionId, SymbolId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrammarIr {
    pub productions: Vec<Production>,
    pub symbols: Vec<Symbol>,
    pub start: ProductionId,
    /// Token pool entries, indexed by [`PoolId`]. Each entry represents a lexer rule's
    /// expansion alternatives, used by [`TerminalKind::TokenPool`] during generation.
    /// Empty for grammars that don't use split lexer/parser files.
    #[serde(default)]
    pub token_pools: Vec<TokenPoolEntry>,
}

/// A token pool entry representing a lexer rule's expansion alternatives.
///
/// During generation, `TerminalKind::TokenPool(pool_id)` indexes into `GrammarIr::token_pools`
/// to find the pool entry, then uses a tape byte to select among the alternatives.
/// Semantic hooks can override this expansion via [`SemanticHooks::on_token_pool`](crate::hooks::SemanticHooks::on_token_pool).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPoolEntry {
    pub name: String,
    pub alternatives: Vec<Alternative>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Production {
    pub id: ProductionId,
    pub name: String,
    pub alternatives: Vec<Alternative>,
    pub attrs: ProductionAttrs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    pub symbols: Vec<SymbolRef>,
    pub weight: f32,
    pub semantic_tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRef {
    pub symbol: SymbolId,
    pub modifier: Modifier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Modifier {
    Once,
    Optional,
    ZeroOrMore { min: u32, max: u32 },
    OneOrMore { min: u32, max: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Symbol {
    Terminal(TerminalKind),
    NonTerminal(ProductionId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TerminalKind {
    Literal(Vec<u8>),
    CharClass { ranges: Vec<(u8, u8)>, negated: bool },
    AnyByte,
    ByteRange(u8, u8),
    TokenPool(PoolId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionAttrs {
    pub min_depth: u32,
    pub is_recursive: bool,
    pub token_kind: Option<String>,
    pub semantic_hook: Option<String>,
}

impl Default for ProductionAttrs {
    fn default() -> Self {
        Self {
            min_depth: u32::MAX,
            is_recursive: false,
            token_kind: None,
            semantic_hook: None,
        }
    }
}
