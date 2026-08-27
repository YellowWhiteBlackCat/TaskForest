#![no_main]

use libfuzzer_sys::fuzz_target;

// The five-tuple parser is the crate's unsafe-facing entry point for
// UNTRUSTED input: raw bytes from the wire, never validated by anything
// before us. The parser contract is total — any byte sequence must either
// yield a tuple or None, never panic, never index out of bounds.
fuzz_target!(|data: &[u8]| {
    let _ = taskmanager_afpacket::five_tuple(data);
});
