use barkus_core::ir::{Modifier, Symbol, TerminalKind};
use barkus_parser_common::test_helpers::{generate_one, generate_seeded};
use barkus_peg::compile;

// -- Parse + validate success cases -------------------------------------------

#[test]
fn simple_literal() {
    let ir = compile(r#"start <- "hello""#).unwrap();
    assert_eq!(ir.productions.len(), 1);
    assert_eq!(ir.productions[0].alternatives.len(), 1);
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

#[test]
fn ordered_choice() {
    let ir = compile(r#"start <- "alpha" / "beta" / "gamma""#).unwrap();
    assert_eq!(ir.productions[0].alternatives.len(), 3);
    for seed in 0..20 {
        let s = String::from_utf8(generate_seeded(&ir, seed)).unwrap();
        assert!(
            s == "alpha" || s == "beta" || s == "gamma",
            "unexpected output: {:?}",
            s
        );
    }
}

#[test]
fn character_class() {
    let ir = compile("start <- [a-zA-Z]").unwrap();
    assert_eq!(ir.productions.len(), 1);
    let sym = &ir.symbols[0];
    match sym {
        Symbol::Terminal(TerminalKind::CharClass { ranges, negated }) => {
            assert!(!negated);
            assert_eq!(ranges, &[(b'a', b'z'), (b'A', b'Z')]);
        }
        _ => panic!("expected CharClass, got {:?}", sym),
    }
}

#[test]
fn negated_character_class() {
    let ir = compile("start <- [^0-9]").unwrap();
    let sym = &ir.symbols[0];
    match sym {
        Symbol::Terminal(TerminalKind::CharClass { ranges, negated }) => {
            assert!(negated);
            assert_eq!(ranges, &[(b'0', b'9')]);
        }
        _ => panic!("expected negated CharClass, got {:?}", sym),
    }
}

#[test]
fn single_char_class() {
    let ir = compile("start <- [abc]").unwrap();
    let sym = &ir.symbols[0];
    match sym {
        Symbol::Terminal(TerminalKind::CharClass { ranges, negated }) => {
            assert!(!negated);
            assert_eq!(ranges, &[(b'a', b'a'), (b'b', b'b'), (b'c', b'c')]);
        }
        _ => panic!("expected CharClass, got {:?}", sym),
    }
}

#[test]
fn quantifier_optional() {
    let ir = compile(r#"start <- "a" "b"?"#).unwrap();
    let alt = &ir.productions[0].alternatives[0];
    assert_eq!(alt.symbols.len(), 2);
    assert!(matches!(alt.symbols[1].modifier, Modifier::Optional));
    let mut saw_with = false;
    let mut saw_without = false;
    for seed in 0..50 {
        let s = String::from_utf8(generate_seeded(&ir, seed)).unwrap();
        assert!(s == "a" || s == "ab", "unexpected: {:?}", s);
        if s == "ab" {
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
    let ir = compile(r#"start <- "a" "b"* "c""#).unwrap();
    let alt = &ir.productions[0].alternatives[0];
    assert_eq!(alt.symbols.len(), 3);
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
    let ir = compile(r#"start <- "b"+"#).unwrap();
    let alt = &ir.productions[0].alternatives[0];
    assert_eq!(alt.symbols.len(), 1);
    assert!(matches!(
        alt.symbols[0].modifier,
        Modifier::OneOrMore { .. }
    ));
    for seed in 0..20 {
        let s = String::from_utf8(generate_seeded(&ir, seed)).unwrap();
        assert!(!s.is_empty(), "OneOrMore should produce at least one");
        assert!(
            s.chars().all(|c| c == 'b'),
            "should be all 'b': {:?}",
            s
        );
    }
}

#[test]
fn lookahead_ignored() {
    let ir = compile(r#"start <- &"prefix" "hello" !"bad""#).unwrap();
    assert_eq!(ir.productions.len(), 1);
    let alt = &ir.productions[0].alternatives[0];
    assert_eq!(alt.symbols.len(), 1);
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

#[test]
fn negative_lookahead_ignored() {
    let ir = compile(
        r#"
        keyword <- "if" / "else" / "while"
        ident <- !keyword [a-z]+
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions.len(), 2);
    let alt = &ir.productions[1].alternatives[0];
    assert_eq!(alt.symbols.len(), 1);
}

#[test]
fn both_quote_styles() {
    let ir = compile(r#"start <- 'hello' "world""#).unwrap();
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"helloworld");
}

#[test]
fn any_char_dot() {
    let ir = compile("start <- .").unwrap();
    let sym = &ir.symbols[0];
    assert!(matches!(sym, Symbol::Terminal(TerminalKind::AnyByte)));
}

#[test]
fn arrow_assignment() {
    let ir = compile(r#"start <- "a""#).unwrap();
    assert_eq!(ir.productions.len(), 1);
}

#[test]
fn equals_assignment() {
    let ir = compile(r#"start = "a""#).unwrap();
    assert_eq!(ir.productions.len(), 1);
}

#[test]
fn unicode_arrow_assignment() {
    let ir = compile("start \u{2190} \"a\"").unwrap();
    assert_eq!(ir.productions.len(), 1);
}

#[test]
fn optional_semicolons() {
    let ir = compile(
        r#"
        a <- "x";
        b <- "y"
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions.len(), 2);
}

#[test]
fn comments() {
    let ir = compile(
        r#"
        # This is a PEG grammar
        start <- "hello" # inline comment
        # trailing comment
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions.len(), 1);
    let bytes = generate_one(&ir);
    assert_eq!(bytes, b"hello");
}

#[test]
fn grouping() {
    let ir = compile(r#"start <- ("a" / "b") "c""#).unwrap();
    for seed in 0..20 {
        let s = String::from_utf8(generate_seeded(&ir, seed)).unwrap();
        assert!(s == "ac" || s == "bc", "unexpected: {:?}", s);
    }
}

#[test]
fn grouping_with_quantifier() {
    let ir = compile(r#"start <- ("ab")+"#).unwrap();
    for seed in 0..20 {
        let s = String::from_utf8(generate_seeded(&ir, seed)).unwrap();
        assert!(!s.is_empty());
        assert!(s.len() % 2 == 0, "should be multiples of 'ab': {:?}", s);
    }
}

#[test]
fn multi_rule_grammar() {
    let ir = compile(
        r#"
        Expr   <- Value ('+' Value)*
        Value  <- Number / '(' Expr ')'
        Number <- [0-9]+
    "#,
    )
    .unwrap();
    assert_eq!(ir.productions[0].name, "Expr");
    assert!(ir.productions.len() >= 3);
}

#[test]
fn recursive_grammar() {
    let ir = compile(
        r#"
        expr <- term '+' expr / term
        term <- 'x' / '(' expr ')'
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

#[test]
fn first_rule_is_start() {
    let ir = compile(
        r#"
        program <- statement+
        statement <- "stmt;"
    "#,
    )
    .unwrap();
    assert_eq!(ir.start.0, 0);
    assert_eq!(ir.productions[0].name, "program");
}

// -- Error cases --------------------------------------------------------------

#[test]
fn error_empty_grammar() {
    let err = compile("").unwrap_err();
    assert!(err.message.contains("empty grammar"), "got: {}", err);
}

#[test]
fn error_empty_grammar_comments_only() {
    let err = compile("# just a comment\n# another\n").unwrap_err();
    assert!(err.message.contains("empty grammar"), "got: {}", err);
}

#[test]
fn error_undefined_rule() {
    let err = compile(r#"start <- missing"#).unwrap_err();
    assert!(err.message.contains("undefined rule"), "got: {}", err);
}

#[test]
fn error_duplicate_rule() {
    let err = compile(
        r#"
        start <- "a"
        start <- "b"
    "#,
    )
    .unwrap_err();
    assert!(err.message.contains("duplicate"), "got: {}", err);
}

#[test]
fn error_unterminated_string() {
    let err = compile(r#"start <- "hello"#).unwrap_err();
    assert!(
        err.message.contains("unterminated string"),
        "got: {}",
        err
    );
}

#[test]
fn error_unterminated_char_class() {
    let err = compile("start <- [a-z").unwrap_err();
    assert!(
        err.message.contains("unterminated character class"),
        "got: {}",
        err
    );
}

#[test]
fn error_missing_arrow() {
    let err = compile(r#"start "hello""#).unwrap_err();
    assert!(
        err.message.contains("<-") || err.message.contains("="),
        "got: {}",
        err
    );
}

// -- Integration: compile + generate -----------------------------------------

#[test]
fn generate_sequence() {
    let ir = compile(
        r#"
        start <- greeting ' ' name
        greeting <- "hello" / "hi"
        name <- "world" / "rust"
    "#,
    )
    .unwrap();
    for seed in 0..20 {
        let s = String::from_utf8(generate_seeded(&ir, seed)).unwrap();
        let parts: Vec<&str> = s.splitn(2, ' ').collect();
        assert_eq!(parts.len(), 2, "expected space-separated: {:?}", s);
        assert!(
            parts[0] == "hello" || parts[0] == "hi",
            "unexpected greeting: {:?}",
            parts[0]
        );
        assert!(
            parts[1] == "world" || parts[1] == "rust",
            "unexpected name: {:?}",
            parts[1]
        );
    }
}
