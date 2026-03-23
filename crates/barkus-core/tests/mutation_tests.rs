use barkus_core::ast::AstNodeKind;
use barkus_core::generate::{decode, generate};
use barkus_core::ir::analysis::compute_min_depths;
use barkus_core::ir::grammar::*;
use barkus_core::ir::ids::*;
use barkus_core::mutation::fragment_db::FragmentDb;
use barkus_core::mutation::meta::MutationMeta;
use barkus_core::mutation::ops::{self, MutationKind};
use barkus_core::profile::Profile;
use rand::rngs::SmallRng;
use rand::SeedableRng;

// ── Helpers ──

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

/// S -> "(" S ")" | "x" | S? S
fn build_recursive_grammar() -> GrammarIr {
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
                // "(" S ")"
                multi_alt(vec![
                    (open, Modifier::Once),
                    (s_nt, Modifier::Once),
                    (close, Modifier::Once),
                ]),
                // "x"
                simple_alt(x),
                // S? S
                multi_alt(vec![(s_nt, Modifier::Optional), (s_nt, Modifier::Once)]),
            ],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);
    ir
}

/// S -> "a" | "b" S   (simple recursive for meta tests)
fn build_simple_recursive_grammar() -> GrammarIr {
    let mut symbols = Vec::new();
    let a = lit_sym(&mut symbols, b"a");
    let b = lit_sym(&mut symbols, b"b");
    let s_nt = nt_sym(&mut symbols, ProductionId(0));

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![
                simple_alt(a),
                multi_alt(vec![(b, Modifier::Once), (s_nt, Modifier::Once)]),
            ],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);
    ir
}

/// S -> A B where A -> "x"* and B -> "a" | "b"
fn build_repetition_grammar() -> GrammarIr {
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
    ir
}

// ── MutationMeta tests ──

#[test]
fn meta_subtree_sizes_and_depths() {
    let ir = build_simple_recursive_grammar();
    let profile = Profile::builder().max_depth(8).build();

    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let (ast, _tape, tape_map) = generate(&ir, &profile, &mut rng).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, &ir);

        // subtree_sizes[root] == total node count
        assert_eq!(
            meta.subtree_sizes[ast.root.0 as usize],
            ast.nodes.len() as u32,
            "seed={seed}: root subtree size != total nodes"
        );

        // depths[root] == 0
        assert_eq!(
            meta.depths[ast.root.0 as usize], 0,
            "seed={seed}: root depth != 0"
        );

        // nodes_by_production[0] should contain all Production(ProductionId(0)) nodes
        let prod0_nodes: Vec<NodeId> = ast
            .nodes
            .iter()
            .filter(|n| matches!(&n.kind, AstNodeKind::Production(pid) if *pid == ProductionId(0)))
            .map(|n| n.id)
            .collect();
        assert_eq!(
            meta.nodes_by_production[0].len(),
            prod0_nodes.len(),
            "seed={seed}: nodes_by_production mismatch"
        );
    }
}

// ── FragmentDb tests ──

#[test]
fn fragment_db_ingest_and_sample() {
    let ir = build_simple_recursive_grammar();
    let profile = Profile::builder().max_depth(8).build();
    let mut rng = SmallRng::seed_from_u64(42);

    let mut db = FragmentDb::new(ir.productions.len(), 100);

    // Ingest several tapes
    for seed in 0..10 {
        let mut gen_rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, tape_map) = generate(&ir, &profile, &mut gen_rng).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, &ir);
        db.ingest(&tape.bytes, &meta, &mut rng);
    }

    // Pool for production 0 should be populated
    assert!(
        db.pool_len(ProductionId(0)) > 0,
        "production 0 pool should be non-empty"
    );

    // Sample returns Some
    assert!(db.sample(ProductionId(0), &mut rng).is_some());
}

