/// Dialect-specific SQL formatting rules.
///
/// Default implementations provide ANSI-standard behavior (double-quote identifiers,
/// single-quote strings, TRUE/FALSE booleans). Override only what differs.
pub trait SqlDialect: Send + Sync {
    fn name(&self) -> &str;

    fn quote_identifier(&self, name: &str) -> String {
        format!("\"{name}\"")
    }

    fn string_literal(&self, val: &str) -> String {
        format!("'{}'", val.replace('\'', "''"))
    }

    fn bool_literal(&self, val: bool) -> String {
        if val { "TRUE" } else { "FALSE" }.into()
    }
}

pub struct GenericDialect;
pub struct PostgresDialect;
pub struct TrinoDialect;
pub struct SqliteDialect;
pub struct MySqlDialect;

impl SqlDialect for GenericDialect {
    fn name(&self) -> &str {
        "generic"
    }
}

impl SqlDialect for PostgresDialect {
    fn name(&self) -> &str {
        "postgresql"
    }
}

impl SqlDialect for TrinoDialect {
    fn name(&self) -> &str {
        "trino"
    }
}

impl SqlDialect for SqliteDialect {
    fn name(&self) -> &str {
        "sqlite"
    }
    fn bool_literal(&self, val: bool) -> String {
        if val { "1" } else { "0" }.into()
    }
}

impl SqlDialect for MySqlDialect {
    fn name(&self) -> &str {
        "mysql"
    }
    /// MySQL identifiers are backtick-quoted in standard mode (and double-
    /// quoted only with ANSI_QUOTES sql_mode set, which is non-default).
    fn quote_identifier(&self, name: &str) -> String {
        format!("`{name}`")
    }
    /// MySQL accepts TRUE/FALSE keywords but resolves them to 1/0; emit
    /// 1/0 directly so generated literals match what mysql_parser wants
    /// to canonicalize into.
    fn bool_literal(&self, val: bool) -> String {
        if val { "1" } else { "0" }.into()
    }
}
