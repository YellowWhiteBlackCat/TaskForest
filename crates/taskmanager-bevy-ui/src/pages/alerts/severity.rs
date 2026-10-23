//! The severity ink vocabulary for the alert and event surfaces.
//!
//! The palette itself belongs to `taskmanager-theme` (the severity palette);
//! this module owns only two things: the mapping from the core's alert
//! severity onto the theme's severity levels, and the conversion of the
//! palette's three tokens into bevy colors. The level→ink rule stays in the
//! theme, so a second severity table can never fork inside this frontend.

use crate::palette::{UiPalette, theme_color};
use bevy::color::Color;
use taskmanager_core::core::alerts::AlertSeverity;
use taskmanager_theme::palette::{SeverityLevel, SeverityPalette};

/// The theme's severity level for one core alert severity.
///
/// `Info` is the core's informational level; the product vocabulary calls it
/// *Notice*, so the mapping happens here once instead of in every surface.
#[must_use]
pub(crate) fn severity_level(severity: AlertSeverity) -> SeverityLevel {
    match severity {
        AlertSeverity::Critical => SeverityLevel::Critical,
        AlertSeverity::Warning => SeverityLevel::Warning,
        AlertSeverity::Info => SeverityLevel::Notice,
    }
}

/// The theme's severity palette for the live window palette.
///
/// The palette adapter carries the theme's own severity palette verbatim, so
/// the ink a severity surface paints is the owner's ink — not a table
/// rebuilt from converted channel values.
#[must_use]
pub(crate) fn severity_palette(palette: &UiPalette) -> SeverityPalette {
    palette.severity
}

/// The ink one severity paints.
#[must_use]
pub(crate) fn severity_ink(severity: AlertSeverity, palette: &UiPalette) -> Color {
    theme_color(severity_level(severity).ink(severity_palette(palette)))
}
