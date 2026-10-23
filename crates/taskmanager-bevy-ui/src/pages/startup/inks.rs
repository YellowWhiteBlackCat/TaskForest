//! Startup ink overrides: the palette role a text node wears when the
//! shared type ladder's role ink is not the right one.
//!
//! The window's text-role observer stamps every text node's ink from its
//! typographic role (body ink, caption ink, mono ink). A page that needs a
//! SEMANTIC ink — the impact tier's color, a crashed unit's error red —
//! cannot declare it in the scene: the role observer owns the ink and
//! stamps it when the node lands. So the semantic ink rides its own typed
//! marker and this module's one system paints it from the live palette,
//! right after the scene paint (the crate's established pattern, the same
//! shape the table's zero-value dimming uses).
//!
//! The marker carries a ROLE, never a color: the palette stays the only
//! color source and a theme switch re-paints every marked node.

use bevy::app::{App, PostUpdate};
use bevy::ecs::component::Component;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, Res};
use bevy::text::TextColor;
use bevy::ui::UiSystems;

use crate::palette::UiPalette;
use crate::widgets::scene_paint::ScenePaint;
use crate::window::WindowPalette;

/// The palette role a marked text node wears.
///
/// The `Default` seed only exists for the bsn! template mechanism; a marked
/// node always carries an explicit role.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupInk(pub(crate) InkRole);

/// The palette roles the startup page paints with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InkRole {
    /// The theme's destructive/error ink — a crashed unit, a high boot cost.
    Danger,
    /// The theme's warning ink — a medium boot cost.
    Warning,
    /// The theme's CPU-series ink — the low boot cost tier.
    Cpu,
    /// The theme's dimmed ink — a fact the provider did not measure.
    #[default]
    Dim,
}

/// The palette ink of one role.
pub(crate) fn ink_color(role: InkRole, palette: &UiPalette) -> bevy::color::Color {
    match role {
        InkRole::Danger => palette.danger_color,
        InkRole::Warning => palette.warning_color,
        InkRole::Cpu => palette.cpu,
        InkRole::Dim => palette.dim_color,
    }
}

/// Register the ink painter: after the scene paint (so the marked nodes
/// exist) and before the layout (so the frame renders the semantic ink).
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        PostUpdate,
        paint_startup_inks
            .after(ScenePaint)
            .before(UiSystems::Layout),
    );
}

/// Paint every marked text node with its role's palette ink.
fn paint_startup_inks(
    palette: Res<WindowPalette>,
    mut marked: Query<(&StartupInk, &mut TextColor)>,
) {
    for (role, mut ink) in &mut marked {
        let selected = ink_color(role.0, &palette.inner);
        if ink.0.to_srgba() != selected.to_srgba() {
            ink.0 = selected;
        }
    }
}
