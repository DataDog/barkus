use barkus_core::generate::{decode, generate};
use barkus_core::ir::analysis::compute_min_depths;
use barkus_core::ir::grammar::*;
use barkus_core::ir::ids::*;
use barkus_core::profile::Profile;
use rand::rngs::SmallRng;
use rand::SeedableRng;

// ── Helpers (duplicated — Rust integration tests are separate compilation units) ──

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

/// S → A B C, each of A/B/C → "x" | "y".
/// Three independent choices — flipping one decision byte only changes the
/// corresponding output byte; the other two are untouched.
#[test]
fn locality_sequence_of_choices() {
    let mut symbols = Vec::new();
    let x = lit_sym(&mut symbols, b"x");
    let y = lit_sym(&mut symbols, b"y");
    let a_nt = nt_sym(&mut symbols, ProductionId(1));
    let b_nt = nt_sym(&mut symbols, ProductionId(2));
    let c_nt = nt_sym(&mut symbols, ProductionId(3));

    let binary_choice = |id: u32| Production {
        id: ProductionId(id),
        name: format!("P{id}"),
        alternatives: vec![simple_alt(x), simple_alt(y)],
        attrs: ProductionAttrs::default(),
    };

    let mut ir = GrammarIr {
        productions: vec![
            Production {
                id: ProductionId(0),
                name: "S".into(),
                alternatives: vec![multi_alt(vec![
                    (a_nt, Modifier::Once),
                    (b_nt, Modifier::Once),
                    (c_nt, Modifier::Once),
                ])],
                attrs: ProductionAttrs::default(),
            },
            binary_choice(1),
            binary_choice(2),
            binary_choice(3),
        ],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();

    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, _map) = generate(&ir, &profile, &mut rng).unwrap();
        let original = ast.serialize();
        assert_eq!(original.len(), 3, "seed={seed}: output should be 3 bytes");
        // Tape: [mode, reserved, A_choice, B_choice, C_choice]
        assert_eq!(tape.bytes.len(), 5, "seed={seed}: tape should be 5 bytes");

        for flip_idx in 2..5 {
            let mut mutated = tape.bytes.clone();
            mutated[flip_idx] ^= 0xFF;
            let (decoded, _) = decode(&ir, &profile, &mutated).unwrap();
            let out = decoded.serialize();
            assert_eq!(out.len(), 3);

            // Only the byte whose decision was flipped may differ.
            for (out_byte, tape_byte) in [(0usize, 2usize), (1, 3), (2, 4)] {
                if tape_byte != flip_idx {
                    assert_eq!(
                        original[out_byte], out[out_byte],
                        "seed={seed} flip={flip_idx}: output byte {out_byte} changed unexpectedly"
                    );
                }
            }
        }
    }
}

/// S → A B where A → "hello" | "world" and B → "foo" | "bar".
/// Flipping B's tape byte leaves A's 5-byte prefix unchanged.
#[test]
fn locality_prefix_stable_under_suffix_mutation() {
    let mut symbols = Vec::new();
    let hello = lit_sym(&mut symbols, b"hello");
    let world = lit_sym(&mut symbols, b"world");
    let foo = lit_sym(&mut symbols, b"foo");
    let bar = lit_sym(&mut symbols, b"bar");
    let a_nt = nt_sym(&mut symbols, ProductionId(1));
    let b_nt = nt_sym(&mut symbols, ProductionId(2));

    let mut ir = GrammarIr {
        productions: vec![
            Production {
                id: ProductionId(0),
                name: "S".into(),
                alternatives: vec![multi_alt(vec![
                    (a_nt, Modifier::Once),
                    (b_nt, Modifier::Once),
                ])],
                attrs: ProductionAttrs::default(),
            },
            Production {
                id: ProductionId(1),
                name: "A".into(),
                alternatives: vec![simple_alt(hello), simple_alt(world)],
                attrs: ProductionAttrs::default(),
            },
            Production {
                id: ProductionId(2),
                name: "B".into(),
                alternatives: vec![simple_alt(foo), simple_alt(bar)],
                attrs: ProductionAttrs::default(),
            },
        ],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();

    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, _map) = generate(&ir, &profile, &mut rng).unwrap();
        let original = ast.serialize();
        assert_eq!(tape.bytes.len(), 4, "seed={seed}: tape should be 4 bytes");
        assert_eq!(original.len(), 8, "seed={seed}: output should be 8 bytes");

        // Flip B's byte (index 3) — A's 5-byte prefix must be identical.
        let mut mutated = tape.bytes.clone();
        mutated[3] ^= 0xFF;
        let (decoded, _) = decode(&ir, &profile, &mutated).unwrap();
        let out = decoded.serialize();
        assert_eq!(
            &original[..5],
            &out[..5],
            "seed={seed}: A prefix changed when B was flipped"
        );
    }
}

