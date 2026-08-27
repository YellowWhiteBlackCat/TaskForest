#![no_main]
//! Fuzz target for the zram `mm_stat` parser: sysfs text with an open field
//! list and kernel-version-dependent shape, read once per zram device. The
//! parser contract is total — any byte sequence (any field count, value
//! range, or garbage) either yields all three counters or `None`, never a
//! panic and never a fabricated zero. The assertion pins the exact
//! specification the parser owns: the three leading whitespace-separated
//! tokens must each parse as u64.
use libfuzzer_sys::fuzz_target;

fn leading_u64(token: Option<&str>) -> Option<u64> {
    token?.parse::<u64>().ok()
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let got = taskmanager_platform_linux::parse_zram_mm_stat(&text);
    let expected = {
        let mut tokens = text.split_whitespace();
        match (
            leading_u64(tokens.next()),
            leading_u64(tokens.next()),
            leading_u64(tokens.next()),
        ) {
            (Some(orig), Some(compr), Some(mem_used)) => Some((orig, compr, mem_used)),
            _ => None,
        }
    };
    assert_eq!(
        got, expected,
        "mm_stat parse diverged from its specification"
    );
});
