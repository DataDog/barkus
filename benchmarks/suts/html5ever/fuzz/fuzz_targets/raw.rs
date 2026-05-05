#![no_main]
use libfuzzer_sys::fuzz_target;
use html5ever_fuzz::run_raw;

fuzz_target!(|data: &[u8]| {
    run_raw(data);
});
