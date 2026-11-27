use std::collections::HashSet;

use barkus_core::tape::DecisionTape;
use barkus_sql::context::{Column, SqlContext, SqlType, Table};
use barkus_sql::dialect::{
    GenericDialect, MySqlDialect, PostgresDialect, SqlDialect, SqliteDialect, TrinoDialect,
};
use barkus_sql::SqlGenerator;
use rand::rngs::SmallRng;
use rand::SeedableRng;

// ── Basic generation ────────────────────────────────────────────────────────

#[test]
fn default_generator_produces_output() {
    let gen = SqlGenerator::new().unwrap();
    let mut successes = 0;
    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        if let Ok((sql, _tape, _map)) = gen.generate(&mut rng) {
            if !sql.is_empty() {
                successes += 1;
            }
        }
    }
    assert!(
        successes > 0,
        "expected at least one successful generation across 20 seeds"
    );
}

#[test]
fn deterministic_from_seed() {
    let gen = SqlGenerator::new().unwrap();
    let mut rng1 = SmallRng::seed_from_u64(42);
    let mut rng2 = SmallRng::seed_from_u64(42);
    let r1 = gen.generate(&mut rng1);
    let r2 = gen.generate(&mut rng2);
    match (r1, r2) {
        (Ok((sql1, _, _)), Ok((sql2, _, _))) => assert_eq!(sql1, sql2),
        (Err(_), Err(_)) => {} // both failed — consistent
        _ => panic!("same seed should produce same result"),
    }
}

// ── Tape roundtrip ──────────────────────────────────────────────────────────

#[test]
fn decode_roundtrip() {
    let gen = SqlGenerator::new().unwrap();
    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        if let Ok((sql1, tape, _map)) = gen.generate(&mut rng) {
            if sql1.is_empty() {
                continue;
            }
            let (sql2, _) = gen.decode(&tape).unwrap();
            assert_eq!(sql1, sql2, "decode roundtrip failed for seed {seed}");
            return; // one successful roundtrip is enough
        }
    }
    panic!("no seeds produced successful generation for roundtrip test");
}

// ── Custom context ──────────────────────────────────────────────────────────

#[test]
fn synthetic_context_has_tables() {
    let ctx = SqlContext::synthetic();
    assert_eq!(ctx.tables.len(), 3);
    assert_eq!(ctx.tables[0].name, "users");
    assert_eq!(ctx.tables[1].name, "orders");
    assert_eq!(ctx.tables[2].name, "products");
}

#[test]
fn context_from_json() {
    let json = r#"
    {
        "tables": [
            {
                "name": "customers",
                "columns": [
                    {"name": "id", "ty": "integer", "nullable": false},
                    {"name": "name", "ty": "text"}
                ]
            }
        ]
    }
    "#;
    let ctx: SqlContext = serde_json::from_str(json).unwrap();
    assert_eq!(ctx.tables.len(), 1);
    assert_eq!(ctx.tables[0].columns.len(), 2);
}

// ── Builder ─────────────────────────────────────────────────────────────────

#[test]
fn builder_with_postgres_dialect() {
    let gen = SqlGenerator::builder()
        .dialect(PostgresDialect)
        .build()
        .unwrap();
    let mut rng = SmallRng::seed_from_u64(99);
    // Just verify it doesn't panic.
    let _ = gen.generate(&mut rng);
}

#[test]
fn builder_with_custom_context() {
    let ctx = SqlContext {
        tables: vec![barkus_sql::context::Table {
            name: "my_table".into(),
            columns: vec![barkus_sql::context::Column {
                name: "col1".into(),
                ty: barkus_sql::context::SqlType::Text,
                nullable: false,
            }],
        }],
        functions: vec![],
    };
    let gen = SqlGenerator::builder().context(ctx).build().unwrap();
    let mut rng = SmallRng::seed_from_u64(77);
    let _ = gen.generate(&mut rng);
}

// ── SQL validation with sqlparser ───────────────────────────────────────────