/// Same grammar as above, flipping A's byte leaves B's 3-byte suffix unchanged.
#[test]
fn locality_suffix_stable_under_prefix_mutation() {
    let mut symbols = Vec::new();
    let hello = lit_sym(&mut symbols, b"hello");
    let world = lit_sym(&mut symbols, b"world");
    let foo = lit_sym(&mut symbols, b"foo");
    let bar = lit_sym(&mut symbols, b"bar");
    let a_nt = nt_sym(&mut symbols, ProductionId(1));
    let b_nt = nt_sym(&mut symbols, ProductionId(2));

    let mut ir = GrammarIr {
        productions: vec![
            Production {
                id: ProductionId(0),
                name: "S".into(),
                alternatives: vec![multi_alt(vec![
                    (a_nt, Modifier::Once),
                    (b_nt, Modifier::Once),
                ])],
                attrs: ProductionAttrs::default(),
            },
            Production {
                id: ProductionId(1),
                name: "A".into(),
                alternatives: vec![simple_alt(hello), simple_alt(world)],
                attrs: ProductionAttrs::default(),
            },
            Production {
                id: ProductionId(2),
                name: "B".into(),
                alternatives: vec![simple_alt(foo), simple_alt(bar)],
                attrs: ProductionAttrs::default(),
            },
        ],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();

    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, _map) = generate(&ir, &profile, &mut rng).unwrap();
        let original = ast.serialize();
        assert_eq!(tape.bytes.len(), 4);
        assert_eq!(original.len(), 8);

        // Flip A's byte (index 2) — B's 3-byte suffix must be identical.
        let mut mutated = tape.bytes.clone();
        mutated[2] ^= 0xFF;
        let (decoded, _) = decode(&ir, &profile, &mutated).unwrap();
        let out = decoded.serialize();
        assert_eq!(
            &original[5..],
            &out[5..],
            "seed={seed}: B suffix changed when A was flipped"
        );
    }
}

/// S → "(" S ")" | "x". Flipping an inner decision preserves the prefix
/// of opening parens produced by earlier (outer) decisions.
#[test]
fn locality_recursive_grammar() {
    let mut symbols = Vec::new();
    let open = lit_sym(&mut symbols, b"(");
    let close = lit_sym(&mut symbols, b")");
    let x = lit_sym(&mut symbols, b"x");
    let s_nt = nt_sym(&mut symbols, ProductionId(0));

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![
                multi_alt(vec![
                    (open, Modifier::Once),
                    (s_nt, Modifier::Once),
                    (close, Modifier::Once),
                ]),
                simple_alt(x),
            ],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);

    let profile = Profile::builder().max_depth(10).build();

    let mut tested = 0;
    for seed in 0..100 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, _map) = generate(&ir, &profile, &mut rng).unwrap();
        let original = ast.serialize();
        let n_decisions = tape.bytes.len() - 2;

        // Need at least 2 decisions (1+ nesting level) for a meaningful test.
        if n_decisions < 2 {
            continue;
        }
        tested += 1;

        let depth = original.iter().take_while(|&&b| b == b'(').count();
        assert_eq!(depth, n_decisions - 1, "seed={seed}: depth mismatch");

        // Flip the last decision (innermost): all leading '(' are from earlier
        // decisions and must be preserved.
        {
            let mut mutated = tape.bytes.clone();
            mutated[tape.bytes.len() - 1] ^= 0xFF;
            if let Ok((decoded, _)) = decode(&ir, &profile, &mutated) {
                let out = decoded.serialize();
                assert!(
                    out.len() >= depth,
                    "seed={seed}: mutated output shorter than nesting depth"
                );
                assert_eq!(
                    &original[..depth],
                    &out[..depth],
                    "seed={seed}: outer parens not preserved after innermost flip"
                );
            }
            // decode failure (depth budget) is acceptable — mutation deepened the tree
        }

        // Flip a middle decision: the prefix from decisions before it is preserved.
        if n_decisions >= 3 {
            let mid_decision = n_decisions / 2; // 0-indexed decision position
            let mid_tape_idx = 2 + mid_decision;
            let mut mutated = tape.bytes.clone();
            mutated[mid_tape_idx] ^= 0xFF;

            if let Ok((decoded, _)) = decode(&ir, &profile, &mutated) {
                let out = decoded.serialize();
                // Decisions 0..mid each contributed one '(' to the prefix.
                assert!(
                    out.len() >= mid_decision,
                    "seed={seed}: mutated output shorter than preserved prefix"
                );
                assert_eq!(
                    &original[..mid_decision],
                    &out[..mid_decision],
                    "seed={seed}: prefix not preserved after mid-decision flip"
                );
            }
        }
    }
    assert!(tested >= 10, "not enough multi-level seeds found");
}

