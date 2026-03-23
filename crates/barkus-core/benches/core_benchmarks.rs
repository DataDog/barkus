use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::rngs::SmallRng;
use rand::SeedableRng;

use barkus_core::generate::{decode, generate};
use barkus_core::ir::analysis::compute_min_depths;
use barkus_core::ir::grammar::*;
use barkus_core::ir::ids::*;
use barkus_core::mutation::fragment_db::FragmentDb;
use barkus_core::mutation::ops::mutate;
use barkus_core::mutation::MutationMeta;
use barkus_core::profile::Profile;

// ── Grammar builders ──

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

fn char_class_sym(symbols: &mut Vec<Symbol>, ranges: Vec<(u8, u8)>) -> SymbolId {
    let id = SymbolId(symbols.len() as u32);
    symbols.push(Symbol::Terminal(TerminalKind::CharClass {
        ranges,
        negated: false,
    }));
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

/// Small grammar: S -> "a" | "b" | "c"
fn build_small_grammar() -> GrammarIr {
    let mut symbols = Vec::new();
    let a = lit_sym(&mut symbols, b"a");
    let b = lit_sym(&mut symbols, b"b");
    let c = lit_sym(&mut symbols, b"c");

    let mut ir = GrammarIr {
        productions: vec![Production {
            id: ProductionId(0),
            name: "S".into(),
            alternatives: vec![simple_alt(a), simple_alt(b), simple_alt(c)],
            attrs: ProductionAttrs::default(),
        }],
        symbols,
        start: ProductionId(0),
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);
    ir
}

/// Medium grammar: JSON-like structure (~10 productions).
///
/// value   -> object | array | string | number | "true" | "false" | "null"
/// object  -> "{" members? "}"
/// members -> pair ("," pair)*  (approximated as pair+)
/// pair    -> string ":" value
/// array   -> "[" elements? "]"
/// elements-> value ("," value)*  (approximated as value+)
/// string  -> '"' char* '"'
/// char    -> [a-z]
/// number  -> digit+
/// digit   -> [0-9]
fn build_medium_grammar() -> GrammarIr {
    let mut symbols = Vec::new();
    let mut productions = Vec::new();

    // Reserve production IDs.
    let p_value = ProductionId(0);
    let p_object = ProductionId(1);
    let p_members = ProductionId(2);
    let p_pair = ProductionId(3);
    let p_array = ProductionId(4);
    let p_elements = ProductionId(5);
    let p_string = ProductionId(6);
    let p_char = ProductionId(7);
    let p_number = ProductionId(8);
    let p_digit = ProductionId(9);

    // Terminals.
    let lbrace = lit_sym(&mut symbols, b"{");
    let rbrace = lit_sym(&mut symbols, b"}");
    let lbrack = lit_sym(&mut symbols, b"[");
    let rbrack = lit_sym(&mut symbols, b"]");
    let colon = lit_sym(&mut symbols, b":");
    let comma = lit_sym(&mut symbols, b",");
    let quote = lit_sym(&mut symbols, b"\"");
    let t_true = lit_sym(&mut symbols, b"true");
    let t_false = lit_sym(&mut symbols, b"false");
    let t_null = lit_sym(&mut symbols, b"null");
    let az = char_class_sym(&mut symbols, vec![(b'a', b'z')]);
    let digit_cc = char_class_sym(&mut symbols, vec![(b'0', b'9')]);

    // Non-terminal symbols.
    let nt_value = nt_sym(&mut symbols, p_value);
    let nt_object = nt_sym(&mut symbols, p_object);
    let nt_members = nt_sym(&mut symbols, p_members);
    let nt_pair = nt_sym(&mut symbols, p_pair);
    let nt_array = nt_sym(&mut symbols, p_array);
    let nt_elements = nt_sym(&mut symbols, p_elements);
    let nt_string = nt_sym(&mut symbols, p_string);
    let nt_char = nt_sym(&mut symbols, p_char);
    let nt_number = nt_sym(&mut symbols, p_number);
    let nt_digit = nt_sym(&mut symbols, p_digit);

    // value -> object | array | string | number | "true" | "false" | "null"
    productions.push(Production {
        id: p_value,
        name: "value".into(),
        alternatives: vec![
            simple_alt(nt_object),
            simple_alt(nt_array),
            simple_alt(nt_string),
            simple_alt(nt_number),
            simple_alt(t_true),
            simple_alt(t_false),
            simple_alt(t_null),
        ],
        attrs: ProductionAttrs::default(),
    });

    // object -> "{" members? "}"
    productions.push(Production {
        id: p_object,
        name: "object".into(),
        alternatives: vec![multi_alt(vec![
            (lbrace, Modifier::Once),
            (nt_members, Modifier::Optional),
            (rbrace, Modifier::Once),
        ])],
        attrs: ProductionAttrs::default(),
    });

    // members -> pair ("," pair)*
    productions.push(Production {
        id: p_members,
        name: "members".into(),
        alternatives: vec![multi_alt(vec![
            (nt_pair, Modifier::Once),
            (comma, Modifier::ZeroOrMore { min: 0, max: 3 }),
            (nt_pair, Modifier::ZeroOrMore { min: 0, max: 3 }),
        ])],
        attrs: ProductionAttrs::default(),
    });

    // pair -> string ":" value
    productions.push(Production {
        id: p_pair,
        name: "pair".into(),
        alternatives: vec![multi_alt(vec![
            (nt_string, Modifier::Once),
            (colon, Modifier::Once),
            (nt_value, Modifier::Once),
        ])],
        attrs: ProductionAttrs::default(),
    });

    // array -> "[" elements? "]"
    productions.push(Production {
        id: p_array,
        name: "array".into(),
        alternatives: vec![multi_alt(vec![
            (lbrack, Modifier::Once),
            (nt_elements, Modifier::Optional),
            (rbrack, Modifier::Once),
        ])],
        attrs: ProductionAttrs::default(),
    });

    // elements -> value ("," value)*
    productions.push(Production {
        id: p_elements,
        name: "elements".into(),
        alternatives: vec![multi_alt(vec![
            (nt_value, Modifier::Once),
            (comma, Modifier::ZeroOrMore { min: 0, max: 3 }),
            (nt_value, Modifier::ZeroOrMore { min: 0, max: 3 }),
        ])],
        attrs: ProductionAttrs::default(),
    });

    // string -> '"' char* '"'
    productions.push(Production {
        id: p_string,
        name: "string".into(),
        alternatives: vec![multi_alt(vec![
            (quote, Modifier::Once),
            (nt_char, Modifier::ZeroOrMore { min: 0, max: 8 }),
            (quote, Modifier::Once),
        ])],
        attrs: ProductionAttrs::default(),
    });

    // char -> [a-z]
    productions.push(Production {
        id: p_char,
        name: "char".into(),
        alternatives: vec![simple_alt(az)],
        attrs: ProductionAttrs::default(),
    });

    // number -> digit+
    productions.push(Production {
        id: p_number,
        name: "number".into(),
        alternatives: vec![Alternative {
            symbols: vec![SymbolRef {
                symbol: nt_digit,
                modifier: Modifier::OneOrMore { min: 1, max: 5 },
            }],
            weight: 1.0,
            semantic_tag: None,
        }],
        attrs: ProductionAttrs::default(),
    });

    // digit -> [0-9]
    productions.push(Production {
        id: p_digit,
        name: "digit".into(),
        alternatives: vec![simple_alt(digit_cc)],
        attrs: ProductionAttrs::default(),
    });

    let mut ir = GrammarIr {
        productions,
        symbols,
        start: p_value,
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);
    ir
}

/// Large grammar: chain of ~50 productions with branching and repetition.
///
/// P0 -> P1 P2 | "leaf0"
/// P1 -> P3 P4 | "leaf1"
/// ...
/// P48 -> "leaf48a" | "leaf48b"
/// P49 -> [a-z]+
fn build_large_grammar() -> GrammarIr {
    let n = 50usize;
    let mut symbols = Vec::new();
    let mut productions = Vec::with_capacity(n);

    // Pre-create all production IDs so we can reference them.
    let prod_ids: Vec<ProductionId> = (0..n).map(|i| ProductionId(i as u32)).collect();

    // Create terminal leaves for each production.
    let leaf_syms: Vec<SymbolId> = (0..n)
        .map(|i| lit_sym(&mut symbols, format!("L{i}").as_bytes()))
        .collect();

    // Create non-terminal symbols for each production.
    let nt_syms: Vec<SymbolId> = (0..n).map(|i| nt_sym(&mut symbols, prod_ids[i])).collect();

    // A shared char-class terminal for the leaf production.
    let az = char_class_sym(&mut symbols, vec![(b'a', b'z')]);

    // Build productions: each non-leaf production references two children or falls back to a leaf.
    for i in 0..n - 1 {
        let left_child = (2 * i + 1).min(n - 1);
        let right_child = (2 * i + 2).min(n - 1);

        productions.push(Production {
            id: prod_ids[i],
            name: format!("P{i}"),
            alternatives: vec![
                multi_alt(vec![
                    (nt_syms[left_child], Modifier::Once),
                    (nt_syms[right_child], Modifier::Once),
                ]),
                simple_alt(leaf_syms[i]),
            ],
            attrs: ProductionAttrs::default(),
        });
    }

    // Last production: terminal-only with char-class repetition.
    productions.push(Production {
        id: prod_ids[n - 1],
        name: format!("P{}", n - 1),
        alternatives: vec![
            simple_alt(leaf_syms[n - 1]),
            Alternative {
                symbols: vec![SymbolRef {
                    symbol: az,
                    modifier: Modifier::OneOrMore { min: 1, max: 4 },
                }],
                weight: 1.0,
                semantic_tag: None,
            },
        ],
        attrs: ProductionAttrs::default(),
    });

    let mut ir = GrammarIr {
        productions,
        symbols,
        start: prod_ids[0],
        token_pools: Vec::new(),
    };
    compute_min_depths(&mut ir);
    ir
}

// ── Pre-generate a tape for decode/mutation benchmarks ──

struct PreparedInput {
    grammar: GrammarIr,
    profile: Profile,
    tape_bytes: Vec<u8>,
    meta: MutationMeta,
    fragment_db: FragmentDb,
}

fn prepare_input(grammar: GrammarIr, profile: Profile, seed: u64) -> PreparedInput {
    let mut rng = SmallRng::seed_from_u64(seed);

    // Generate a few times to populate the fragment DB.
    let n_prods = grammar.productions.len();
    let mut fragment_db = FragmentDb::new(n_prods, 64);

    let mut last_tape_bytes = Vec::new();
    let mut last_meta = None;

    for s in 0..10 {
        let mut rng_inner = SmallRng::seed_from_u64(seed + s);
        if let Ok((ast, tape, tape_map)) = generate(&grammar, &profile, &mut rng_inner) {
            let meta = MutationMeta::compute(&ast, tape_map, &grammar);
            fragment_db.ingest(&tape.bytes, &meta, &mut rng);
            last_tape_bytes = tape.bytes;
            last_meta = Some(meta);
        }
    }

    // If we never succeeded, generate once more with a generous budget.
    if last_meta.is_none() {
        let generous = Profile::builder()
            .max_depth(20)
            .max_total_nodes(50_000)
            .build();
        let mut rng2 = SmallRng::seed_from_u64(seed + 100);
        let (ast, tape, tape_map) = generate(&grammar, &generous, &mut rng2).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, &grammar);
        fragment_db.ingest(&tape.bytes, &meta, &mut rng);
        last_tape_bytes = tape.bytes;
        last_meta = Some(meta);
    }

    PreparedInput {
        grammar,
        profile,
        tape_bytes: last_tape_bytes,
        meta: last_meta.unwrap(),
        fragment_db,
    }
}

