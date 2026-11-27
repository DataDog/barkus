use barkus_antlr::compile_split;
use barkus_core::generate::generate;
use barkus_core::profile::Profile;
use barkus_parser_common::test_helpers::{generate_one, generate_seeded};
use rand::rngs::SmallRng;
use rand::SeedableRng;

// ── Minimal hand-written split grammar ──────────────────────────────────────

#[test]
fn minimal_split_grammar() {
    let lexer = r#"
        lexer grammar TestLexer;
        HELLO : 'hello' ;
        WORLD : 'world' ;
    "#;
    let parser = r#"
        parser grammar TestParser;
        options { tokenVocab = TestLexer; }
        start : HELLO WORLD ;
    "#;
    let ir = compile_split(lexer, parser).unwrap();
    assert_eq!(ir.productions[0].name, "start");
    let bytes = generate_one(&ir);
    // HELLO and WORLD are single-literal keyword rules → become literals.
    assert_eq!(bytes, b"helloworld");
}

#[test]
fn split_grammar_with_alternatives() {
    let lexer = r#"
        lexer grammar L;
        GREETING : 'hello' | 'hi' | 'hey' ;
        NAME : 'alice' | 'bob' ;
    "#;
    let parser = r#"
        parser grammar P;
        options { tokenVocab = L; }
        start : GREETING NAME ;
    "#;
    let ir = compile_split(lexer, parser).unwrap();
    // GREETING has multiple alternatives → becomes a token pool.
    assert!(ir.token_pools.iter().any(|p| p.name == "GREETING"));
    assert!(ir.token_pools.iter().any(|p| p.name == "NAME"));
    let bytes = generate_one(&ir);
    let s = String::from_utf8(bytes).unwrap();
    // Output should be one greeting + one name.
    let greetings = ["hello", "hi", "hey"];
    let names = ["alice", "bob"];
    let greeting_match = greetings.iter().find(|g| s.starts_with(*g));
    assert!(
        greeting_match.is_some(),
        "output should start with a greeting: {s}"
    );
    let rest = &s[greeting_match.unwrap().len()..];
    assert!(names.contains(&rest), "rest should be a name: {rest}");
}

// ── Fragment rules are inlined, not exposed as pools ────────────────────────

#[test]
fn fragment_rules_not_exposed() {
    let lexer = r#"
        lexer grammar L;
        fragment DIGIT : [0-9] ;
        NUMBER : DIGIT+ ;
        PLUS : '+' ;
    "#;
    let parser = r#"
        parser grammar P;
        options { tokenVocab = L; }
        start : NUMBER PLUS NUMBER ;
    "#;
    let ir = compile_split(lexer, parser).unwrap();
    // Fragment DIGIT should NOT appear as a token pool.
    assert!(!ir.token_pools.iter().any(|p| p.name == "DIGIT"));
    // NUMBER should appear as a token pool (it has a char class, not a simple literal).
    assert!(ir.token_pools.iter().any(|p| p.name == "NUMBER"));
}

// ── Skip tokens are excluded ────────────────────────────────────────────────

#[test]
fn skip_tokens_excluded() {
    let lexer = r#"
        lexer grammar L;
        WS : [ \t\r\n]+ -> skip ;
        WORD : [a-z]+ ;
    "#;
    let parser = r#"
        parser grammar P;
        options { tokenVocab = L; }
        start : WORD ;
    "#;
    let ir = compile_split(lexer, parser).unwrap();
    // WS should not appear as a production or token pool.
    assert!(!ir.token_pools.iter().any(|p| p.name == "WS"));
    assert!(!ir.productions.iter().any(|p| p.name == "WS"));
}

#[test]
fn channel_hidden_excluded() {
    let lexer = r#"
        lexer grammar L;
        COMMENT : '//' ~[\r\n]* -> channel(HIDDEN) ;
        WORD : [a-z]+ ;
    "#;
    let parser = r#"
        parser grammar P;
        options { tokenVocab = L; }
        start : WORD ;
    "#;
    let ir = compile_split(lexer, parser).unwrap();
    assert!(!ir.token_pools.iter().any(|p| p.name == "COMMENT"));
}

// ── Keyword literals ────────────────────────────────────────────────────────

