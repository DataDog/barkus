use barkus_core::generate::{decode, decode_with_hooks, generate, generate_with_hooks};
use barkus_core::hooks::SemanticHooks;
use barkus_core::ir::analysis::compute_min_depths;
use barkus_core::ir::grammar::*;
use barkus_core::ir::ids::*;
use barkus_core::profile::Profile;
use rand::rngs::SmallRng;
use rand::SeedableRng;

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

/// Build a simple grammar: S -> "a" | "b"
fn build_simple_grammar() -> GrammarIr {
    let mut symbols = Vec::new();
    let a = lit_sym(&mut symbols, b"a");
    let b = lit_sym(&mut symbols, b"b");

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![simple_alt(a), simple_alt(b)],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);
    ir
}

// ── Test: generate with () hooks is identical to generate without hooks ──

#[test]
fn unit_hooks_backwards_compat() {
    let ir = build_simple_grammar();
    let profile = Profile::default();

    for seed in 0..50 {
        let mut rng1 = SmallRng::seed_from_u64(seed);
        let mut rng2 = SmallRng::seed_from_u64(seed);

        let (ast1, tape1, _) = generate(&ir, &profile, &mut rng1).unwrap();
        let (ast2, tape2, _) =
            generate_with_hooks(&ir, ir.start, &profile, &mut rng2, &mut ()).unwrap();

        assert_eq!(ast1.serialize(), ast2.serialize(), "seed={seed}");
        assert_eq!(tape1.bytes, tape2.bytes, "tape mismatch seed={seed}");
    }
}

// ── Test: custom on_production hook ──

struct OverrideHook;

impl SemanticHooks for OverrideHook {
    fn on_production(
        &mut self,
        hook_name: &str,
        _tape_byte: u8,
        _prod_id: ProductionId,
    ) -> Option<Vec<u8>> {
        if hook_name == "test_override" {
            Some(b"OVERRIDDEN".to_vec())
        } else {
            None
        }
    }
}

#[test]
fn on_production_override() {
    let mut symbols = Vec::new();
    let a = lit_sym(&mut symbols, b"a");

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![simple_alt(a)],
            attrs: ProductionAttrs {
                semantic_hook: Some("test_override".into()),
                ..ProductionAttrs::default()
            },
        }],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();
    let mut rng = SmallRng::seed_from_u64(42);
    let mut hooks = OverrideHook;

    let (ast, _tape, _) =
        generate_with_hooks(&ir, ir.start, &profile, &mut rng, &mut hooks).unwrap();
    assert_eq!(ast.serialize(), b"OVERRIDDEN");
}

#[test]
fn on_production_fallthrough() {
    // Hook returns None for unknown hook names → normal expansion.
    let mut symbols = Vec::new();
    let a = lit_sym(&mut symbols, b"hello");

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![simple_alt(a)],
            attrs: ProductionAttrs {
                semantic_hook: Some("unknown_hook".into()),
                ..ProductionAttrs::default()
            },
        }],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();
    let mut rng = SmallRng::seed_from_u64(42);
    let mut hooks = OverrideHook;

    let (ast, _tape, _) =
        generate_with_hooks(&ir, ir.start, &profile, &mut rng, &mut hooks).unwrap();
    assert_eq!(ast.serialize(), b"hello");
}

// ── Test: on_token_pool hook ──

struct PoolHook;

impl SemanticHooks for PoolHook {
    fn on_token_pool(&mut self, pool_id: PoolId, _tape_byte: u8) -> Option<Vec<u8>> {
        if pool_id == PoolId(0) {
            Some(b"my_table".to_vec())
        } else {
            None
        }
    }
}

#[test]
fn on_token_pool_override() {
    let mut symbols = Vec::new();
    let pool_sym = {
        let id = SymbolId(symbols.len() as u32);
        symbols.push(Symbol::Terminal(TerminalKind::TokenPool(PoolId(0))));
        id
    };

    // Pool entry with mechanical expansion of "x" | "y"
    let x = lit_sym(&mut symbols, b"x");
    let y = lit_sym(&mut symbols, b"y");

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![simple_alt(pool_sym)],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: vec![TokenPoolEntry {
            name: "IDENTIFIER".into(),
            alternatives: vec![simple_alt(x), simple_alt(y)],
        }],
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();
    let mut rng = SmallRng::seed_from_u64(42);
    let mut hooks = PoolHook;

    let (ast, _tape, _) =
        generate_with_hooks(&ir, ir.start, &profile, &mut rng, &mut hooks).unwrap();
    assert_eq!(ast.serialize(), b"my_table");
}

#[test]
fn on_token_pool_mechanical_fallback() {
    // PoolHook only overrides PoolId(0) — PoolId(1) falls through to mechanical expansion.
    let mut symbols = Vec::new();
    let pool_sym = {
        let id = SymbolId(symbols.len() as u32);
        symbols.push(Symbol::Terminal(TerminalKind::TokenPool(PoolId(1))));
        id
    };

    let hello = lit_sym(&mut symbols, b"hello");

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![simple_alt(pool_sym)],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: vec![
            TokenPoolEntry {
                name: "UNUSED".into(),
                alternatives: Vec::new(),
            },
            TokenPoolEntry {
                name: "GREETING".into(),
                alternatives: vec![simple_alt(hello)],
            },
        ],
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();
    let mut rng = SmallRng::seed_from_u64(42);
    let mut hooks = PoolHook;

    let (ast, _tape, _) =
        generate_with_hooks(&ir, ir.start, &profile, &mut rng, &mut hooks).unwrap();
    // Should fall through to mechanical expansion → "hello"
    assert_eq!(ast.serialize(), b"hello");
}