#[test]
fn fragment_db_capacity_bound() {
    let ir = build_simple_recursive_grammar();
    let profile = Profile::builder().max_depth(8).build();
    let mut rng = SmallRng::seed_from_u64(42);

    let max_per = 3;
    let mut db = FragmentDb::new(ir.productions.len(), max_per);

    for seed in 0..50 {
        let mut gen_rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, tape_map) = generate(&ir, &profile, &mut gen_rng).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, &ir);
        db.ingest(&tape.bytes, &meta, &mut rng);
    }

    assert!(
        db.pool_len(ProductionId(0)) <= max_per,
        "pool exceeds max_per={max_per}: got {}",
        db.pool_len(ProductionId(0))
    );
}

#[test]
fn fragment_db_empty_pool_returns_none() {
    let db = FragmentDb::new(5, 100);
    let mut rng = SmallRng::seed_from_u64(1);
    assert!(db.sample(ProductionId(0), &mut rng).is_none());
}

// ── TapeMap tests ──

#[test]
fn tape_map_covers_full_body() {
    let ir = build_simple_recursive_grammar();
    let profile = Profile::builder().max_depth(8).build();

    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, tape_map) = generate(&ir, &profile, &mut rng).unwrap();

        // Find the root entry by node_id
        let root_entry = tape_map
            .entries
            .iter()
            .find(|e| e.node_id == ast.root)
            .expect("seed={seed}: root node not found in tape_map");
        assert_eq!(
            root_entry.tape_offset, 2,
            "seed={seed}: root tape_offset should be HEADER_SIZE (2)"
        );
        assert_eq!(
            root_entry.tape_offset + root_entry.tape_len,
            tape.bytes.len(),
            "seed={seed}: root entry should cover full tape body"
        );
    }
}

#[test]
fn tape_map_entries_have_production_ids() {
    let ir = build_simple_recursive_grammar();
    let profile = Profile::builder().max_depth(8).build();
    let mut rng = SmallRng::seed_from_u64(99);

    let (ast, _tape, tape_map) = generate(&ir, &profile, &mut rng).unwrap();

    for entry in &tape_map.entries {
        let node = &ast.nodes[entry.node_id.0 as usize];
        match &node.kind {
            AstNodeKind::Production(pid) => {
                assert_eq!(
                    *pid, entry.production_id,
                    "entry production_id doesn't match AST node"
                );
            }
            _ => panic!("tape_map entry points to non-production node"),
        }
    }
}

// ── Mutation operator tests ──

#[test]
fn point_mutate_changes_one_byte() {
    let ir = build_simple_recursive_grammar();
    let profile = Profile::builder().max_depth(8).build();

    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let (_ast, tape, _map) = generate(&ir, &profile, &mut rng).unwrap();
        if tape.bytes.len() <= 2 {
            continue;
        }

        let mut mutated = tape.bytes.clone();
        let mut mut_rng = SmallRng::seed_from_u64(seed + 100);
        ops::point_mutate(&mut mutated, &mut mut_rng);

        let diff_count = tape
            .bytes
            .iter()
            .zip(mutated.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            diff_count, 1,
            "seed={seed}: expected exactly 1 byte diff, got {diff_count}"
        );
    }
}

#[test]
fn range_rerandomize_bounded() {
    let ir = build_simple_recursive_grammar();
    let profile = Profile::builder().max_depth(8).build();

    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, tape_map) = generate(&ir, &profile, &mut rng).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, &ir);

        let mut mutated = tape.bytes.clone();
        let mut mut_rng = SmallRng::seed_from_u64(seed + 200);
        ops::range_rerandomize(&mut mutated, &meta, &mut mut_rng);

        // Header bytes must be unchanged
        assert_eq!(
            &tape.bytes[..2],
            &mutated[..2],
            "seed={seed}: header changed"
        );
    }
}

#[test]
fn splice_decodes_ok() {
    let ir = build_simple_recursive_grammar();
    let profile = Profile::builder().max_depth(8).build();
    let mut rng = SmallRng::seed_from_u64(42);

    let mut db = FragmentDb::new(ir.productions.len(), 100);

    // Populate the fragment db
    for seed in 0..20 {
        let mut gen_rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, tape_map) = generate(&ir, &profile, &mut gen_rng).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, &ir);
        db.ingest(&tape.bytes, &meta, &mut rng);
    }

    // Now splice and decode
    let mut spliced = 0;
    for seed in 20..40 {
        let mut gen_rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, tape_map) = generate(&ir, &profile, &mut gen_rng).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, &ir);

        let mut mutated = tape.bytes.clone();
        if ops::splice(&mut mutated, &meta, &db, &mut rng) {
            spliced += 1;
            // Should decode without panicking
            let _ = decode(&ir, &profile, &mutated);
        }
    }
    assert!(spliced > 0, "no splices applied");
}

