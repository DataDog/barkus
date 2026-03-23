pub mod context;
pub mod dialect;
pub mod hooks;

use barkus_antlr::compile_split;
use barkus_core::generate::{decode_with_hooks, generate_with_hooks};
use barkus_core::ir::GrammarIr;
use barkus_core::profile::Profile;
use barkus_core::tape::map::TapeMap;
use barkus_core::tape::DecisionTape;
use rand::Rng;

use context::SqlContext;
use dialect::SqliteDialect;
use hooks::{HookMetadata, SqlHooks};

/// High-level SQL generator combining grammar, profile, and semantic hooks.
pub struct SqlGenerator {
    grammar: GrammarIr,
    profile: Profile,
    context: SqlContext,
    dialect: Box<dyn dialect::SqlDialect>,
    /// Precomputed hook metadata (pool kinds, prod kinds, flat column list).
    metadata: HookMetadata,
}

/// Error type for SQL generation.
#[derive(Debug)]
pub enum SqlError {
    Grammar(barkus_antlr::ParseError),
    Generate(barkus_core::error::GenerateError),
}

impl std::fmt::Display for SqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SqlError::Grammar(e) => write!(f, "grammar error: {e}"),
            SqlError::Generate(e) => write!(f, "generation error: {e}"),
        }
    }
}

impl std::error::Error for SqlError {}

impl From<barkus_antlr::ParseError> for SqlError {
    fn from(e: barkus_antlr::ParseError) -> Self {
        SqlError::Grammar(e)
    }
}

impl From<barkus_core::error::GenerateError> for SqlError {
    fn from(e: barkus_core::error::GenerateError) -> Self {
        SqlError::Generate(e)
    }
}

impl SqlGenerator {
    /// Create a new SQL generator with default settings (SQLite grammar, synthetic schema).
    pub fn new() -> Result<Self, SqlError> {
        SqlGeneratorBuilder::new().build()
    }

    /// Create a builder for customized SQL generation.
    pub fn builder() -> SqlGeneratorBuilder {
        SqlGeneratorBuilder::new()
    }

    /// Generate a SQL string from random input.
    pub fn generate(
        &self,
        rng: &mut impl Rng,
    ) -> Result<(String, DecisionTape, TapeMap), SqlError> {
        let mut hooks = SqlHooks::new(
            &self.context,
            &*self.dialect,
            &self.metadata.pool_kinds,
            &self.metadata.prod_kinds,
            &self.metadata.all_columns,
        );
        let (ast, tape, map) = generate_with_hooks(
            &self.grammar,
            self.grammar.start,
            &self.profile,
            rng,
            &mut hooks,
        )?;
        let bytes = ast.serialize();
        let sql = String::from_utf8_lossy(&bytes).into_owned();
        Ok((sql, tape, map))
    }

    /// Decode a SQL string from a previously recorded tape.
    pub fn decode(&self, tape: &DecisionTape) -> Result<(String, TapeMap), SqlError> {
        let mut hooks = SqlHooks::new(
            &self.context,
            &*self.dialect,
            &self.metadata.pool_kinds,
            &self.metadata.prod_kinds,
            &self.metadata.all_columns,
        );
        let (ast, map) = decode_with_hooks(&self.grammar, &self.profile, &tape.bytes, &mut hooks)?;
        let bytes = ast.serialize();
        let sql = String::from_utf8_lossy(&bytes).into_owned();
        Ok((sql, map))
    }
}

impl Default for SqlGenerator {
    fn default() -> Self {
        Self::new().expect("default SQL generator should compile successfully")
    }
}

/// Builder for configuring a [`SqlGenerator`].
pub struct SqlGeneratorBuilder {
    context: Option<SqlContext>,
    dialect: Option<Box<dyn dialect::SqlDialect>>,
    profile: Option<Profile>,
    lexer_source: Option<String>,
    parser_source: Option<String>,
}

impl SqlGeneratorBuilder {
    pub fn new() -> Self {
        SqlGeneratorBuilder {
            context: None,
            dialect: None,
            profile: None,
            lexer_source: None,
            parser_source: None,
        }
    }

    /// Set the schema context.
    pub fn context(mut self, ctx: SqlContext) -> Self {
        self.context = Some(ctx);
        self
    }

    /// Set the SQL dialect.
    pub fn dialect(mut self, d: impl dialect::SqlDialect + 'static) -> Self {
        self.dialect = Some(Box::new(d));
        self
    }

    /// Set the generation profile.
    pub fn profile(mut self, p: Profile) -> Self {
        self.profile = Some(p);
        self
    }

    /// Set custom grammar sources (ANTLR split grammar).
    pub fn grammar(mut self, lexer_source: &str, parser_source: &str) -> Self {
        self.lexer_source = Some(lexer_source.to_string());
        self.parser_source = Some(parser_source.to_string());
        self
    }

    /// Build the generator, compiling the grammar.
    pub fn build(self) -> Result<SqlGenerator, SqlError> {
        let lexer_src = self.lexer_source.unwrap_or_else(|| {
            include_str!("../../../grammars/antlr-sql/sqlite/SQLiteLexer.g4").to_string()
        });
        let parser_src = self.parser_source.unwrap_or_else(|| {
            include_str!("../../../grammars/antlr-sql/sqlite/SQLiteParser.g4").to_string()
        });

        let grammar = compile_split(&lexer_src, &parser_src)?;

        let dialect: Box<dyn dialect::SqlDialect> =
            self.dialect.unwrap_or_else(|| Box::new(SqliteDialect));
        let context = self.context.unwrap_or_else(SqlContext::synthetic);
        let mut profile = self.profile.unwrap_or_default();
        // Real SQL grammars are deeply recursive; increase depth budget.
        if profile.max_depth < 80 {
            profile.max_depth = 80;
        }

        let metadata = HookMetadata::new(&grammar, &context);

        Ok(SqlGenerator {
            grammar,
            profile,
            context,
            dialect,
            metadata,
        })
    }
}

impl Default for SqlGeneratorBuilder {
    fn default() -> Self {
        Self::new()
    }
}