#[test]
fn single_literal_lexer_rule_becomes_literal() {
    let lexer = r#"
        lexer grammar L;
        SELECT : 'SELECT' ;
        FROM : 'FROM' ;
        STAR : '*' ;
        ID : [a-zA-Z_]+ ;
    "#;
    let parser = r#"
        parser grammar P;
        options { tokenVocab = L; }
        start : SELECT STAR FROM ID ;
    "#;
    let ir = compile_split(lexer, parser).unwrap();
    // SELECT, FROM, STAR are keyword literals — no pool needed.
    assert!(!ir.token_pools.iter().any(|p| p.name == "SELECT"));
    assert!(!ir.token_pools.iter().any(|p| p.name == "FROM"));
    assert!(!ir.token_pools.iter().any(|p| p.name == "STAR"));
    // ID is multi-alternative → pool.
    assert!(ir.token_pools.iter().any(|p| p.name == "ID"));
    let bytes = generate_one(&ir);
    let s = String::from_utf8(bytes).unwrap();
    assert!(
        s.starts_with("SELECT*FROM"),
        "expected SELECT*FROM..., got: {s}"
    );
}

// ── Deterministic generation ────────────────────────────────────────────────

#[test]
fn deterministic_from_seed() {
    let lexer = r#"
        lexer grammar L;
        WORD : [a-z]+ ;
        NUM : [0-9]+ ;
    "#;
    let parser = r#"
        parser grammar P;
        options { tokenVocab = L; }
        start : WORD NUM ;
    "#;
    let ir = compile_split(lexer, parser).unwrap();
    let a = generate_seeded(&ir, 123);
    let b = generate_seeded(&ir, 123);
    assert_eq!(a, b, "same seed should produce same output");
    let c = generate_seeded(&ir, 456);
    // Different seed may produce different output (not guaranteed but very likely).
    // Just check both are non-empty.
    assert!(!a.is_empty());
    assert!(!c.is_empty());
}

// ── Split grammar header variations ─────────────────────────────────────────

