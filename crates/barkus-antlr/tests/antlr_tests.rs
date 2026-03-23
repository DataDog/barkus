use barkus_antlr::compile;
use barkus_core::ir::{Modifier, Symbol, TerminalKind};
use barkus_parser_common::test_helpers::{generate_one, generate_seeded};

// ── Parse + validate success cases ──────────────────────────────────────────

#[test]
fn simple_literal() {
    let ir = compile(
        r#"
        grammar Test;
        start : 'hello' ;
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions[0].name, "start");
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

#[test]
fn grammar_without_header() {
    let ir = compile("start : 'hello' ;").unwrap();
    assert_eq!(ir.productions[0].name, "start");
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

#[test]
fn choices() {
    let ir = compile(
        r#"
        grammar Test;
        start : 'a' | 'b' | 'c' ;
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions[0].alternatives.len(), 3);
    for seed in 0..20 {
        let bytes = generate_seeded(&ir, seed);
        let s = String::from_utf8(bytes).unwrap();
        assert!(
            s == "a" || s == "b" || s == "c",
            "unexpected output: {:?}",
            s
        );
    }
}

#[test]
fn character_class() {
    let ir = compile(
        r#"
        grammar Test;
        start : DIGIT ;
        DIGIT : [0-9] ;
    "#,
    )
    .unwrap();
    for seed in 0..20 {
        let bytes = generate_seeded(&ir, seed);
        assert_eq!(bytes.len(), 1);
        assert!(
            bytes[0] >= b'0' && bytes[0] <= b'9',
            "expected digit, got {:?}",
            bytes[0] as char
        );
    }
}

#[test]
fn negated_character_class() {
    let ir = compile(
        r#"
        grammar Test;
        start : NOTAB ;
        NOTAB : ~[ab] ;
    "#,
    )
    .unwrap();
    let sym = &ir.symbols[ir.productions[1].alternatives[0].symbols[0].symbol.0 as usize];
    match sym {
        Symbol::Terminal(TerminalKind::CharClass { negated, ranges }) => {
            assert!(negated);
            assert_eq!(ranges, &[(b'a', b'a'), (b'b', b'b')]);
        }
        other => panic!("expected negated char class, got {:?}", other),
    }
}

#[test]
fn quantifier_optional() {
    let ir = compile(
        r#"
        grammar Test;
        start : 'a' 'b'? 'c' ;
    "#,
    )
    .unwrap();
    let alt = &ir.productions[0].alternatives[0];
    assert_eq!(alt.symbols.len(), 3);
    assert!(matches!(alt.symbols[1].modifier, Modifier::Optional));

    let mut saw_with = false;
    let mut saw_without = false;
    for seed in 0..50 {
        let s = String::from_utf8(generate_seeded(&ir, seed)).unwrap();
        assert!(s == "ac" || s == "abc", "unexpected: {:?}", s);
        if s == "abc" {
            saw_with = true;
        } else {
            saw_without = true;
        }
    }
    assert!(saw_with, "never generated optional element");
    assert!(saw_without, "never omitted optional element");
}

#[test]
fn quantifier_zero_or_more() {
    let ir = compile(
        r#"
        grammar Test;
        start : 'a' 'b'* 'c' ;
    "#,
    )
    .unwrap();
    let alt = &ir.productions[0].alternatives[0];
    assert!(matches!(
        alt.symbols[1].modifier,
        Modifier::ZeroOrMore { .. }
    ));
    for seed in 0..20 {
        let s = String::from_utf8(generate_seeded(&ir, seed)).unwrap();
        assert!(s.starts_with('a'), "should start with 'a': {:?}", s);
        assert!(s.ends_with('c'), "should end with 'c': {:?}", s);
    }
}

#[test]
fn quantifier_one_or_more() {
    let ir = compile(
        r#"
        grammar Test;
        start : 'a' 'b'+ 'c' ;
    "#,
    )
    .unwrap();
    let alt = &ir.productions[0].alternatives[0];
    assert!(matches!(
        alt.symbols[1].modifier,
        Modifier::OneOrMore { .. }
    ));
    for seed in 0..20 {
        let s = String::from_utf8(generate_seeded(&ir, seed)).unwrap();
        assert!(s.starts_with('a'), "should start with 'a': {:?}", s);
        assert!(s.ends_with('c'), "should end with 'c': {:?}", s);
        let middle = &s[1..s.len() - 1];
        assert!(
            !middle.is_empty(),
            "one-or-more should produce at least one 'b'"
        );
        assert!(
            middle.chars().all(|c| c == 'b'),
            "middle should be all 'b': {:?}",
            middle
        );
    }
}

#[test]
fn lexer_rules() {
    let ir = compile(
        r#"
        grammar Test;
        start : ID ;
        ID : [a-zA-Z]+ ;
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions.len(), 2);
    assert_eq!(ir.productions[0].name, "start");
    assert_eq!(ir.productions[1].name, "ID");
    assert_eq!(ir.start.0, 0);
}

#[test]
fn skip_action() {
    let ir = compile(
        r#"
        grammar Test;
        start : 'hello' ;
        WS : [ \t\r\n]+ -> skip ;
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions.len(), 2);
    assert_eq!(ir.productions[1].name, "WS");
}

#[test]
fn channel_action() {
    let ir = compile(
        r#"
        grammar Test;
        start : 'hello' ;
        WS : [ \t\r\n]+ -> channel(HIDDEN) ;
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions.len(), 2);
}

#[test]
fn fragment_rule() {
    let ir = compile(
        r#"
        grammar Test;
        start : NUMBER ;
        NUMBER : DIGIT+ ;
        fragment DIGIT : [0-9] ;
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions.len(), 3);
    assert_eq!(ir.productions[2].name, "DIGIT");
    for seed in 0..10 {
        let bytes = generate_seeded(&ir, seed);
        assert!(
            bytes.iter().all(|b| b.is_ascii_digit()),
            "expected digits, got {:?}",
            String::from_utf8_lossy(&bytes)
        );
    }
}

#[test]
fn grouped_alternatives() {
    let ir = compile(
        r#"
        grammar Test;
        start : ('a' | 'b') 'c' ;
    "#,
    )
    .unwrap();
    for seed in 0..20 {
        let s = String::from_utf8(generate_seeded(&ir, seed)).unwrap();
        assert!(s == "ac" || s == "bc", "unexpected: {:?}", s);
    }
}

#[test]
fn group_with_quantifier() {
    let ir = compile(
        r#"
        grammar Test;
        start : ('a' | 'b')+ ;
    "#,
    )
    .unwrap();
    for seed in 0..20 {
        let bytes = generate_seeded(&ir, seed);
        assert!(!bytes.is_empty(), "one-or-more should produce output");
        assert!(
            bytes.iter().all(|b| *b == b'a' || *b == b'b'),
            "unexpected: {:?}",
            String::from_utf8_lossy(&bytes)
        );
    }
}

#[test]
fn any_char_dot() {
    let ir = compile(
        r#"
        grammar Test;
        start : ESCAPED ;
        ESCAPED : '\\' . ;
    "#,
    )
    .unwrap();
    let escaped_prod = &ir.productions[1];
    let syms = &escaped_prod.alternatives[0].symbols;
    assert_eq!(syms.len(), 2);
    let any_sym = &ir.symbols[syms[1].symbol.0 as usize];
    assert!(
        matches!(any_sym, Symbol::Terminal(TerminalKind::AnyByte)),
        "expected AnyByte, got {:?}",
        any_sym
    );
}

#[test]
fn multiple_parser_and_lexer_rules() {
    let ir = compile(
        r#"
        grammar Test;
        expr : NUMBER '+' NUMBER ;
        NUMBER : [0-9]+ ;
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions[0].name, "expr");
    assert_eq!(ir.start.0, 0);
}

#[test]
fn line_comments() {
    let ir = compile(
        r#"
        grammar Test;
        // This is a comment
        start : 'hello' ; // inline comment
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions.len(), 1);
}

#[test]
fn block_comments() {
    let ir = compile(
        r#"
        grammar Test;
        /* block comment */
        start : 'a' /* mid-rule */ 'b' ;
    "#,
    )
    .unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"ab");
}

// ── Error cases ─────────────────────────────────────────────────────────────

#[test]
fn error_missing_semicolon() {
    let err = compile("grammar Test;\nstart : 'hello'").unwrap_err();
    assert!(
        err.message.contains("expected") || err.message.contains("end of input"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn error_undefined_rule() {
    let err = compile(
        r#"
        grammar Test;
        start : missing ;
    "#,
    )
    .unwrap_err();
    assert!(err.message.contains("undefined rule"), "got: {}", err);
}

#[test]
fn error_empty_grammar() {
    let err = compile("grammar Test;").unwrap_err();
    assert!(err.message.contains("empty grammar"), "got: {}", err);
}

#[test]
fn error_duplicate_rule() {
    let err = compile(
        r#"
        grammar Test;
        start : 'a' ;
        start : 'b' ;
    "#,
    )
    .unwrap_err();
    assert!(err.message.contains("duplicate"), "got: {}", err);
}

#[test]
fn recursive_grammar() {
    let ir = compile(
        r#"
        grammar Test;
        expr : term '+' expr | term ;
        term : 'x' | '(' expr ')' ;
    "#,
    )
    .unwrap();
    assert!(ir.productions[0].attrs.is_recursive);
    assert!(ir.productions[1].attrs.is_recursive);
    for seed in 0..10 {
        let bytes = generate_seeded(&ir, seed);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains('x'), "output should contain 'x': {:?}", s);
    }
}

// ── Integration: compile the JSON fixture ──────────────────────────────────

#[test]
fn compile_json_fixture() {
    let source = include_str!("../../../fixtures/grammars/json.g4");
    let ir = compile(source).unwrap();
    assert!(
        ir.productions.len() >= 5,
        "JSON grammar should have several productions"
    );
    assert_eq!(ir.productions[ir.start.0 as usize].name, "json");
    for seed in 0..5 {
        let bytes = generate_seeded(&ir, seed);
        assert!(!bytes.is_empty(), "generated output should not be empty");
    }
}