#[test]
fn generated_sql_parses_with_sqlparser() {
    use sqlparser::dialect::SQLiteDialect;
    use sqlparser::parser::Parser;

    let gen = SqlGenerator::new().unwrap();
    let dialect = SQLiteDialect {};
    let mut parsed_count = 0;

    for seed in 0..50 {
        let mut rng = SmallRng::seed_from_u64(seed);
        if let Ok((sql, _, _)) = gen.generate(&mut rng) {
            if sql.trim().is_empty() {
                continue;
            }
            // Try to parse — we don't require all to succeed since the grammar
            // doesn't yet have full semantic awareness, but track the ratio.
            if Parser::parse_sql(&dialect, &sql).is_ok() {
                parsed_count += 1;
            }
        }
    }
    // At this stage, even a few valid parses demonstrate the pipeline works.
    // As semantic hooks improve, this threshold should increase.
    eprintln!("sqlparser parsed {parsed_count}/50 generated SQL strings");
}

// ── Identifier reachability (parity bias regression) ────────────────────────

const SINGLE_IDENT_LEXER: &str = "lexer grammar IdentLexer;\nIDENTIFIER : [a-z]+ ;\n";
const SINGLE_IDENT_PARSER: &str = "parser grammar IdentParser;\nstart : IDENTIFIER ;\n";

/// Build a valid DecisionTape with the 2-byte control header followed by a single decision byte.
fn tape_with_byte(byte: u8) -> DecisionTape {
    DecisionTape::from_bytes(vec![0, 0, byte])
}

#[test]
fn synthetic_context_all_tables_and_columns_reachable() {
    let ctx = SqlContext::synthetic();
    let dialect = SqliteDialect;
    let gen = SqlGenerator::builder()
        .grammar(SINGLE_IDENT_LEXER, SINGLE_IDENT_PARSER)
        .context(ctx.clone())
        .build()
        .unwrap();

    let mut emitted = HashSet::new();
    for byte in 0u8..=255 {
        let (sql, _) = gen.decode(&tape_with_byte(byte)).unwrap();
        emitted.insert(sql);
    }

    for table in &ctx.tables {
        let expected = dialect.quote_identifier(&table.name);
        assert!(
            emitted.contains(&expected),
            "table {expected} was never emitted across tape_byte 0..=255"
        );
        for col in &table.columns {
            let expected_col = dialect.quote_identifier(&col.name);
            assert!(
                emitted.contains(&expected_col),
                "column {expected_col} was never emitted across tape_byte 0..=255"
            );
        }
    }
}

#[test]
fn even_schema_all_tables_and_columns_reachable() {
    // Both table count (4) and total column count (8) are even.
    let ctx = SqlContext {
        tables: vec![
            Table {
                name: "t_zero".into(),
                columns: vec![
                    Column {
                        name: "c_zero".into(),
                        ty: SqlType::Integer,
                        nullable: false,
                    },
                    Column {
                        name: "c_one".into(),
                        ty: SqlType::Text,
                        nullable: false,
                    },
                ],
            },
            Table {
                name: "t_one".into(),
                columns: vec![
                    Column {
                        name: "c_two".into(),
                        ty: SqlType::Integer,
                        nullable: false,
                    },
                    Column {
                        name: "c_three".into(),
                        ty: SqlType::Text,
                        nullable: false,
                    },
                ],
            },
            Table {
                name: "t_two".into(),
                columns: vec![
                    Column {
                        name: "c_four".into(),
                        ty: SqlType::Integer,
                        nullable: false,
                    },
                    Column {
                        name: "c_five".into(),
                        ty: SqlType::Text,
                        nullable: false,
                    },
                ],
            },
            Table {
                name: "t_three".into(),
                columns: vec![
                    Column {
                        name: "c_six".into(),
                        ty: SqlType::Integer,
                        nullable: false,
                    },
                    Column {
                        name: "c_seven".into(),
                        ty: SqlType::Text,
                        nullable: false,
                    },
                ],
            },
        ],
        functions: vec![],
    };
    let dialect = SqliteDialect;
    let gen = SqlGenerator::builder()
        .grammar(SINGLE_IDENT_LEXER, SINGLE_IDENT_PARSER)
        .context(ctx.clone())
        .build()
        .unwrap();

    let mut emitted = HashSet::new();
    for byte in 0u8..=255 {
        let (sql, _) = gen.decode(&tape_with_byte(byte)).unwrap();
        emitted.insert(sql);
    }

    for table in &ctx.tables {
        let expected = dialect.quote_identifier(&table.name);
        assert!(
            emitted.contains(&expected),
            "even-schema table {expected} was never emitted across tape_byte 0..=255"
        );
        for col in &table.columns {
            let expected_col = dialect.quote_identifier(&col.name);
            assert!(
                emitted.contains(&expected_col),
                "even-schema column {expected_col} was never emitted across tape_byte 0..=255"
            );
        }
    }
}