#[test]
fn parser_grammar_header_only() {
    let lexer = "lexer grammar L; HELLO : 'hello' ;";
    let parser = "parser grammar P; start : HELLO ;";
    let ir = compile_split(lexer, parser).unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

#[test]
fn options_block_skipped() {
    let lexer = r#"
        lexer grammar L;
        options { caseInsensitive = true; }
        HELLO : 'hello' ;
    "#;
    let parser = r#"
        parser grammar P;
        options { tokenVocab = L; }
        start : HELLO ;
    "#;
    let ir = compile_split(lexer, parser).unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

// ── Vendored SQLite grammar integration ─────────────────────────────────────

#[test]
fn sqlite_grammar_compiles() {
    let lexer_src = include_str!("../../../grammars/sqlite/SQLiteLexer.g4");
    let parser_src = include_str!("../../../grammars/sqlite/SQLiteParser.g4");
    let ir = compile_split(lexer_src, parser_src).unwrap();
    // Basic sanity: should have multiple productions and some token pools.
    assert!(
        ir.productions.len() > 10,
        "expected many productions, got {}",
        ir.productions.len()
    );
    assert!(
        !ir.token_pools.is_empty(),
        "expected token pools from lexer rules"
    );
    // The start rule should be 'parse' (the first lowercase rule in SQLiteParser.g4).
    assert_eq!(ir.productions[ir.start.0 as usize].name, "parse");
}

#[test]
fn sqlite_grammar_generates() {
    let lexer_src = include_str!("../../../grammars/sqlite/SQLiteLexer.g4");
    let parser_src = include_str!("../../../grammars/sqlite/SQLiteParser.g4");
    let ir = compile_split(lexer_src, parser_src).unwrap();
    // Generate with many seeds — at least some should produce non-empty output.
    // Use a higher max_depth since real SQL grammars are deeply recursive.
    let profile = Profile {
        max_depth: 80,
        ..Default::default()
    };
    let mut non_empty = 0;
    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        match generate(&ir, &profile, &mut rng) {
            Ok((ast, _tape, _map)) => {
                let bytes = ast.serialize();
                if !bytes.is_empty() {
                    non_empty += 1;
                }
            }
            Err(_) => {
                // Some seeds may still hit budget limits — that's OK.
            }
        }
    }
    assert!(
        non_empty > 0,
        "expected at least one non-empty output across 20 seeds"
    );
}

#[test]
fn same_line_skip_does_not_skip_earlier_rule() {
    let lexer = "lexer grammar L;\nTOK : 'hello'; WS : ' ' -> skip;\n";
    let parser = "parser grammar P;\noptions { tokenVocab = L; }\nstart : TOK ;\n";
    let ir = compile_split(lexer, parser).unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

#[test]
fn fragment_inlining_populates_pool_and_generates_digits() {
    let lexer = r#"
        lexer grammar L;
        fragment DIGIT : [0-9] ;
        NUM : DIGIT+ ;
    "#;
    let parser = r#"
        parser grammar P;
        options { tokenVocab = L; }
        start : NUM ;
    "#;
    let ir = compile_split(lexer, parser).unwrap();
    assert!(!ir.token_pools.iter().any(|p| p.name == "DIGIT"));
    let num_pool = ir
        .token_pools
        .iter()
        .find(|p| p.name == "NUM")
        .expect("NUM pool should exist");
    assert!(
        !num_pool.alternatives.is_empty(),
        "NUM pool should have inlined alternatives"
    );
    for seed in 0..20 {
        let bytes = generate_seeded(&ir, seed);
        assert!(!bytes.is_empty(), "NUM should generate non-empty output");
        assert!(
            bytes.iter().all(u8::is_ascii_digit),
            "expected digits, got {:?}",
            String::from_utf8_lossy(&bytes)
        );
    }
}

#[test]
fn recursive_fragments_terminate() {
    let lexer = r#"
        lexer grammar L;
        fragment A : B 'x' ;
        fragment B : A | 'y' ;
        TOK : A ;
    "#;
    let parser = r#"
        parser grammar P;
        options { tokenVocab = L; }
        start : TOK ;
    "#;
    let ir = compile_split(lexer, parser).unwrap();
    let tok_pool = ir
        .token_pools
        .iter()
        .find(|p| p.name == "TOK")
        .expect("TOK pool should exist");
    assert!(!tok_pool.alternatives.is_empty());
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"yx");
}

#[test]
fn vendored_split_grammars_compile_with_inlined_fragments() {
    let cases: &[(&str, &str, &str, &[&str])] = &[
        (
            "sqlite",
            include_str!("../../../grammars/sqlite/SQLiteLexer.g4"),
            include_str!("../../../grammars/sqlite/SQLiteParser.g4"),
            &["NUMERIC_LITERAL", "IDENTIFIER"],
        ),
        (
            "trino",
            include_str!("../../../grammars/trino/TrinoLexer.g4"),
            include_str!("../../../grammars/trino/TrinoParser.g4"),
            &["INTEGER_VALUE_", "IDENTIFIER_"],
        ),
        (
            "mysql",
            include_str!("../../../grammars/mysql/MySQLLexer.g4"),
            include_str!("../../../grammars/mysql/MySQLParser.g4"),
            &["IDENTIFIER", "SELECT_SYMBOL"],
        ),
        (
            "postgresql",
            include_str!("../../../grammars/postgresql/PostgreSQLLexer.g4"),
            include_str!("../../../grammars/postgresql/PostgreSQLParser.g4"),
            &["Identifier", "Integral"],
        ),
        (
            "html",
            include_str!("../../../grammars/html/HTMLLexer.g4"),
            include_str!("../../../grammars/html/HTMLParser.g4"),
            &["TAG_NAME"],
        ),
        (
            "css3",
            include_str!("../../../grammars/css3/css3Lexer.g4"),
            include_str!("../../../grammars/css3/css3Parser.g4"),
            &["Ident", "FontFace"],
        ),
    ];

    for &(name, lexer_src, parser_src, expected_non_empty_pools) in cases {
        let ir = compile_split(lexer_src, parser_src)
            .unwrap_or_else(|e| panic!("{name} failed to compile_split: {e}"));
        assert!(
            !ir.token_pools.is_empty(),
            "{name} should produce token pools"
        );
        for &pool_name in expected_non_empty_pools {
            let pool = ir
                .token_pools
                .iter()
                .find(|p| p.name == pool_name)
                .unwrap_or_else(|| panic!("{name}: expected token pool {pool_name}"));
            assert!(
                !pool.alternatives.is_empty(),
                "{name}: token pool {pool_name} should have non-empty alternatives"
            );
        }
    }
}
