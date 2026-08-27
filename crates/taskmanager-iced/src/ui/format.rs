//! Formatters and presentation helpers for the Iced frontend.
//!
//! Preference-aware quantity formatting delegates to the `taskmanager-core`
//! single source (`core::units`, reached through the `taskmanager-application`
//! re-export because the dependency firewall forbids a direct core edge in
//! this crate); the wrappers below keep the historical call shapes.

pub use taskmanager_shell::presentation::{
    MISSING_VALUE, bytes, duration, missing_value, optional_bytes, optional_count,
    optional_duration, optional_nice,
};

use taskmanager_application::units::format_quantity_with;

/// Format a memory quantity honoring the persisted unit preference: bytes
/// (`12.0 GiB`) or bits (`98.3 Gib`) — the bits form is 8× the byte value
/// with the same base-2 magnitude ladder.
#[must_use]
pub fn memory_text(value: u64, use_bytes: bool) -> String {
    format_quantity_with(value, use_bytes, true, false)
}

/// Format a memory quantity honoring both persisted preferences: the
/// bytes-vs-bits unit and the base-2 vs base-10 ladder (GPUI Settings Units
/// matrix parity). Delegates to the neutral core single source.
#[must_use]
pub fn memory_text_pref(value: u64, use_bytes: bool, use_base2: bool) -> String {
    format_quantity_with(value, use_bytes, use_base2, false)
}

/// Format a drive or network quantity (bytes or the 8× bits equivalent) on
/// the base-2 or base-10 ladder, following the same preference pair as
/// memory. Delegates to the neutral core single source.
#[must_use]
pub fn quantity_text_pref(value: u64, use_bytes: bool, use_base2: bool) -> String {
    format_quantity_with(value, use_bytes, use_base2, false)
}

/// Base-10 magnitude formatter: `KB`/`MB`/`GB` (bytes) or `Kb`/`Mb`/`Gb`
/// (bits) with the decimal 1000 ladder. Delegates to the neutral core single
/// source.
#[must_use]
pub fn base10_bytes(value: u64, as_bits: bool) -> String {
    format_quantity_with(value, !as_bits, false, false)
}

/// Base-2 bit magnitude formatter (the 8× byte value) with `b` suffixes:
/// `Kib`/`Mib`/`Gib`. Delegates to the neutral core single source.
#[must_use]
pub fn bits(value: u64) -> String {
    format_quantity_with(value, false, true, false)
}

#[cfg(test)]
#[path = "../../tests/gui/ui/format_tests.rs"]
mod tests;