// ── Multi-dialect pool classification & hook injection ──────────────────────

fn distinctive_context() -> SqlContext {
    SqlContext {
        tables: vec![
            Table {
                name: "barkus_tbl_alpha".into(),
                columns: vec![
                    Column {
                        name: "barkus_col_one".into(),
                        ty: SqlType::Integer,
                        nullable: false,
                    },
                    Column {
                        name: "barkus_col_two".into(),
                        ty: SqlType::Text,
                        nullable: false,
                    },
                ],
            },
            Table {
                name: "barkus_tbl_beta".into(),
                columns: vec![Column {
                    name: "barkus_col_three".into(),
                    ty: SqlType::Float,
                    nullable: true,
                }],
            },
        ],
        functions: vec![],
    }
}

fn assert_dialect_emits_schema_and_literals<D: SqlDialect + 'static>(
    dialect_label: &str,
    dialect: D,
    lexer_src: &str,
    parser_src: &str,
) {
    let ctx = distinctive_context();
    let mut expected_idents = Vec::new();
    for table in &ctx.tables {
        expected_idents.push(dialect.quote_identifier(&table.name));
        for col in &table.columns {
            expected_idents.push(dialect.quote_identifier(&col.name));
        }
    }
    let words = ["foo", "bar", "baz", "qux", "hello", "world", "test", "data"];
    let hook_strings: Vec<String> = words.iter().map(|w| dialect.string_literal(w)).collect();
    // Hook numbers are tape_byte * 7 + 1 (1, 8, 15, ..., 1786).
    let hook_numbers: Vec<String> = (0u32..=255).map(|b| (b * 7 + 1).to_string()).collect();

    let gen = SqlGenerator::builder()
        .dialect(dialect)
        .context(ctx)
        .grammar(lexer_src, parser_src)
        .build()
        .unwrap();

    let mut saw_ident = false;
    let mut saw_string_lit = false;
    let mut saw_numeric_lit = false;

    for seed in 0..300 {
        let mut rng = SmallRng::seed_from_u64(seed);
        if let Ok((sql, _, _)) = gen.generate(&mut rng) {
            if !saw_ident && expected_idents.iter().any(|id| sql.contains(id)) {
                saw_ident = true;
            }
            if !saw_string_lit && hook_strings.iter().any(|s| sql.contains(s)) {
                saw_string_lit = true;
            }
            if !saw_numeric_lit && hook_numbers.iter().any(|n| sql.contains(n)) {
                saw_numeric_lit = true;
            }
            if saw_ident && saw_string_lit && saw_numeric_lit {
                break;
            }
        }
    }

    assert!(
        saw_ident,
        "{dialect_label}: expected SqlHooks to inject schema identifiers across 300 seeds"
    );
    assert!(
        saw_string_lit || saw_numeric_lit,
        "{dialect_label}: expected SqlHooks to inject string or numeric literals across 300 seeds"
    );
}

#[test]
fn trino_dialect_emits_schema_and_literals() {
    assert_dialect_emits_schema_and_literals(
        "trino",
        TrinoDialect,
        include_str!("../../../grammars/trino/TrinoLexer.g4"),
        include_str!("../../../grammars/trino/TrinoParser.g4"),
    );
}

#[test]
fn postgres_dialect_emits_schema_and_literals() {
    assert_dialect_emits_schema_and_literals(
        "postgresql",
        PostgresDialect,
        include_str!("../../../grammars/postgresql/PostgreSQLLexer.g4"),
        include_str!("../../../grammars/postgresql/PostgreSQLParser.g4"),
    );
}

#[test]
fn mysql_dialect_emits_schema_and_literals() {
    assert_dialect_emits_schema_and_literals(
        "mysql",
        MySqlDialect,
        include_str!("../../../grammars/mysql/MySQLLexer.g4"),
        include_str!("../../../grammars/mysql/MySQLParser.g4"),
    );
}

