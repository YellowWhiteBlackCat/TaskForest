//! The shared transient-status icon binding.
//!
//! A transient status carries one severity, and that severity wears one
//! semantic glyph. Which glyph is a product fact, so it has exactly one
//! address here: the icon registry owns the [`IconId`] vocabulary, and the
//! shared status ladder owns the severity it projects from.
//!
//! Graphical frontends bind the same [`IconId`] from this table; a text
//! surface uses the shell owner's marker form. No frontend owns a second
//! severity-to-glyph correspondence.

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
