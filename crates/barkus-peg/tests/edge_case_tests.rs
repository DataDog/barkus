use barkus_parser_common::test_helpers::generate_seeded;
use barkus_peg::compile;

#[test]
fn deeply_nested_parens_returns_error() {
    let input = format!("start <- {}\"a\"{}", "(".repeat(300), ")".repeat(300));
    let err = compile(&input).unwrap_err();
    assert!(
        err.message.contains("maximum nesting depth exceeded"),
        "got: {err}"
    );
}

#[test]
fn deeply_nested_positive_lookahead_returns_error() {
    let input = format!("start <- {}\"a\"", "&".repeat(300));
    let err = compile(&input).unwrap_err();
    assert!(
        err.message.contains("maximum nesting depth exceeded"),
        "got: {err}"
    );
}

#[test]
fn deeply_nested_negative_lookahead_returns_error() {
    let input = format!("start <- {}\"a\"", "!".repeat(300));
    let err = compile(&input).unwrap_err();
    assert!(
        err.message.contains("maximum nesting depth exceeded"),
        "got: {err}"
    );
}

#[test]
fn deeply_nested_mixed_parens_and_lookahead_returns_error() {
    let input = format!("start <- {}\"a\"{}", "&(".repeat(150), ")".repeat(150));
    let err = compile(&input).unwrap_err();
    assert!(
        err.message.contains("maximum nesting depth exceeded"),
        "got: {err}"
    );
}

#[test]
fn parenthesized_rule_arrow_returns_error() {
    let err = compile(r#"start <- ( a <- "x" )"#).unwrap_err();
    assert_eq!(err.line, 1);
    assert_eq!(err.column, 14);
    assert!(
        err.message.contains("expected ')'") && err.message.contains("Arrow"),
        "got: {err}"
    );
}

#[test]
fn parenthesized_rule_equals_returns_error() {
    let err = compile(r#"start <- ( a = "x" )"#).unwrap_err();
    assert_eq!(err.line, 1);
    assert_eq!(err.column, 14);
    assert!(
        err.message.contains("expected ')'") && err.message.contains("Equals"),
        "got: {err}"
    );
}

#[test]
fn stacked_quantifier_question_star_returns_error() {
    let err = compile(r#"s <- "a"?*"#).unwrap_err();
    assert_eq!(err.line, 1);
    assert_eq!(err.column, 10);
    assert!(
        err.message.contains("unexpected repeated quantifier"),
        "got: {err}"
    );
    assert!(
        !err.message.contains("expected rule name"),
        "misleading error: {err}"
    );
}

#[test]
fn stacked_quantifier_plus_question_returns_error() {
    let err = compile(r#"s <- "a"+?"#).unwrap_err();
    assert_eq!(err.line, 1);
    assert_eq!(err.column, 10);
    assert!(
        err.message.contains("unexpected repeated quantifier"),
        "got: {err}"
    );
}

#[test]
fn stacked_quantifier_inside_lookahead_returns_error() {
    let err = compile(r#"s <- &"a"?* "b""#).unwrap_err();
    assert_eq!(err.line, 1);
    assert_eq!(err.column, 11);
    assert!(
        err.message.contains("unexpected repeated quantifier"),
        "got: {err}"
    );
}

#[test]
fn parenthesized_nested_quantifier_compiles() {
    let ir = compile(r#"s <- ("a"?)*"#).unwrap();
    for seed in 0..20 {
        let s = String::from_utf8(generate_seeded(&ir, seed)).unwrap();
        assert!(s.chars().all(|c| c == 'a'), "unexpected output: {s:?}");
    }
}

#[test]
fn sequential_groups_and_rules_do_not_exhaust_depth() {
    let groups = "(\"a\") ".repeat(200);
    let input = format!("first <- {groups}\nsecond <- \"b\"");
    let ir = compile(&input).unwrap();
    assert_eq!(ir.productions[0].name, "first");
    assert!(ir.productions.iter().any(|p| p.name == "second"));
}
