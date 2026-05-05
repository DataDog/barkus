//! Shared helpers used by every barkus_* fuzz_target in this SUT.
//!
//! Direct Rust use of barkus-core (NOT the C FFI), so all three
//! ValidityMode variants are first-class — no M3 fallback applies here.

use barkus_core::generate::decode;
use barkus_core::ir::GrammarIr;
use barkus_core::profile::{Profile, ValidityMode};
use std::sync::OnceLock;

/// Minimal CSS-shaped EBNF used as a placeholder until benchmarks/fixtures/
/// grammars-v4 (the antlr/grammars-v4 submodule) lands. The point of M5 is
/// to validate end-to-end Rust+libFuzzer plumbing; the real CSS3 grammar
/// from antlr/grammars-v4 swaps in later without harness changes.
// `r##"..."##` so the literal `"#"` inside the grammar (id selector) does
// not prematurely close the raw string.
pub const CSS_GRAMMAR: &str = r##"
start = stylesheet ;
stylesheet = rule | rule stylesheet ;
rule = selector " " "{" " " decls "}" " " ;
decls = decl | decl decls ;
decl = ident ":" " " value ";" " " ;
selector = ident | ident "." ident | "#" ident ;
value = ident | ident " " value | num ;
ident = "div" | "p" | "a" | "span" | "body" | "html"
      | "color" | "background" | "margin" | "padding"
      | "blue" | "red" | "green" | "auto" | "inherit"
      | "x" | "y" | "z" ;
num = "0" | "1" | "10" | "100" ;
"##;

pub fn grammar() -> &'static GrammarIr {
    static G: OnceLock<GrammarIr> = OnceLock::new();
    G.get_or_init(|| barkus_ebnf::compile(CSS_GRAMMAR).expect("compile CSS grammar"))
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
