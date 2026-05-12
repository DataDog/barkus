//! Shared helpers used by every barkus_* fuzz_target in this SUT.
//!
//! Direct Rust use of barkus-core (NOT the C FFI), so all three
//! ValidityMode variants are first-class — no M3 fallback applies here.

use barkus_core::generate::decode;
use barkus_core::ir::GrammarIr;
use barkus_core::profile::{Profile, ValidityMode};
use std::sync::OnceLock;

// Vendored from antlr/grammars-v4 (css3/css3Lexer.g4, css3/css3Parser.g4).
// Embedded via include_str! so the harness has no runtime dependency on the
// grammar files.
pub const CSS_LEXER: &str = include_str!("../../../../../grammars/css3/css3Lexer.g4");
pub const CSS_PARSER: &str = include_str!("../../../../../grammars/css3/css3Parser.g4");

pub fn grammar() -> &'static GrammarIr {
    static G: OnceLock<GrammarIr> = OnceLock::new();
    G.get_or_init(|| barkus_antlr::compile_split(CSS_LEXER, CSS_PARSER).expect("compile CSS grammar"))
}

pub fn run_with_mode(tape: &[u8], mode: ValidityMode) {
    if tape.is_empty() {
        return;
    }
    let profile = Profile::builder().validity_mode(mode).build();
    let Ok((ast, _)) = decode(grammar(), &profile, tape) else {
        return;
    };
    let css = ast.serialize();
    let Ok(s) = std::str::from_utf8(&css) else {
        return;
    };
    let mut pi = cssparser::ParserInput::new(s);
    let mut p = cssparser::Parser::new(&mut pi);
    while p.next().is_ok() {}
}
