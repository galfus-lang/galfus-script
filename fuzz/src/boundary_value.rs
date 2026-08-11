#![no_main]

use bincode::Options;
use galfus_contract::BoundaryValue;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Bound deserialization so hostile inputs cannot cause unbounded allocation.
    let _ = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(64 * 1024)
        .deserialize::<BoundaryValue>(data);
});
