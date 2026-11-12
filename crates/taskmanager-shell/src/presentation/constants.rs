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
/// that can drift. Graphical surfaces that cannot hold a decoration codepoint
/// paint an icon plate; the FORM differs by shape while the shared semantic
/// identity does not.
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
