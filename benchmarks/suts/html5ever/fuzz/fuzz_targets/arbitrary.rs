#![no_main]
use libfuzzer_sys::fuzz_target;
use html5ever_fuzz::run_str;

fuzz_target!(|s: &str| {
    run_str(s);
});