// ── Benchmarks ──

fn bench_generate(c: &mut Criterion) {
    let mut group = c.benchmark_group("generate");
    group.throughput(criterion::Throughput::Elements(1));

    let grammars: Vec<(&str, GrammarIr)> = vec![
        ("small", build_small_grammar()),
        ("medium", build_medium_grammar()),
        ("large", build_large_grammar()),
    ];

    for (name, grammar) in &grammars {
        let profile = Profile::default();
        group.bench_with_input(BenchmarkId::new("throughput", name), name, |b, _| {
            let mut rng = SmallRng::seed_from_u64(42);
            b.iter(|| {
                let _ = generate(black_box(grammar), black_box(&profile), &mut rng);
            });
        });
    }

    group.finish();
}

fn bench_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode");
    group.throughput(criterion::Throughput::Elements(1));

    let cases: Vec<(&str, PreparedInput)> = vec![
        (
            "small",
            prepare_input(build_small_grammar(), Profile::default(), 1),
        ),
        (
            "medium",
            prepare_input(build_medium_grammar(), Profile::default(), 2),
        ),
        (
            "large",
            prepare_input(build_large_grammar(), Profile::default(), 3),
        ),
    ];

    for (name, input) in &cases {
        group.bench_with_input(BenchmarkId::new("throughput", name), name, |b, _| {
            b.iter(|| {
                let _ = decode(
                    black_box(&input.grammar),
                    black_box(&input.profile),
                    black_box(&input.tape_bytes),
                );
            });
        });
    }

    group.finish();
}

