use barkus_core::error::IrError;
use barkus_core::ir::analysis::{compute_min_depths, mark_recursive};
use barkus_core::ir::grammar::*;
use barkus_core::ir::ids::*;

fn lit_sym(symbols: &mut Vec<Symbol>, bytes: &[u8]) -> SymbolId {
    let id = SymbolId(symbols.len() as u32);
    symbols.push(Symbol::Terminal(TerminalKind::Literal(bytes.to_vec())));
    id
}

fn nt_sym(symbols: &mut Vec<Symbol>, prod: ProductionId) -> SymbolId {
    let id = SymbolId(symbols.len() as u32);
    symbols.push(Symbol::NonTerminal(prod));
    id
}

fn simple_alt(sym: SymbolId) -> Alternative {
    Alternative {
        symbols: vec![SymbolRef {
            symbol: sym,
            modifier: Modifier::Once,
        }],
        weight: 1.0,
        semantic_tag: None,
    }
}

fn multi_alt(syms: Vec<(SymbolId, Modifier)>) -> Alternative {
    Alternative {
        symbols: syms
            .into_iter()
            .map(|(s, m)| SymbolRef {
                symbol: s,
                modifier: m,
            })
            .collect(),
        weight: 1.0,
        semantic_tag: None,
    }
}

// ── Validation tests ──

#[test]
fn valid_ir_passes_validation() {
    let mut symbols = Vec::new();
    let hello = lit_sym(&mut symbols, b"hello");

    let ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![simple_alt(hello)],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };

    assert!(ir.validate().is_ok());
}

#[test]
fn invalid_symbol_ref_caught() {
    let ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![simple_alt(SymbolId(999))],
            attrs: ProductionAttrs::default(),
        }],
        symbols: vec![],
        start: ProductionId(0),
        token_pools: Vec::new(),
    };

    assert!(matches!(ir.validate(), Err(IrError::InvalidSymbolRef(_))));
}

#[test]
fn invalid_production_ref_caught() {
    let mut symbols = Vec::new();
    let bad_nt = nt_sym(&mut symbols, ProductionId(999));

    let ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![simple_alt(bad_nt)],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };

    assert!(matches!(
        ir.validate(),
        Err(IrError::InvalidProductionRef(_))
    ));
}

#[test]
fn empty_alternative_caught() {
    let ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![Alternative {
                symbols: vec![],
                weight: 1.0,
                semantic_tag: None,
            }],
            attrs: ProductionAttrs::default(),
        }],
        symbols: vec![],
        start: ProductionId(0),
        token_pools: Vec::new(),
    };

    assert!(matches!(
        ir.validate(),
        Err(IrError::EmptyAlternative { .. })
    ));
}

#[test]
fn missing_start_production() {
    let ir = GrammarIr {
        productions: vec![],
        symbols: vec![],
        start: ProductionId(0),
        token_pools: Vec::new(),
    };

    assert!(matches!(
        ir.validate(),
        Err(IrError::MissingStartProduction)
    ));
}

// ── Analysis tests ──

/// S -> "a" | S "a"  →  min_depth(S) = 1
#[test]
fn compute_min_depths_simple_recursive() {
    let mut symbols = Vec::new();
    let a = lit_sym(&mut symbols, b"a");
    let s_nt = nt_sym(&mut symbols, ProductionId(0));

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![
                simple_alt(a),
                multi_alt(vec![(s_nt, Modifier::Once), (a, Modifier::Once)]),
            ],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };

    compute_min_depths(&mut ir);
    assert_eq!(ir.productions[0].attrs.min_depth, 1);
}

/// A -> B, B -> "x"  →  min_depth(A) = 2, min_depth(B) = 1
#[test]
fn compute_min_depths_chain() {
    let mut symbols = Vec::new();
    let x = lit_sym(&mut symbols, b"x");
    let b_nt = nt_sym(&mut symbols, ProductionId(1));

    let mut ir = GrammarIr {
        productions: vec![
            Production {
                id: ProductionId(0),
                name: "A".into(),
                alternatives: vec![simple_alt(b_nt)],
                attrs: ProductionAttrs::default(),
            },
            Production {
                id: ProductionId(1),
                name: "B".into(),
                alternatives: vec![simple_alt(x)],
                attrs: ProductionAttrs::default(),
            },
        ],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };

    compute_min_depths(&mut ir);
    assert_eq!(ir.productions[0].attrs.min_depth, 2);
    assert_eq!(ir.productions[1].attrs.min_depth, 1);
}

#[test]
fn mark_recursive_detects_self_recursion() {
    let mut symbols = Vec::new();
    let a = lit_sym(&mut symbols, b"a");
    let s_nt = nt_sym(&mut symbols, ProductionId(0));

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![
                simple_alt(a),
                multi_alt(vec![(s_nt, Modifier::Once), (a, Modifier::Once)]),
            ],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };

    mark_recursive(&mut ir);
    assert!(ir.productions[0].attrs.is_recursive);
}

#[test]
fn mark_recursive_non_recursive_is_false() {
    let mut symbols = Vec::new();
    let x = lit_sym(&mut symbols, b"x");
    let b_nt = nt_sym(&mut symbols, ProductionId(1));

    let mut ir = GrammarIr {
        productions: vec![
            Production {
                id: ProductionId(0),
                name: "A".into(),
                alternatives: vec![simple_alt(b_nt)],
                attrs: ProductionAttrs::default(),
            },
            Production {
                id: ProductionId(1),
                name: "B".into(),
                alternatives: vec![simple_alt(x)],
                attrs: ProductionAttrs::default(),
            },
        ],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };

    mark_recursive(&mut ir);
    assert!(!ir.productions[0].attrs.is_recursive);
    assert!(!ir.productions[1].attrs.is_recursive);
}

#[test]
fn mark_recursive_mutual_recursion() {
    // A -> B, B -> A | "x"
    let mut symbols = Vec::new();
    let x = lit_sym(&mut symbols, b"x");
    let b_nt = nt_sym(&mut symbols, ProductionId(1));
    let a_nt = nt_sym(&mut symbols, ProductionId(0));

    let mut ir = GrammarIr {
        productions: vec![
            Production {
                id: ProductionId(0),
                name: "A".into(),
                alternatives: vec![simple_alt(b_nt)],
                attrs: ProductionAttrs::default(),
            },
            Production {
                id: ProductionId(1),
                name: "B".into(),
                alternatives: vec![simple_alt(a_nt), simple_alt(x)],
                attrs: ProductionAttrs::default(),
            },
        ],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };

    mark_recursive(&mut ir);
    assert!(ir.productions[0].attrs.is_recursive);
    assert!(ir.productions[1].attrs.is_recursive);
}
