use barkus_ebnf::compile;
use barkus_parser_common::test_helpers::{generate_one, generate_seeded};

// ── Parse + validate success cases ──────────────────────────────────────────

#[test]
fn simple_literal() {
    let ir = compile(r#"start = "hello" ;"#).unwrap();
    assert_eq!(ir.productions.len(), 1);
    assert_eq!(ir.productions[0].alternatives.len(), 1);
}

#[test]
fn multiple_rules() {
    let ir = compile(
        r#"
        start = greeting " " name ;
        greeting = "hello" | "hi" ;
        name = "world" | "rust" ;
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions.len(), 3);
    assert_eq!(ir.productions[0].name, "start");
}

#[test]
fn choices() {
    let ir = compile(r#"start = "a" | "b" | "c" | "d" ;"#).unwrap();
    assert_eq!(ir.productions[0].alternatives.len(), 4);
}

#[test]
fn recursive_grammar() {
    let ir = compile(
        r#"
        expr = term "+" expr | term ;
        term = "x" | "(" expr ")" ;
    "#,
    )
    .unwrap();
    assert!(ir.productions[0].attrs.is_recursive);
    assert!(ir.productions[1].attrs.is_recursive);
}

#[test]
fn optional_modifier() {
    let ir = compile(r#"start = "a" ["b"] ;"#).unwrap();
    let alt = &ir.productions[0].alternatives[0];
    assert_eq!(alt.symbols.len(), 2);
    assert!(matches!(
        alt.symbols[1].modifier,
        barkus_core::ir::Modifier::Optional
    ));
}

#[test]
fn repetition_modifier() {
    let ir = compile(r#"start = "a" {"b"} ;"#).unwrap();
    let alt = &ir.productions[0].alternatives[0];
    assert_eq!(alt.symbols.len(), 2);
    assert!(matches!(
        alt.symbols[1].modifier,
        barkus_core::ir::Modifier::ZeroOrMore { .. }
    ));
}

#[test]
fn concatenation() {
    let ir = compile(r#"start = "a" "b" "c" ;"#).unwrap();
    let alt = &ir.productions[0].alternatives[0];
    assert_eq!(alt.symbols.len(), 3);
}

#[test]
fn escape_sequences() {
    let ir = compile(r#"start = "hello\nworld" ;"#).unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello\nworld");
}

// ── Standard EBNF features ──────────────────────────────────────────────────

#[test]
fn comma_concatenation() {
    let ir = compile(r#"start = "a", "b", "c" ;"#).unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"abc");
}

#[test]
fn single_quoted_strings() {
    let ir = compile(r#"start = 'hello' ;"#).unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

#[test]
fn mixed_quotes() {
    let ir = compile(r#"start = "a", 'b', "c" ;"#).unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"abc");
}

#[test]
fn single_quote_containing_double() {
    let ir = compile(r#"start = '"' ;"#).unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"\"");
}

#[test]
fn dot_terminator() {
    let ir = compile(r#"start = "hello" ."#).unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

#[test]
fn slash_alternation() {
    let ir = compile(r#"start = "a" / "b" / "c" ;"#).unwrap();
    assert_eq!(ir.productions[0].alternatives.len(), 3);
}

#[test]
fn repetition_factor() {
    let ir = compile(r#"start = 3 * "ab" ;"#).unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"ababab");
}

#[test]
fn repetition_factor_with_nonterminal() {
    let ir = compile(
        r#"
        start = 2 * x ;
        x = "hi" ;
    "#,
    )
    .unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hihi");
}

#[test]
fn exception_ignored_for_generation() {
    let ir = compile(
        r#"
        start = letter - vowel ;
        letter = "a" | "b" | "c" ;
        vowel = "a" ;
    "#,
    )
    .unwrap();
    let bytes = generate_one(&ir);
    let s = String::from_utf8(bytes).unwrap();
    assert!(s == "a" || s == "b" || s == "c");
}

// ── Comment handling ────────────────────────────────────────────────────────

#[test]
fn line_comments() {
    let ir = compile(
        r#"
        // This is a grammar
        start = "a" | "b" ; // inline comment
        // trailing comment
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions.len(), 1);
    assert_eq!(ir.productions[0].alternatives.len(), 2);
}

#[test]
fn block_comments() {
    let ir = compile(
        r#"
        /* Header comment */
        start = "a" /* mid-rule */ "b" ;
    "#,
    )
    .unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"ab");
}

#[test]
fn multiline_block_comment() {
    let ir = compile(
        r#"
        /*
         * Multi-line
         * block comment
         */
        start = "hello" ;
    "#,
    )
    .unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

#[test]
fn ebnf_block_comments() {
    let ir = compile(
        r#"
        (* This is a standard EBNF comment *)
        start = "a" (* mid-rule *) "b" ;
    "#,
    )
    .unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"ab");
}

#[test]
fn multiline_ebnf_block_comment() {
    let ir = compile(
        r#"
        (*
         * Multi-line
         * EBNF comment
         *)
        start = "hello" ;
    "#,
    )
    .unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

#[test]
fn unterminated_block_comment() {
    let err = compile("/* never closed\nstart = \"a\" ;").unwrap_err();
    assert!(
        err.message.contains("unterminated block comment"),
        "got: {}",
        err
    );
}

#[test]
fn unterminated_ebnf_block_comment() {
    let err = compile("(* never closed\nstart = \"a\" ;").unwrap_err();
    assert!(
        err.message.contains("unterminated block comment"),
        "got: {}",
        err
    );
}

// ── Error cases ─────────────────────────────────────────────────────────────

#[test]
fn error_missing_semicolon() {
    let err = compile(r#"start = "hello""#).unwrap_err();
    assert!(
        err.message.contains("expected")
            || err.message.contains("Semicolon")
            || err.message.contains("end of input"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn error_undefined_rule() {
    let err = compile(r#"start = missing ;"#).unwrap_err();
    assert!(err.message.contains("undefined rule"), "got: {}", err);
}

#[test]
fn error_empty_body() {
    let err = compile(r#"start = ;"#).unwrap_err();
    assert!(
        err.message.contains("empty") || err.message.contains("unexpected"),
        "got: {}",
        err
    );
}

#[test]
fn error_duplicate_rule() {
    let err = compile(
        r#"
        start = "a" ;
        start = "b" ;
    "#,
    )
    .unwrap_err();
    assert!(err.message.contains("duplicate"), "got: {}", err);
}

#[test]
fn error_empty_grammar() {
    let err = compile("").unwrap_err();
    assert!(err.message.contains("empty grammar"), "got: {}", err);
}

// ── Integration: compile + generate ─────────────────────────────────────────

#[test]
fn generate_simple_literal() {
    let ir = compile(r#"start = "hello" ;"#).unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

#[test]
fn generate_choices_are_valid() {
    let ir = compile(r#"start = "alpha" | "beta" | "gamma" ;"#).unwrap();
    for seed in 0..20 {
        let bytes = generate_seeded(&ir, seed);
        let s = String::from_utf8(bytes).unwrap();
        assert!(
            s == "alpha" || s == "beta" || s == "gamma",
            "unexpected output: {:?}",
            s
        );
    }
}

#[test]
fn generate_concatenation() {
    let ir = compile(
        r#"
        start = greeting name ;
        greeting = "hi" ;
        name = "world" ;
    "#,
    )
    .unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hiworld");
}

#[test]
fn generate_recursive_terminates() {
    let ir = compile(
        r#"
        expr = term "+" expr | term ;
        term = "x" | "(" expr ")" ;
    "#,
    )
    .unwrap();
    for seed in 0..10 {
        let bytes = generate_seeded(&ir, seed);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains('x'), "output should contain 'x': {:?}", s);
    }
}

#[test]
fn generate_optional() {
    let ir = compile(r#"start = "a" ["b"] "c" ;"#).unwrap();
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
fn generate_repetition() {
    let ir = compile(r#"start = "a" {"b"} "c" ;"#).unwrap();
    for seed in 0..20 {
        let s = String::from_utf8(generate_seeded(&ir, seed)).unwrap();
        assert!(s.starts_with('a'), "should start with 'a': {:?}", s);
        assert!(s.ends_with('c'), "should end with 'c': {:?}", s);
        let middle = &s[1..s.len() - 1];
        assert!(
            middle.chars().all(|c| c == 'b'),
            "middle should be all 'b': {:?}",
            middle
        );
    }
}
