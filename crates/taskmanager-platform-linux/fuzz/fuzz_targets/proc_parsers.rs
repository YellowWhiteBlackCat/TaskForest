#![no_main]
//! Fuzz target for the /proc text parsers: the provider reads kernel-owned
//! files whose content is fully untrusted input. The parser contract is
//! total — any byte sequence must parse or return a fallback, never panic.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = taskmanager_platform_linux::parse_proc_stat(&text);
    let _ = taskmanager_platform_linux::parse_proc_status_memory(&text);
    let _ = taskmanager_platform_linux::parse_proc_io(&text);
    let _ = taskmanager_platform_linux::parse_thread_stat(&text);
});
