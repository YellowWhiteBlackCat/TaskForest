//! Cross-platform filename boundaries for deferred host-owned publications.

const MAX_FILENAME_BYTES: usize = 255;

/// Returns whether `value` is exactly one portable filename component.
///
/// Current-directory targets are resolved by a host worker, so accepting a
/// path here would let an otherwise renderer-neutral request escape the
/// worker's declared directory. The contract also rejects names that are
/// legal on one supported OS but are separators, device names, or aliases on
/// another.
pub(crate) fn is_single_filename(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_FILENAME_BYTES
        || value == "."
        || value == ".."
        || matches!(value.chars().last(), Some(' ' | '.'))
        || value.chars().any(char::is_control)
        || value.chars().any(|character| {
            matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        })
    {
        return false;
    }

    !is_windows_reserved_device_name(value)
}

fn is_windows_reserved_device_name(value: &str) -> bool {
    let base = value.split('.').next().unwrap_or(value);
    let uppercase = base.to_ascii_uppercase();
    matches!(uppercase.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || uppercase
            .strip_prefix("COM")
            .or_else(|| uppercase.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                suffix.len() == 1 && suffix.as_bytes()[0].is_ascii_digit() && suffix != "0"
            })
}
