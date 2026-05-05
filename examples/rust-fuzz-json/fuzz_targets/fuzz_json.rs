//! cargo-fuzz target: tape-driven JSON fuzzing.
//!
//! Mirrors the Go fuzz example. Each input is treated as a barkus decision
//! tape: we decode it into a JSON-shaped string and feed that to a JSON
//! parser. libFuzzer mutates tape bytes between runs, so the parser sees
//! structured-but-edge-case JSON rather than random bytes.
//!
//! Run with:
//!
//!   cargo +nightly fuzz run fuzz_json --fuzz-dir examples/rust-fuzz-json
//!
//! See examples/README.md for a recipe to seed the corpus with valid tapes.
//!
//! Outside of fuzz tests, the same `decode` API works for one-shot payload
//! generation — see `barkus-core` docs.
#![no_main]

use barkus_core::generate::decode;
use barkus_core::ir::grammar::GrammarIr;
use barkus_core::profile::Profile;
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

const JSON_EBNF: &str = include_str!("../../../fixtures/grammars/json.ebnf");

// Compile the grammar once and reuse it across fuzz iterations. libFuzzer
// runs the target in a tight loop, so any per-iteration setup is hot-path.
fn grammar() -> &'static (GrammarIr, Profile) {
    static G: OnceLock<(GrammarIr, Profile)> = OnceLock::new();
    G.get_or_init(|| {
        // Profile options: max_depth caps recursion (default 20), and the
        // builder also exposes max_total_nodes / validity_mode if you need
        // tighter bounds or near-valid output.
        let profile = Profile::builder().max_depth(20).build();
        let grammar = barkus_ebnf::compile(JSON_EBNF).expect("grammar should compile");
        (grammar, profile)
    })
}

fuzz_target!(|tape: &[u8]| {
    if tape.len() < 3 {
        return;
    }
    let (grammar, profile) = grammar();
    let Ok((ast, _)) = decode(grammar, profile, tape) else {
        return; // invalid tape — fuzzer will learn to avoid these
    };
    let bytes = ast.serialize();
    // Feed the JSON-shaped output to your parser or system under test.
    let _: Result<serde_json::Value, _> = serde_json::from_slice(&bytes);
});
