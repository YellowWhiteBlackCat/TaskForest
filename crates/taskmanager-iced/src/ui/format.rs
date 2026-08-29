//! Formatters and presentation helpers for the Iced frontend.
//!
//! Preference-aware quantity formatting delegates to the `taskmanager-core`
//! single source (`core::units`); the shared bytes/duration/missing-value
//! formatters live in [`taskmanager_shell::presentation`] and are imported
//! from that owner path at their call sites.

use taskmanager_core::core::units::format_quantity_with;

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

#[cfg(test)]
#[path = "../../tests/gui/ui/format_tests.rs"]
mod tests;
