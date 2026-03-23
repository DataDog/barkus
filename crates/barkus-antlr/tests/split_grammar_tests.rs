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
    let lexer_src = include_str!("../../../grammars/antlr-sql/sqlite/SQLiteLexer.g4");
    let parser_src = include_str!("../../../grammars/antlr-sql/sqlite/SQLiteParser.g4");
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
    let lexer_src = include_str!("../../../grammars/antlr-sql/sqlite/SQLiteLexer.g4");
    let parser_src = include_str!("../../../grammars/antlr-sql/sqlite/SQLiteParser.g4");
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
