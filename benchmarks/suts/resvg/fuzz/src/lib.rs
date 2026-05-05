//! Shared helpers for resvg/usvg fuzz targets.

use barkus_core::generate::decode;
use barkus_core::ir::GrammarIr;
use barkus_core::profile::{Profile, ValidityMode};
use std::sync::OnceLock;

/// Minimal XML/SVG-shaped EBNF placeholder. Real svg.g4 / xml.g4 from
/// antlr/grammars-v4 land in a future iteration; M5's job is just to
/// validate the libFuzzer-on-Rust-SUT plumbing for both cssparser AND
/// resvg, not to ship paper-quality grammars.
pub const SVG_GRAMMAR: &str = r##"
start = svg ;
svg = "<svg>" content "</svg>" ;
content = element | element content | "" ;
element = "<rect/>" | "<circle/>" | "<line/>" | "<path/>"
        | "<g>" content "</g>" ;
"##;

pub fn grammar() -> &'static GrammarIr {
    static G: OnceLock<GrammarIr> = OnceLock::new();
    G.get_or_init(|| barkus_ebnf::compile(SVG_GRAMMAR).expect("compile SVG grammar"))
}

pub fn run_with_mode(tape: &[u8], mode: ValidityMode) {
    if tape.is_empty() {
        return;
    }
    let profile = Profile::builder().validity_mode(mode).build();
    let Ok((ast, _)) = decode(grammar(), &profile, tape) else {
        return;
    };
    let svg = ast.serialize();
    let Ok(s) = std::str::from_utf8(&svg) else {
        return;
    };
    let _ = usvg::Tree::from_str(s, &usvg::Options::default());
}

pub fn run_raw(data: &[u8]) {
    let Ok(s) = std::str::from_utf8(data) else { return; };
    let _ = usvg::Tree::from_str(s, &usvg::Options::default());
}
