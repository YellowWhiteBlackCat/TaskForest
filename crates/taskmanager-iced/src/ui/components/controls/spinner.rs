//! Component-layer access to the single warm-up spinner owner.
//!
//! `crate::ui::spinner` owns the canvas program, arc math, palette binding
//! and production footprint. This module only re-surfaces that owner to the
//! component family; a second canvas implementation would duplicate a fact.

use taskmanager_theme::Theme;

use crate::ui::components::IcedElement;

/// The component family's named spinner entry. The production warm-up body
/// consumes this wrapper so the canvas program has one owner and one caller.
#[must_use]
pub(crate) fn spinner<'a>(theme_snapshot: &'a Theme, phase: Option<f32>) -> IcedElement<'a> {
    crate::ui::spinner::canvas_view(theme_snapshot, phase)
}
