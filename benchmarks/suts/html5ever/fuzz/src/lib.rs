//! Shared helpers for html5ever fuzz targets.
//!
//! Direct Rust use of barkus-core, so all three ValidityMode variants
//! are first-class (no M3 fallback).

use barkus_core::generate::decode;
use barkus_core::ir::GrammarIr;
use barkus_core::profile::{Profile, ValidityMode};
use html5ever::driver::ParseOpts;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::RcDom;
use std::sync::OnceLock;

// Vendored from antlr/grammars-v4 (html/HTMLLexer.g4, html/HTMLParser.g4).
// Embedded via include_str! so the harness has no runtime dependency on the
// grammar files.
pub const HTML_LEXER: &str = include_str!("../../../../../grammars/html/HTMLLexer.g4");
pub const HTML_PARSER: &str = include_str!("../../../../../grammars/html/HTMLParser.g4");

pub fn grammar() -> &'static GrammarIr {
    static G: OnceLock<GrammarIr> = OnceLock::new();
    G.get_or_init(|| barkus_antlr::compile_split(HTML_LEXER, HTML_PARSER).expect("compile HTML grammar"))
}

fn parse_html(s: &str) {
    let _ = html5ever::parse_document(RcDom::default(), ParseOpts::default()).one(s);
}

pub fn run_with_mode(tape: &[u8], mode: ValidityMode) {
    if tape.is_empty() {
        return;
    }
    let profile = Profile::builder().validity_mode(mode).build();
    let Ok((ast, _)) = decode(grammar(), &profile, tape) else {
        return;
    };
    let html = ast.serialize();
    let Ok(s) = std::str::from_utf8(&html) else {
        return;
    };
    parse_html(s);
}

pub fn run_str(s: &str) {
    parse_html(s);
}

pub fn run_raw(data: &[u8]) {
    let Ok(s) = std::str::from_utf8(data) else { return; };
    parse_html(s);
}
