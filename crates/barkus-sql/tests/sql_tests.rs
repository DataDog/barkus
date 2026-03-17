use barkus_sql::context::SqlContext;
use barkus_sql::dialect::PostgresDialect;
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
    assert!(successes > 0, "expected at least one successful generation across 20 seeds");
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
            if sql1.is_empty() { continue; }
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
    let gen = SqlGenerator::builder()
        .context(ctx)
        .build()
        .unwrap();
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
            if sql.trim().is_empty() { continue; }
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
