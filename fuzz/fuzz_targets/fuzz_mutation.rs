#![no_main]
use libfuzzer_sys::fuzz_target;

use std::sync::LazyLock;

use barkus_core::generate::decode;
use barkus_core::ir::analysis::compute_min_depths;
use barkus_core::ir::{
    Alternative, GrammarIr, Modifier, Production, ProductionAttrs, ProductionId, Symbol,
    SymbolId, SymbolRef, TerminalKind,
};
use barkus_core::mutation::{FragmentDb, MutationMeta};
use barkus_core::mutation::ops;
use barkus_core::profile::{Profile, ValidityMode};
use rand::rngs::SmallRng;
use rand::SeedableRng;

/// Simple recursive grammar: S → "(" S ")" | "x" | S? "y"
static GRAMMAR: LazyLock<GrammarIr> = LazyLock::new(|| {
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
            Alternative {
                symbols: vec![
                    SymbolRef { symbol: SymbolId(0), modifier: Modifier::Once },
                    SymbolRef { symbol: SymbolId(1), modifier: Modifier::Once },
                    SymbolRef { symbol: SymbolId(2), modifier: Modifier::Once },
                ],
                weight: 1.0,
                semantic_tag: None,
            },
            Alternative {
                symbols: vec![
                    SymbolRef { symbol: SymbolId(3), modifier: Modifier::Once },
                ],
                weight: 1.0,
                semantic_tag: None,
            },
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
    // Need at least 2 bytes for the RNG seed plus some tape bytes.
    if data.len() < 3 {
        return;
    }

    let seed = u64::from(data[0]) | (u64::from(data[1]) << 8);
    let tape_bytes = &data[2..];
    let mut rng = SmallRng::seed_from_u64(seed);

    // Decode the tape into an AST; skip if the tape doesn't decode.
    let (ast, tape_map) = match decode(&*GRAMMAR, &*PROFILE, tape_bytes) {
        Ok(pair) => pair,
        Err(_) => return,
    };

    let meta = MutationMeta::compute(&ast, tape_map, &*GRAMMAR);

    let mut tape = tape_bytes.to_vec();
    let db = FragmentDb::new(GRAMMAR.productions.len(), 64);

    // Run the top-level mutate dispatcher.
    let _ = ops::mutate(&mut tape, &meta, &*GRAMMAR, &*PROFILE, &db, &mut rng);

    // Re-decode the mutated tape to check for panics.
    let _ = decode(&*GRAMMAR, &*PROFILE, &tape);
});
