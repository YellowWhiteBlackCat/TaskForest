//! Linux procfs boot identity helpers used by process enumeration.

use std::io;

use taskmanager_core::FailureKind;

fn parse_boot_time(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()
}

pub(super) fn read_boot_time_secs() -> Result<u64, FailureKind> {
    let text = std::fs::read_to_string("/proc/stat").map_err(|error| io_failure(&error))?;
    parse_boot_time(&text).ok_or(FailureKind::ProviderFault)
}

pub(super) fn io_failure(error: &io::Error) -> FailureKind {
    match error.kind() {
        io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        io::ErrorKind::NotFound
        | io::ErrorKind::Interrupted
        | io::ErrorKind::WouldBlock
        | io::ErrorKind::TimedOut => FailureKind::TemporarilyUnavailable,
        _ => FailureKind::ProviderFault,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_process_tree_tests.rs"]
mod tests;
