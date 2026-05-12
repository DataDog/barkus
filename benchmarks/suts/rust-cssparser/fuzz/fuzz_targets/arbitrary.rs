#![no_main]

use libfuzzer_sys::fuzz_target;

// Arbitrary variant: libfuzzer-sys uses arbitrary::Arbitrary for &str so the
// fuzzer is restricted to valid UTF-8 inputs. Distinct from `raw` (which
// accepts arbitrary byte sequences) and gives cssparser the "best raw-bytes
// baseline" since it never wastes iterations on non-UTF8 inputs.
fuzz_target!(|s: &str| {
    let mut parser_input = cssparser::ParserInput::new(s);
    let mut parser = cssparser::Parser::new(&mut parser_input);
    while parser.next().is_ok() {}
});
