use barkus_core::hooks::SemanticHooks;
use barkus_core::ir::ids::{PoolId, ProductionId};

use crate::context::SqlContext;
use crate::dialect::SqlDialect;

/// Pre-classified pool kind, computed once at construction to avoid
/// per-call string matching and allocation in on_token_pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolKind {
    Identifier,
    NumericLiteral,
    StringLiteral,
    Other,
}

/// Classify a pool name (from a lexer rule) into a semantic category.
fn classify_pool(name: &str) -> PoolKind {
    // Pool names vary across ANTLR grammars: UPPER_CASE (SQLite, MySQL),
    // UPPER_CASE_ with trailing underscore (Trino), or PascalCase (PostgreSQL).
    // Normalize by stripping trailing underscores and uppercasing.
    let upper = name.trim_end_matches('_').to_ascii_uppercase();
    match upper.as_str() {
        // Generic / SQLite identifiers
        "IDENTIFIER" | "ID" | "SIMPLE_ID" | "GENERAL_ID" => PoolKind::Identifier,
        // PostgreSQL identifiers
        "QUOTEDIDENTIFIER" | "UNICODEQUOTEDIDENTIFIER" => PoolKind::Identifier,
        // Trino identifiers (trailing `_` stripped)
        "QUOTED_IDENTIFIER" | "BACKQUOTED_IDENTIFIER" | "DIGIT_IDENTIFIER" => PoolKind::Identifier,
        // MySQL identifiers
        "BACK_TICK_QUOTED_ID" => PoolKind::Identifier,

        // Generic / SQLite numeric literals
        "NUMERIC_LITERAL" | "NUMBER_LITERAL" | "INTEGER_LITERAL" => PoolKind::NumericLiteral,
        "DECIMAL_LITERAL" | "REAL_NUMBER" => PoolKind::NumericLiteral,
        // MySQL numeric literals
        "INT_NUMBER" | "DECIMAL_NUMBER" | "FLOAT_NUMBER" => PoolKind::NumericLiteral,
        // PostgreSQL numeric literals
        "NUMERIC" | "INTEGRAL" => PoolKind::NumericLiteral,
        // Trino numeric literals (trailing `_` stripped)
        "INTEGER_VALUE" | "DECIMAL_VALUE" | "DOUBLE_VALUE" => PoolKind::NumericLiteral,

        // Generic / SQLite string literals
        "STRING_LITERAL" | "DQSTRING_LITERAL" | "SQSTRING_LITERAL" => PoolKind::StringLiteral,
        // PostgreSQL string literals
        "STRINGCONSTANT" | "ESCAPESTRINGCONSTANT" => PoolKind::StringLiteral,
        "UNICODEESCAPESTRINGCONSTANT" => PoolKind::StringLiteral,
        // Trino string literals (trailing `_` stripped)
        "STRING" | "UNICODE_STRING" => PoolKind::StringLiteral,
        // MySQL string literals
        "SINGLE_QUOTED_TEXT" | "DOUBLE_QUOTED_TEXT" | "NCHAR_TEXT" => PoolKind::StringLiteral,

        _ => PoolKind::Other,
    }
}

/// Pre-classified production kind for scope tracking.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProdKind {
    ScopeProducer, // FROM/JOIN-like: pushes a scope marker
    Other,
}

fn classify_prod(name: &str) -> ProdKind {
    // Match on exact known production names rather than substring to avoid
    // false positives (e.g., "transform" matching "from").
    if name.ends_with("from_clause")
        || name.ends_with("join_clause")
        || name == "from_clause"
        || name == "join_clause"
        || name == "from_item"
        || name == "table_or_subquery"
    {
        ProdKind::ScopeProducer
    } else {
        ProdKind::Other
    }
}

/// SQL-aware semantic hooks for barkus-core's generation pipeline.
///
/// Overrides token pool expansion to inject real identifiers and type-appropriate
/// literals from the schema context. Tracks production scope for future semantic
/// enhancements (e.g., column resolution from FROM clause tables).
pub struct SqlHooks<'a> {
    context: &'a SqlContext,
    dialect: &'a dyn SqlDialect,
    pool_kinds: &'a [PoolKind],
    prod_kinds: &'a [ProdKind],
    /// Precomputed flat list of all column names across all tables.
    all_columns: &'a [String],
    /// Stack of scope markers (for future scope-aware column resolution).
    scope_depth: u32,
}

