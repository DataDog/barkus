#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|s: &str| {
    let _ = usvg::Tree::from_str(s, &usvg::Options::default());
});
