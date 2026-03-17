#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Test single-source compile
        let _ = barkus_antlr::compile(s);

        // Test split compile: use first newline as split point, falling back to midpoint
        let split = s.find('\n').unwrap_or(s.len() / 2);
        let split = (split..=s.len()).find(|&i| s.is_char_boundary(i)).unwrap_or(s.len());
        let (lexer, parser) = s.split_at(split);
        let _ = barkus_antlr::compile_split(lexer, parser);
    }
});