fn bench_mutation(c: &mut Criterion) {
    let mut group = c.benchmark_group("mutation");
    group.throughput(criterion::Throughput::Elements(1));

    let cases: Vec<(&str, PreparedInput)> = vec![
        (
            "small",
            prepare_input(build_small_grammar(), Profile::default(), 10),
        ),
        (
            "medium",
            prepare_input(build_medium_grammar(), Profile::default(), 20),
        ),
        (
            "large",
            prepare_input(build_large_grammar(), Profile::default(), 30),
        ),
    ];

    for (name, input) in &cases {
        group.bench_with_input(BenchmarkId::new("throughput", name), name, |b, _| {
            let mut rng = SmallRng::seed_from_u64(77);
            let mut tape = input.tape_bytes.clone();
            b.iter(|| {
                // Reset tape each iteration to avoid drift.
                tape.clear();
                tape.extend_from_slice(&input.tape_bytes);
                let _ = mutate(
                    black_box(&mut tape),
                    black_box(&input.meta),
                    black_box(&input.grammar),
                    black_box(&input.profile),
                    black_box(&input.fragment_db),
                    &mut rng,
                );
            });
        });
    }

    group.finish();
}

fn bench_mutation_meta_compute(c: &mut Criterion) {
    let mut group = c.benchmark_group("mutation_meta");
    group.throughput(criterion::Throughput::Elements(1));

    let cases: Vec<(&str, GrammarIr, Profile, u64)> = vec![
        ("small", build_small_grammar(), Profile::default(), 100),
        ("medium", build_medium_grammar(), Profile::default(), 200),
        ("large", build_large_grammar(), Profile::default(), 300),
    ];

    for (name, grammar, profile, seed) in &cases {
        // Pre-generate AST + tape for the compute benchmark.
        let mut rng = SmallRng::seed_from_u64(*seed);
        let (ast, _tape, tape_map) = generate(grammar, profile, &mut rng).unwrap();

        group.bench_with_input(BenchmarkId::new("compute", name), name, |b, _| {
            b.iter(|| {
                let _ = MutationMeta::compute(
                    black_box(&ast),
                    black_box(tape_map.clone()),
                    black_box(grammar),
                );
            });
        });
    }

    group.finish();
}

