#![no_main]
use libfuzzer_sys::fuzz_target;

use std::sync::LazyLock;

use barkus_core::ir::analysis::compute_min_depths;
use barkus_core::ir::{
    Alternative, GrammarIr, Modifier, Production, ProductionAttrs, ProductionId, Symbol,
    SymbolId, SymbolRef, TerminalKind,
};
use barkus_core::profile::{Profile, ValidityMode};

/// Simple recursive grammar built programmatically:
///   S → "(" S ")" | "x" | S? "y"
static SIMPLE_GRAMMAR: LazyLock<GrammarIr> = LazyLock::new(|| {
    // Symbols:
    //   0 = Terminal "("
    //   1 = NonTerminal S (production 0)
    //   2 = Terminal ")"
    //   3 = Terminal "x"
    //   4 = Terminal "y"
    let symbols = vec![
        Symbol::Terminal(TerminalKind::Literal(b"(".to_vec())),
        Symbol::NonTerminal(ProductionId(0)),
        Symbol::Terminal(TerminalKind::Literal(b")".to_vec())),
        Symbol::Terminal(TerminalKind::Literal(b"x".to_vec())),
        Symbol::Terminal(TerminalKind::Literal(b"y".to_vec())),
    ];

    let productions = vec![Production {
        id: ProductionId(0),
        name: "S".into(),
        alternatives: vec![
            // "(" S ")"
            Alternative {
                symbols: vec![
                    SymbolRef { symbol: SymbolId(0), modifier: Modifier::Once },
                    SymbolRef { symbol: SymbolId(1), modifier: Modifier::Once },
                    SymbolRef { symbol: SymbolId(2), modifier: Modifier::Once },
                ],
                weight: 1.0,
                semantic_tag: None,
            },
            // "x"
            Alternative {
                symbols: vec![
                    SymbolRef { symbol: SymbolId(3), modifier: Modifier::Once },
                ],
                weight: 1.0,
                semantic_tag: None,
            },
            // S? "y"
            Alternative {
                symbols: vec![
                    SymbolRef { symbol: SymbolId(1), modifier: Modifier::Optional },
                    SymbolRef { symbol: SymbolId(4), modifier: Modifier::Once },
                ],
                weight: 1.0,
                semantic_tag: None,
            },
        ],
        attrs: ProductionAttrs {
            min_depth: 0,
            is_recursive: true,
            token_kind: None,
            semantic_hook: None,
        },
    }];

    let mut grammar = GrammarIr {
        productions,
        symbols,
        start: ProductionId(0),
        token_pools: vec![],
    };
    compute_min_depths(&mut grammar);
    grammar
});

/// Arithmetic expression grammar compiled from EBNF.
static EXPR_GRAMMAR: LazyLock<GrammarIr> = LazyLock::new(|| {
    barkus_ebnf::compile(
        r#"
        expr   = term { ("+" | "-") term } ;
        term   = factor { ("*" | "/") factor } ;
        factor = "(" expr ")" | number ;
        number = digit { digit } ;
        digit  = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;
        "#,
    )
    .expect("built-in EBNF grammar must compile")
});

static PROFILE: LazyLock<Profile> = LazyLock::new(|| Profile {
    validity_mode: ValidityMode::Strict,
    max_depth: 15,
    max_total_nodes: 500,
    repetition_bounds: (0, 4),
    dictionary: vec![],
    havoc_intensity: 0.0,
    rule_overrides: Default::default(),
});

fuzz_target!(|data: &[u8]| {
    let _ = barkus_core::generate::decode(&*SIMPLE_GRAMMAR, &*PROFILE, data);
    let _ = barkus_core::generate::decode(&*EXPR_GRAMMAR, &*PROFILE, data);
});
