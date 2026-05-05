#![no_main]

use libfuzzer_sys::fuzz_target;

// Raw baseline: feed fuzzer-emitted bytes directly into cssparser.
// No grammar, no struct layer.
fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else { return; };
    let mut parser_input = cssparser::ParserInput::new(s);
    let mut parser = cssparser::Parser::new(&mut parser_input);
    while parser.next().is_ok() {}
});