fn bench_fragment_db_ingest(c: &mut Criterion) {
    let mut group = c.benchmark_group("fragment_db");
    group.throughput(criterion::Throughput::Elements(1));

    let cases: Vec<(&str, GrammarIr, Profile, u64)> = vec![
        ("small", build_small_grammar(), Profile::default(), 400),
        ("medium", build_medium_grammar(), Profile::default(), 500),
        ("large", build_large_grammar(), Profile::default(), 600),
    ];

    for (name, grammar, profile, seed) in &cases {
        let mut rng = SmallRng::seed_from_u64(*seed);
        let (ast, tape, tape_map) = generate(grammar, profile, &mut rng).unwrap();
        let meta = MutationMeta::compute(&ast, tape_map, grammar);
        let n_prods = grammar.productions.len();

        group.bench_with_input(BenchmarkId::new("ingest", name), name, |b, _| {
            let mut rng = SmallRng::seed_from_u64(42);
            let mut db = FragmentDb::new(n_prods, 64);
            b.iter(|| {
                db = FragmentDb::new(n_prods, 64);
                db.ingest(black_box(&tape.bytes), black_box(&meta), &mut rng);
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_generate,
    bench_decode,
    bench_mutation,
    bench_mutation_meta_compute,
    bench_fragment_db_ingest,
);
criterion_main!(benches);
