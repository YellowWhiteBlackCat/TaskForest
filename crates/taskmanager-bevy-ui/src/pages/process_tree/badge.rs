//! The row badges: the aggregate count and the zombie marker.
//!
//! A group row states how many processes it aggregates, and a defunct
//! process states that it is a zombie. Both are chips with their own marker
//! component, so the bounded text cells of a row keep their exact count and
//! their own meaning.

use bevy::color::{Alpha, Color};
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::system::{Query, Res};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn};
use bevy::text::TextFont;
use bevy::ui::prelude::{AlignItems, BackgroundColor, CornerRadius, Node, UiRect, px};
use bevy::ui::widget::Text;

use taskmanager_application::i18n::t;

use crate::palette::{UiPalette, space_4};
use crate::window::{Role, TextRole, WindowPalette};

/// The alpha of the zombie chip's muted fill.
const ZOMBIE_BADGE_ALPHA: f32 = 0.28;

/// The aggregate count chip of a group row (BEVY-127).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TreeAggregateBadge(pub(crate) String);

/// The zombie marker chip of a defunct process row (BEVY-129).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TreeZombieBadge;

/// The aggregate reading of a group row (BEVY-127).
///
/// The marker names the text node that carries a group total, so the weight
/// system can state a total stronger than one process's own reading without
/// the scene inventing a type role.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TreeAggregateValue;

/// The aggregate count chip of a group row.
pub(super) fn aggregate_badge_scene(label: String, palette: &UiPalette) -> Box<dyn Scene> {
    let chip = chip_scene(label.clone(), palette.selection_bg, palette);
    Box::new(bsn! {
        Node {
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
        }
        TreeAggregateBadge({ label })
        Pickable::IGNORE
        Children [@{ chip }]
    })
}

/// The zombie marker chip of a defunct process row.
pub(super) fn zombie_badge_scene(palette: &UiPalette) -> Box<dyn Scene> {
    let muted = palette.scrim.with_alpha(ZOMBIE_BADGE_ALPHA);
    let chip = chip_scene(t("proc.status_zombie").to_owned(), muted, palette);
    Box::new(bsn! {
        Node {
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
        }
        TreeZombieBadge
        Pickable::IGNORE
        Children [@{ chip }]
    })
}

/// One badge chip: a muted pill carrying its label.
fn chip_scene(label: String, fill: Color, palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            padding: UiRect::horizontal(px(space_4())),
            border_radius: CornerRadius::all(px(palette.control_radius_px)),
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
        }
        BackgroundColor({ fill })
        Pickable::IGNORE
        Children [
            Text({ label }) TextRole(Role::Caption) Pickable::IGNORE
        ]
    })
}

/// State a group total stronger than one process's own reading.
///
/// The weight comes from the palette's heading token, so the emphasis rides
/// the theme's type ladder instead of a hardcoded number.
pub(super) fn emphasize_aggregate_values(
    palette: Res<WindowPalette>,
    mut values: Query<&mut TextFont, With<TreeAggregateValue>>,
) {
    let weight = palette.inner.heading.weight;
    for mut font in &mut values {
        if font.weight != weight {
            font.weight = weight;
        }
    }
}