impl<'a> SqlHooks<'a> {
    pub(crate) fn new(
        context: &'a SqlContext,
        dialect: &'a dyn SqlDialect,
        pool_kinds: &'a [PoolKind],
        prod_kinds: &'a [ProdKind],
        all_columns: &'a [String],
    ) -> Self {
        SqlHooks {
            context,
            dialect,
            pool_kinds,
            prod_kinds,
            all_columns,
            scope_depth: 0,
        }
    }

    fn pick_table_name(&self, tape_byte: u8) -> Option<Vec<u8>> {
        if self.context.tables.is_empty() {
            return None;
        }
        let idx = tape_byte as usize % self.context.tables.len();
        let name = &self.context.tables[idx].name;
        Some(self.dialect.quote_identifier(name).into_bytes())
    }

    fn pick_column_name(&self, tape_byte: u8) -> Option<Vec<u8>> {
        if self.all_columns.is_empty() {
            return None;
        }
        let idx = tape_byte as usize % self.all_columns.len();
        Some(
            self.dialect
                .quote_identifier(&self.all_columns[idx])
                .into_bytes(),
        )
    }

    fn pick_number(&self, tape_byte: u8) -> Vec<u8> {
        format!("{}", tape_byte as u32 * 7 + 1).into_bytes()
    }

    fn pick_string_literal(&self, tape_byte: u8) -> Vec<u8> {
        let words = ["foo", "bar", "baz", "qux", "hello", "world", "test", "data"];
        let idx = tape_byte as usize % words.len();
        self.dialect.string_literal(words[idx]).into_bytes()
    }
}

impl SemanticHooks for SqlHooks<'_> {
    fn on_token_pool(&mut self, pool_id: PoolId, tape_byte: u8) -> Option<Vec<u8>> {
        let kind = self
            .pool_kinds
            .get(pool_id.0 as usize)
            .copied()
            .unwrap_or(PoolKind::Other);
        match kind {
            PoolKind::Identifier => {
                // Use bit 0 to select table vs. column and the upper 7 bits
                // as the index so even-length table/column lists have full
                // reachability without parity bias.
                let selector = tape_byte >> 1;
                if tape_byte.is_multiple_of(2) {
                    self.pick_table_name(selector)
                } else {
                    self.pick_column_name(selector)
                }
            }
            PoolKind::NumericLiteral => Some(self.pick_number(tape_byte)),
            PoolKind::StringLiteral => Some(self.pick_string_literal(tape_byte)),
            PoolKind::Other => None,
        }
    }

    fn enter_production(&mut self, prod_id: ProductionId) {
        if let Some(&ProdKind::ScopeProducer) = self.prod_kinds.get(prod_id.0 as usize) {
            self.scope_depth += 1;
        }
    }

    fn exit_production(&mut self, prod_id: ProductionId) {
        if let Some(&ProdKind::ScopeProducer) = self.prod_kinds.get(prod_id.0 as usize) {
            self.scope_depth = self.scope_depth.saturating_sub(1);
        }
    }
}

/// Precomputed metadata cached on SqlGenerator to avoid per-call allocation.
pub(crate) struct HookMetadata {
    pub pool_kinds: Vec<PoolKind>,
    pub prod_kinds: Vec<ProdKind>,
    pub all_columns: Vec<String>,
}

impl HookMetadata {
    pub fn new(grammar: &barkus_core::ir::GrammarIr, context: &SqlContext) -> Self {
        let pool_kinds: Vec<PoolKind> = grammar
            .token_pools
            .iter()
            .map(|p| classify_pool(&p.name))
            .collect();
        let prod_kinds: Vec<ProdKind> = grammar
            .productions
            .iter()
            .map(|p| classify_prod(&p.name))
            .collect();
        let all_columns: Vec<String> = context
            .tables
            .iter()
            .flat_map(|t| t.columns.iter().map(|c| c.name.clone()))
            .collect();
        HookMetadata {
            pool_kinds,
            prod_kinds,
            all_columns,
        }
    }
}
