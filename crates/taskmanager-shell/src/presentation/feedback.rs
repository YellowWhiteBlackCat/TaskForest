//! The one feedback-severity binding every frontend paints.
//!
//! A transient notice carries one severity. What that severity MEANS — which
//! semantic status colour and which semantic icon — is a product fact, so it
//! has exactly one address. The binding has one owner chain and one shape
//! split:
//!
//! - [`crate::app::FeedbackSeverity::status_kind`] projects the severity onto
//!   `StatusKind`; `StatusColors::resolve` selects the semantic ink for a
//!   graphical surface from that shared kind.
//! - `taskmanager_icons::status_icon` selects the semantic glyph from the same
//!   `StatusKind` for graphical surfaces. For text surfaces, [`status_marker`]
//!   owns the text decoration that carries that glyph's meaning: an
//!   informational notice has none, a successful one uses the success marker,
//!   and a warning or danger uses the alert marker.
//!
//! The frontend keeps only what is genuinely its own: the toolkit colour type,
//! and the shape it draws the glyph in. A terminal surface binds its theme
//! colour and `status_marker`; a graphical surface binds `status_icon`. No
//! frontend defines the severity projection, the graphical glyph mapping, or
//! the text-marker table.

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
