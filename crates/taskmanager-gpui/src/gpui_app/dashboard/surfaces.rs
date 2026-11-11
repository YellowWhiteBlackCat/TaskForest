//! Shared Dashboard surface roles.

use taskmanager_theme::{GlassSurfaceRole, Palette};
use taskmanager_ui::primitives::card_surface::CardSurface;

#[must_use]
pub(super) fn page_card(palette: Palette) -> CardSurface {
    CardSurface::for_role(palette, GlassSurfaceRole::PageCard)
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_app/dashboard/surface_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/gui/gpui_app/dashboard/alert_panel_bounds_tests.rs"]
mod alert_panel_bounds_tests;
