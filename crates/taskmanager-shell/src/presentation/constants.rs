//! Small shared presentation constants.

/// The single missing-value placeholder every frontend renders for an
/// uncollected-but-applicable observation.
pub const MISSING_VALUE: &str = "—";

/// The alert marker a surface that paints text wears on a critical reading.
///
/// The marker's SEMANTIC identity belongs to the shared icon registry
/// ([`taskmanager_ui_contract::IconId::Alert`]); this is its TEXT form, for
/// the surfaces that may carry a decoration codepoint. A surface that may not
/// — the Bevy frontend's tofu law — paints an icon plate instead, so the FORM
/// differs by shape while the meaning does not.
///
/// One spelling, because a marker every surface spells for itself is a marker
/// that can drift. Measured at R138: nineteen live sites in three frontends
/// (GPUI eight, Iced six, TUI five) — the SMART wear-critical row, the spare-
/// pool row, the temperature label, the capability line, the source-notice
/// banner, the footer tone table, the toast prefix, and the parser that read
/// that prefix back into a severity. The fourth frontend carries none on
/// purpose: a surface that may not hold a decoration codepoint paints an icon
/// plate, so the FORM differs by shape while the meaning does not.
pub const ALERT_MARKER: &str = "⚠";

/// The success marker a surface that paints text wears on a positive
/// reading.
///
/// The marker's SEMANTIC identity belongs to the shared icon registry
/// ([`taskmanager_ui_contract::IconId::CircleCheck`]); this is its TEXT
/// form, for the surfaces that may carry a decoration codepoint — the same
/// FORM/meaning split [`ALERT_MARKER`] documents, with the shared status
/// ladder owning which severity wears which marker.
pub const SUCCESS_MARKER: &str = "✓";