// ── Test: enter/exit production call order ──

struct TrackingHook {
    events: Vec<(bool, ProductionId)>, // (is_enter, prod_id)
}

impl TrackingHook {
    fn new() -> Self {
        Self { events: Vec::new() }
    }
}

impl SemanticHooks for TrackingHook {
    fn enter_production(&mut self, prod_id: ProductionId) {
        self.events.push((true, prod_id));
    }
    fn exit_production(&mut self, prod_id: ProductionId) {
        self.events.push((false, prod_id));
    }
}

#[test]
fn enter_exit_production_order() {
    // Grammar: S -> A "x" ; A -> "y"
    let mut symbols = Vec::new();
    let x = lit_sym(&mut symbols, b"x");
    let y = lit_sym(&mut symbols, b"y");
    let a_nt = nt_sym(&mut symbols, ProductionId(1));

    let mut ir = GrammarIr {
        productions: vec![
            Production {
                id: ProductionId(0),
                name: "S".into(),
                alternatives: vec![multi_alt(vec![
                    (a_nt, Modifier::Once),
                    (x, Modifier::Once),
                ])],
                attrs: ProductionAttrs::default(),
            },
            Production {
                id: ProductionId(1),
                name: "A".into(),
                alternatives: vec![simple_alt(y)],
                attrs: ProductionAttrs::default(),
            },
        ],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();
    let mut rng = SmallRng::seed_from_u64(0);
    let mut hooks = TrackingHook::new();

    let (ast, _tape, _) =
        generate_with_hooks(&ir, ir.start, &profile, &mut rng, &mut hooks).unwrap();
    assert_eq!(ast.serialize(), b"yx");

    // Expected order: enter S, enter A, exit A, exit S
    assert_eq!(hooks.events.len(), 4);
    assert_eq!(hooks.events[0], (true, ProductionId(0))); // enter S
    assert_eq!(hooks.events[1], (true, ProductionId(1))); // enter A
    assert_eq!(hooks.events[2], (false, ProductionId(1))); // exit A
    assert_eq!(hooks.events[3], (false, ProductionId(0))); // exit S
}

// ── Test: tape roundtrip with hooks ──

#[test]
fn tape_roundtrip_with_on_production_hook() {
    let mut symbols = Vec::new();
    let a = lit_sym(&mut symbols, b"a");

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![simple_alt(a)],
            attrs: ProductionAttrs {
                semantic_hook: Some("test_override".into()),
                ..ProductionAttrs::default()
            },
        }],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();
    let mut rng = SmallRng::seed_from_u64(99);
    let mut gen_hooks = OverrideHook;

    let (ast1, tape, _) =
        generate_with_hooks(&ir, ir.start, &profile, &mut rng, &mut gen_hooks).unwrap();

    let mut dec_hooks = OverrideHook;
    let (ast2, _) = decode_with_hooks(&ir, &profile, &tape.bytes, &mut dec_hooks).unwrap();

    assert_eq!(ast1.serialize(), ast2.serialize());
    assert_eq!(ast1.serialize(), b"OVERRIDDEN");
}

#[test]
fn tape_roundtrip_with_token_pool_hook() {
    let mut symbols = Vec::new();
    let pool_sym = {
        let id = SymbolId(symbols.len() as u32);
        symbols.push(Symbol::Terminal(TerminalKind::TokenPool(PoolId(0))));
        id
    };
    let x = lit_sym(&mut symbols, b"x");

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![simple_alt(pool_sym)],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: vec![TokenPoolEntry {
            name: "IDENT".into(),
            alternatives: vec![simple_alt(x)],
        }],
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();
    let mut rng = SmallRng::seed_from_u64(77);
    let mut gen_hooks = PoolHook;

    let (ast1, tape, _) =
        generate_with_hooks(&ir, ir.start, &profile, &mut rng, &mut gen_hooks).unwrap();

    let mut dec_hooks = PoolHook;
    let (ast2, _) = decode_with_hooks(&ir, &profile, &tape.bytes, &mut dec_hooks).unwrap();

    assert_eq!(ast1.serialize(), ast2.serialize());
    assert_eq!(ast1.serialize(), b"my_table");
}

// ── Test: mechanical token pool expansion without hooks ──

#[test]
fn token_pool_mechanical_expansion_no_hooks() {
    let mut symbols = Vec::new();
    let pool_sym = {
        let id = SymbolId(symbols.len() as u32);
        symbols.push(Symbol::Terminal(TerminalKind::TokenPool(PoolId(0))));
        id
    };
    let foo = lit_sym(&mut symbols, b"foo");
    let bar = lit_sym(&mut symbols, b"bar");

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![simple_alt(pool_sym)],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: vec![TokenPoolEntry {
            name: "KEYWORD".into(),
            alternatives: vec![simple_alt(foo), simple_alt(bar)],
        }],
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();

    let mut saw_foo = false;
    let mut saw_bar = false;
    for seed in 0..50 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, _) = generate(&ir, &profile, &mut rng).unwrap();
        let out = ast.serialize();

        if out == b"foo" {
            saw_foo = true;
        } else if out == b"bar" {
            saw_bar = true;
        } else {
            panic!("unexpected output: {:?}", String::from_utf8_lossy(&out));
        }

        // Roundtrip: decode should produce same output.
        let (ast2, _) = decode(&ir, &profile, &tape.bytes).unwrap();
        assert_eq!(out, ast2.serialize(), "roundtrip failed seed={seed}");
    }
    assert!(saw_foo, "never generated 'foo'");
    assert!(saw_bar, "never generated 'bar'");
}