/// S → A B where A → "x"* and B → "a" | "b".
/// Flipping A's repetition byte leaves B's choice unchanged (and vice versa).
#[test]
fn locality_repetition_count_isolated() {
    let mut symbols = Vec::new();
    let x = lit_sym(&mut symbols, b"x");
    let a_sym = lit_sym(&mut symbols, b"a");
    let b_sym = lit_sym(&mut symbols, b"b");
    let a_nt = nt_sym(&mut symbols, ProductionId(1));
    let b_nt = nt_sym(&mut symbols, ProductionId(2));

    let mut ir = GrammarIr {
        productions: vec![
            Production {
                id: ProductionId(0),
                name: "S".into(),
                alternatives: vec![multi_alt(vec![
                    (a_nt, Modifier::Once),
                    (b_nt, Modifier::Once),
                ])],
                attrs: ProductionAttrs::default(),
            },
            Production {
                id: ProductionId(1),
                name: "A".into(),
                alternatives: vec![Alternative {
                    symbols: vec![SymbolRef {
                        symbol: x,
                        modifier: Modifier::ZeroOrMore { min: 0, max: 5 },
                    }],
                    weight: 1.0,
                    semantic_tag: None,
                }],
                attrs: ProductionAttrs::default(),
            },
            Production {
                id: ProductionId(2),
                name: "B".into(),
                alternatives: vec![simple_alt(a_sym), simple_alt(b_sym)],
                attrs: ProductionAttrs::default(),
            },
        ],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();

    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let (_, tape, _map) = generate(&ir, &profile, &mut rng).unwrap();
        // Tape: [mode, reserved, A_rep, B_choice]
        assert_eq!(tape.bytes.len(), 4, "seed={seed}: unexpected tape length");

        // Use decode for baseline (not generate's AST) — the tape writer has a
        // known wrapping-overflow issue for range > 2 that can desync generate
        // vs decode. Locality is a tape-level property, so decode-vs-decode is
        // the correct comparison.
        let original = decode(&ir, &profile, &tape.bytes).unwrap().0.serialize();
        assert!(!original.is_empty(), "seed={seed}: output too short");

        let a_len = original.len() - 1;
        let b_byte = *original.last().unwrap();

        // Flip A's repetition byte (index 2) → B's last byte unchanged.
        {
            let mut mutated = tape.bytes.clone();
            mutated[2] ^= 0xFF;
            let (decoded, _) = decode(&ir, &profile, &mutated).unwrap();
            let out = decoded.serialize();
            assert_eq!(
                *out.last().unwrap(),
                b_byte,
                "seed={seed}: B changed when A's rep was flipped"
            );
            // A's portion should be all 'x's (possibly different count).
            let new_a_len = out.len() - 1;
            assert!(
                out[..new_a_len].iter().all(|&b| b == b'x'),
                "seed={seed}: A portion contains non-x bytes after rep flip"
            );
        }

        // Flip B's choice byte (index 3) → A's prefix unchanged.
        {
            let mut mutated = tape.bytes.clone();
            mutated[3] ^= 0xFF;
            let (decoded, _) = decode(&ir, &profile, &mutated).unwrap();
            let out = decoded.serialize();
            assert_eq!(
                out.len(),
                original.len(),
                "seed={seed}: length changed on B flip"
            );
            assert_eq!(
                &out[..a_len],
                &original[..a_len],
                "seed={seed}: A changed when B's choice was flipped"
            );
        }
    }
}

/// 8 sequential binary choices (each → "x" | "y"). For every single-byte flip,
/// only the corresponding output byte changes — all others are identical.
#[test]
fn locality_many_decisions_single_flip() {
    const N: usize = 8;

    let mut symbols = Vec::new();
    let x = lit_sym(&mut symbols, b"x");
    let y = lit_sym(&mut symbols, b"y");

    let nt_ids: Vec<SymbolId> = (0..N)
        .map(|i| nt_sym(&mut symbols, ProductionId(i as u32 + 1)))
        .collect();

    let binary_choice = |id: u32| Production {
        id: ProductionId(id),
        name: format!("P{id}"),
        alternatives: vec![simple_alt(x), simple_alt(y)],
        attrs: ProductionAttrs::default(),
    };

    let mut productions = vec![Production {
        id: ProductionId(0),
        name: "S".into(),
        alternatives: vec![multi_alt(
            nt_ids.iter().map(|&s| (s, Modifier::Once)).collect(),
        )],
        attrs: ProductionAttrs::default(),
    }];
    for i in 0..N {
        productions.push(binary_choice(i as u32 + 1));
    }

    let mut ir = GrammarIr {
        productions,
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);

    let profile = Profile::default();

    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, _map) = generate(&ir, &profile, &mut rng).unwrap();
        let original = ast.serialize();
        assert_eq!(original.len(), N, "seed={seed}");
        assert_eq!(tape.bytes.len(), 2 + N, "seed={seed}");

        for flip_pos in 0..N {
            let tape_idx = 2 + flip_pos;
            let mut mutated = tape.bytes.clone();
            mutated[tape_idx] ^= 0xFF;
            let (decoded, _) = decode(&ir, &profile, &mutated).unwrap();
            let out = decoded.serialize();
            assert_eq!(out.len(), N);

            // Every output byte except flip_pos must be unchanged.
            for i in 0..N {
                if i != flip_pos {
                    assert_eq!(
                        original[i], out[i],
                        "seed={seed} flip={flip_pos}: byte {i} changed"
                    );
                }
            }
            // The flipped byte should have changed (XOR 0xFF always flips a binary choice).
            assert_ne!(
                original[flip_pos], out[flip_pos],
                "seed={seed} flip={flip_pos}: flipped byte should differ"
            );
        }
    }
}
