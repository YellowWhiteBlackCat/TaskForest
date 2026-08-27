#![no_main]
//! Fuzz target for command-output parsers (systemd-analyze blame and
//! loginctl list-sessions): output is produced by external binaries and is
//! untrusted input. The parser contract is total — any byte sequence must
//! yield a (possibly empty) result, never panic.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = taskmanager_platform_linux::parse_systemd_blame(&text);
    let _ = taskmanager_platform_linux::parse_systemd_critical_chain(&text);
    let _ = taskmanager_platform_linux::parse_systemd_failed_units(&text);
    let _ = taskmanager_platform_linux::parse_loginctl_sessions(&text);
});
