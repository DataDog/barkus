//! Demonstrates direct tape mutation with barkus-core's mutation operators.

use barkus_core::generate::{decode, generate};
use barkus_core::mutation::fragment_db::FragmentDb;
use barkus_core::mutation::meta::MutationMeta;
use barkus_core::mutation::ops;
use barkus_core::profile::Profile;
use rand::rngs::SmallRng;
use rand::SeedableRng;

const JSON_EBNF: &str = include_str!("../../../fixtures/grammars/json.ebnf");

fn main() {
    let grammar = barkus_ebnf::compile(JSON_EBNF).expect("grammar should compile");
    let profile = Profile::builder().max_depth(10).build();

    let mut rng = SmallRng::seed_from_u64(42);
    let (ast, tape, tape_map) = generate(&grammar, &profile, &mut rng).expect("generation failed");
    let original = String::from_utf8_lossy(&ast.serialize()).to_string();

    println!("=== Original ===");
    println!("output: {original}");
    println!("tape:   {} bytes (2 header + {} body)\n", tape.bytes.len(), tape.bytes.len() - 2);

    // MutationMeta maps tape bytes to AST structure.
    let meta = MutationMeta::compute(&ast, tape_map, &grammar);

    // FragmentDb is populated once and reused by both the splice op and the dispatcher
    // below — without seeding it, splice has no fragments to draw from.
    let mut db = FragmentDb::new(grammar.productions.len(), 64);
    for seed in 0..20 {
        let mut r = SmallRng::seed_from_u64(seed);
        if let Ok((a, tp, tm)) = generate(&grammar, &profile, &mut r) {
            let m = MutationMeta::compute(&a, tm, &grammar);
            db.ingest(&tp.bytes, &m, &mut rng);
        }
    }

    // --- Level 1: tape-level mutations ---

    // 1. Point mutate: flip one bit or ±1 on a single byte.
    {
        let mut t = tape.bytes.clone();
        let mut r = SmallRng::seed_from_u64(100);
        ops::point_mutate(&mut t, &mut r);
        print_decode("point_mutate", &grammar, &profile, &t, &original);
    }

    // 2. Range re-randomize: fill a production's tape region with random bytes.
    {
        let mut t = tape.bytes.clone();
        let mut r = SmallRng::seed_from_u64(101);
        ops::range_rerandomize(&mut t, &meta, &mut r);
        print_decode("range_rerandomize", &grammar, &profile, &t, &original);
    }

    // 3. Splice: replace a production's tape region with a fragment from the corpus.
    {
        let mut t = tape.bytes.clone();
        let mut r = SmallRng::seed_from_u64(102);
        let ok = ops::splice(&mut t, &meta, &db, &mut r);
        if ok {
            print_decode("splice", &grammar, &profile, &t, &original);
        } else {
            println!("splice: no compatible fragment found (try a larger corpus)");
        }
    }

    // --- Level 2: structure-aware mutations ---

    // 4. Subtree regenerate: re-derive a random subtree from scratch.
    {
        let mut t = tape.bytes.clone();
        let mut r = SmallRng::seed_from_u64(103);
        let ok = ops::subtree_regenerate(&mut t, &meta, &grammar, &profile, &mut r);
        if ok {
            print_decode("subtree_regenerate", &grammar, &profile, &t, &original);
        } else {
            println!("subtree_regenerate: no eligible subtree found");
        }
    }

    // 5. Toggle optional: flip an optional element present ↔ absent.
    {
        let mut t = tape.bytes.clone();
        let mut r = SmallRng::seed_from_u64(104);
        let ok = ops::toggle_optional(&mut t, &meta, &mut r);
        if ok {
            print_decode("toggle_optional", &grammar, &profile, &t, &original);
        } else {
            println!("toggle_optional: no optional modifiers in this tape");
        }
    }

    // 6. Perturb repetition: adjust a repetition count by ±1.
    {
        let mut t = tape.bytes.clone();
        let mut r = SmallRng::seed_from_u64(105);
        let ok = ops::perturb_repetition(&mut t, &meta, &mut r);
        if ok {
            print_decode("perturb_repetition", &grammar, &profile, &t, &original);
        } else {
            println!("perturb_repetition: no repetition modifiers in this tape");
        }
    }

    // --- Top-level dispatcher (picks a random operator) ---
    println!();
    println!("=== Weighted-random dispatch (10 rounds) ===");
    for i in 0..10 {
        let mut t = tape.bytes.clone();
        let mut r = SmallRng::seed_from_u64(200 + i);
        let kind = ops::mutate(&mut t, &meta, &grammar, &profile, &db, &mut r);
        match decode(&grammar, &profile, &t) {
            Ok((a, _)) => {
                let bytes = a.serialize();
                let out = String::from_utf8_lossy(&bytes);
                println!("  {kind:?}: {out}");
            }
            Err(e) => println!("  {kind:?}: decode error: {e}"),
        }
    }
}

fn print_decode(name: &str, grammar: &barkus_core::ir::grammar::GrammarIr, profile: &Profile, tape: &[u8], original: &str) {
    match decode(grammar, profile, tape) {
        Ok((ast, _)) => {
            let out = String::from_utf8_lossy(&ast.serialize()).to_string();
            let marker = if out != original { " ← CHANGED" } else { "" };
            println!("{name}: {out}{marker}");
        }
        Err(e) => println!("{name}: decode error: {e}"),
    }
}
