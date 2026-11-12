//! The one feedback-severity binding every frontend paints.
//!
//! A transient notice carries one severity. What that severity MEANS — which
//! semantic status colour and which semantic icon — is a product fact, so it
//! has exactly one address. The four frontends used to spell this binding
//! three different ways: GPUI matched the theme's accent/success/warning/
//! danger inks and answered the icon question from the product severity, Iced
//! answered the ink question through the shared status ladder but mapped the
//! icon from a *different* severity vocabulary, and Bevy matched the palette
//! directly. The three answers disagreed, so the same notice read as a
//! different colour and a different glyph per frontend.
//!
//! The binding now has two halves with two owners:
//!
//! - [`FeedbackSeverity::status_kind`] projects the product severity onto the
//!   shared transient-status vocabulary, so every frontend resolves the ink
//!   through one ladder (`StatusColors::resolve`).
//! - `taskmanager_icons::status_icon` answers the semantic glyph for that
//!   shared status kind.
//!
//! The frontend keeps only what is genuinely its own: the toolkit type it
//! projects the resolved colour onto, and the shape it draws the glyph in.

use zhuge_ui_tokens::StatusKind;

use super::constants::{ALERT_MARKER, SUCCESS_MARKER};

/// The text marker one transient status wears on a text surface.
///
/// The shared icon registry answers the glyph question for the graphical
/// frontends; a terminal surface carries the same meaning as a text
/// decoration instead. This table is that text form's single owner, so the
/// success check and the alert marker are the same facts the graphical
/// frontends draw as [`taskmanager_ui_contract::IconId::CircleCheck`] and
/// [`taskmanager_ui_contract::IconId::Alert`].
///
/// `None` means the severity wears no marker at all: an informational
/// notice is the copy itself, not a decoration.
#[must_use]
pub const fn status_marker(kind: StatusKind) -> Option<&'static str> {
    match kind {
        StatusKind::Info => None,
        StatusKind::Success => Some(SUCCESS_MARKER),
        StatusKind::Warning | StatusKind::Danger => Some(ALERT_MARKER),
    }
}

#[cfg(test)]
#[path = "../../tests/headless/shell_feedback_binding.rs"]
mod tests;
