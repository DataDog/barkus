//! Shared helpers for resvg/usvg fuzz targets.

use barkus_core::generate::decode;
use barkus_core::ir::GrammarIr;
use barkus_core::profile::{Profile, ValidityMode};
use std::sync::OnceLock;

// SVG is XML-shaped. There is no `svg.g4` in antlr/grammars-v4 at the pinned
// commit, so we use the vendored XML grammar — usvg's parser exercises the
// same XML well-formedness path before SVG-specific validation. Loaded via
// include_str! so the harness has no runtime dependency on the grammar files.
pub const XML_LEXER: &str = include_str!("../../../../../grammars/xml/XMLLexer.g4");
pub const XML_PARSER: &str = include_str!("../../../../../grammars/xml/XMLParser.g4");

pub fn grammar() -> &'static GrammarIr {
    static G: OnceLock<GrammarIr> = OnceLock::new();
    G.get_or_init(|| barkus_antlr::compile_split(XML_LEXER, XML_PARSER).expect("compile XML grammar"))
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
