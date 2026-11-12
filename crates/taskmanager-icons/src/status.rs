//! The shared transient-status icon binding.
//!
//! A transient status carries one severity, and that severity wears one
//! semantic glyph. Which glyph is a product fact, so it has exactly one
//! address here: the icon registry owns the [`IconId`] vocabulary, and the
//! shared status ladder owns the severity it projects from.
//!
//! The four frontends used to answer this question three different ways. GPUI
//! matched the severity directly, Iced matched the shared status kind but
//! through a private copy of this table, and Bevy and the TUI had no glyph
//! answer at all. The same notice therefore read as a different picture per
//! frontend.

use taskmanager_ui_contract::IconId;
use zhuge_ui_tokens::StatusKind;

/// The semantic icon that represents one transient status severity.
///
/// One glyph per severity, so the notice reads the same way in every
/// frontend. A renderer that cannot draw the glyph paints its text form
/// instead; the meaning stays shared either way.
#[must_use]
pub const fn status_icon(kind: StatusKind) -> IconId {
    match kind {
        StatusKind::Info => IconId::Health,
        StatusKind::Success => IconId::CircleCheck,
        StatusKind::Warning => IconId::TriangleAlert,
        StatusKind::Danger => IconId::Alert,
    }
}

#[cfg(test)]
#[path = "../tests/headless/status_icon.rs"]
mod tests;
