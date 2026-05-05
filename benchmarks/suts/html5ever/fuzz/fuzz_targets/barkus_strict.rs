#![no_main]
use barkus_core::profile::ValidityMode;
use libfuzzer_sys::fuzz_target;
use html5ever_fuzz::run_with_mode;

fuzz_target!(|tape: &[u8]| {
    run_with_mode(tape, ValidityMode::Strict);
});
