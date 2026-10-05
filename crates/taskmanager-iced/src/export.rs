//! Structured clipboard process export for Iced.

use taskmanager_core::core::process::ProcessItem;
use taskmanager_shell::presentation::{bytes, missing_value};

/// Format a single process item as tab-separated values (TSV).
#[must_use]
pub fn process_to_tsv(process: &ProcessItem) -> String {
    let user = process.current_user().unwrap_or_else(missing_value);
    let cpu = process
        .current_cpu_percentage()
        .map_or_else(missing_value, |value| format!("{value:.1}%"));
    let memory = process
        .current_memory_bytes()
        .map_or_else(missing_value, bytes);
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        process.pid,
        process.name,
        cpu,
        memory,
        if user.is_empty() { "—" } else { &user },
        process.status,
    )
}

/// One JSON string literal, escaped by serde (quotes, backslashes, and every
/// control character) instead of a hand-rolled rule.
fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

/// Format a process item as a clean JSON object string.
///
/// The field set (`pid`, `name`, `cpu_usage`, `memory_bytes`, `user`,
/// `status`, `cmdline`), their order, and the numeric spellings are the
/// published ones; every string literal is escaped by serde, so a command line
/// carrying quotes, newlines or other control characters still yields legal
/// JSON.
#[must_use]
pub fn process_to_json(process: &ProcessItem) -> String {
    let user = process
        .current_user()
        .map_or_else(|| "null".to_owned(), |user| json_string(&user));
    let cmdline = if process.cmdline.is_empty() {
        "null".to_owned()
    } else {
        json_string(&process.cmdline)
    };
    let cpu = process
        .current_cpu_percentage()
        .map_or_else(|| "null".to_owned(), |value| format!("{value:.1}"));
    let memory = process
        .current_memory_bytes()
        .map_or_else(|| "null".to_owned(), |value| value.to_string());
    format!(
        "{{\n  \"pid\": {},\n  \"name\": {},\n  \"cpu_usage\": {},\n  \"memory_bytes\": {},\n  \"user\": {},\n  \"status\": {},\n  \"cmdline\": {}\n}}",
        process.pid,
        json_string(&process.name),
        cpu,
        memory,
        user,
        json_string(&process.status),
        cmdline
    )
}

#[cfg(test)]
#[path = "../tests/gui/export_tests.rs"]
mod tests;