#[test]
fn subtree_regenerate_decodes_ok() {
    let ir = build_simple_recursive_grammar();
    let profile = Profile::builder().max_depth(8).build();

    let mut regenerated = 0;
    for seed in 0..30 {
        let mut gen_rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, tape_map) = generate(&ir, &profile, &mut gen_rng).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, &ir);

        let mut mutated = tape.bytes.clone();
        let mut mut_rng = SmallRng::seed_from_u64(seed + 300);
        if ops::subtree_regenerate(&mut mutated, &meta, &ir, &profile, &mut mut_rng) {
            regenerated += 1;
            let _ = decode(&ir, &profile, &mutated);
        }
    }
    assert!(regenerated > 0, "no subtree regenerations applied");
}

#[test]
fn toggle_optional_decodable() {
    let ir = build_recursive_grammar();
    let profile = Profile::builder().max_depth(6).build();

    let mut toggled = 0;
    for seed in 0..50 {
        let mut gen_rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, tape_map) = generate(&ir, &profile, &mut gen_rng).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, &ir);

        let mut mutated = tape.bytes.clone();
        let mut mut_rng = SmallRng::seed_from_u64(seed + 400);
        if ops::toggle_optional(&mut mutated, &meta, &mut mut_rng) {
            toggled += 1;
            // Should not panic on decode (may error due to budget, that's ok)
            let _ = decode(&ir, &profile, &mutated);
        }
    }
    assert!(toggled > 0, "no optional toggles applied");
}

#[test]
fn perturb_repetition_changes_count() {
    let ir = build_repetition_grammar();
    let profile = Profile::default();

    let mut perturbed = 0;
    for seed in 0..30 {
        let mut gen_rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, tape_map) = generate(&ir, &profile, &mut gen_rng).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, &ir);

        let mut mutated = tape.bytes.clone();
        let mut mut_rng = SmallRng::seed_from_u64(seed + 500);
        if ops::perturb_repetition(&mut mutated, &meta, &mut mut_rng) {
            perturbed += 1;
            // Decode should succeed
            let result = decode(&ir, &profile, &mutated);
            assert!(
                result.is_ok(),
                "seed={seed}: decode failed after perturb_repetition"
            );
        }
    }
    assert!(perturbed > 0, "no repetition perturbations applied");
}

#[test]
fn roundtrip_all_operators() {
    let ir = build_recursive_grammar();
    let profile = Profile::builder().max_depth(6).build();
    let mut rng = SmallRng::seed_from_u64(42);

    let mut db = FragmentDb::new(ir.productions.len(), 100);

    // Populate fragment db
    for seed in 0..20 {
        let mut gen_rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, tape_map) = generate(&ir, &profile, &mut gen_rng).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, &ir);
        db.ingest(&tape.bytes, &meta, &mut rng);
    }

    // For each seed, apply mutate() and verify decode doesn't panic
    for seed in 20..50 {
        let mut gen_rng = SmallRng::seed_from_u64(seed);
        let (ast, tape, tape_map) = generate(&ir, &profile, &mut gen_rng).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, &ir);

        let mut mutated = tape.bytes.clone();
        let kind = ops::mutate(&mut mutated, &meta, &ir, &profile, &db, &mut rng);

        // Decode should not panic (may return error for budget, that's fine)
        let _ = decode(&ir, &profile, &mutated);

        // Verify kind is valid
        assert!(matches!(
            kind,
            MutationKind::PointMutate
                | MutationKind::RangeRerandomize
                | MutationKind::Splice
                | MutationKind::SubtreeRegenerate
                | MutationKind::ToggleOptional
                | MutationKind::PerturbRepetition
        ));
    }
}