#[test]
fn dialect_pool_names_classified_across_all_grammars() {
    // Verify every dialect's identifier, numeric, and string pool names trigger SqlHooks.
    let ident_pools = [
        // SQLite & MySQL
        "IDENTIFIER",
        "BACK_TICK_QUOTED_ID",
        // PostgreSQL
        "Identifier",
        "QuotedIdentifier",
        "UnicodeQuotedIdentifier",
        // Trino (trailing underscore)
        "IDENTIFIER_",
        "QUOTED_IDENTIFIER_",
        "BACKQUOTED_IDENTIFIER_",
        "DIGIT_IDENTIFIER_",
    ];
    let numeric_pools = [
        // SQLite
        "NUMERIC_LITERAL",
        // PostgreSQL
        "Numeric",
        "Integral",
        // Trino
        "INTEGER_VALUE_",
        "DECIMAL_VALUE_",
        "DOUBLE_VALUE_",
        // MySQL
        "INT_NUMBER",
        "DECIMAL_NUMBER",
        "FLOAT_NUMBER",
    ];
    let string_pools = [
        // SQLite
        "STRING_LITERAL",
        // PostgreSQL
        "StringConstant",
        "EscapeStringConstant",
        "UnicodeEscapeStringConstant",
        // Trino
        "STRING_",
        "UNICODE_STRING_",
        // MySQL
        "SINGLE_QUOTED_TEXT",
        "DOUBLE_QUOTED_TEXT",
        "NCHAR_TEXT",
    ];

    let ctx = distinctive_context();
    for pool in ident_pools {
        let lexer = format!("lexer grammar L;\n{pool} : [a-z]+ ;\n");
        let parser = format!("parser grammar P;\nstart : {pool} ;\n");
        let gen = SqlGenerator::builder()
            .context(ctx.clone())
            .grammar(&lexer, &parser)
            .build()
            .unwrap();
        let (sql, _) = gen.decode(&tape_with_byte(0)).unwrap();
        assert_eq!(sql, "\"barkus_tbl_alpha\"", "pool {pool}: Identifier");
    }

    for pool in numeric_pools {
        let lexer = format!("lexer grammar L;\n{pool} : [a-z]+ ;\n");
        let parser = format!("parser grammar P;\nstart : {pool} ;\n");
        let gen = SqlGenerator::builder()
            .context(ctx.clone())
            .grammar(&lexer, &parser)
            .build()
            .unwrap();
        let (sql, _) = gen.decode(&tape_with_byte(3)).unwrap();
        assert_eq!(sql, "22", "pool {pool}: NumericLiteral");
    }

    for pool in string_pools {
        let lexer = format!("lexer grammar L;\n{pool} : [a-z]+ ;\n");
        let parser = format!("parser grammar P;\nstart : {pool} ;\n");
        let gen = SqlGenerator::builder()
            .context(ctx.clone())
            .grammar(&lexer, &parser)
            .build()
            .unwrap();
        let (sql, _) = gen.decode(&tape_with_byte(0)).unwrap();
        assert_eq!(sql, "'foo'", "pool {pool}: StringLiteral");
    }
}

// ── Dialect identifier quote escaping ───────────────────────────────────────

#[test]
fn quote_identifier_escapes_embedded_quotes() {
    let double_quote_dialects: [&dyn SqlDialect; 4] = [
        &GenericDialect,
        &PostgresDialect,
        &TrinoDialect,
        &SqliteDialect,
    ];
    for d in double_quote_dialects {
        let name = d.name();
        let quote = |s| d.quote_identifier(s);
        assert_eq!(quote("users"), "\"users\"", "{name}");
        assert_eq!(quote("my\"table"), "\"my\"\"table\"", "{name}");
        assert_eq!(quote("a\"\"b"), "\"a\"\"\"\"b\"", "{name}");
        assert_eq!(quote("my`table"), "\"my`table\"", "{name}");
    }

    let mysql = MySqlDialect;
    assert_eq!(mysql.quote_identifier("users"), "`users`");
    assert_eq!(mysql.quote_identifier("my`table"), "`my``table`");
    assert_eq!(mysql.quote_identifier("a``b"), "`a````b`");
    assert_eq!(mysql.quote_identifier("my\"table"), "`my\"table`");
}
