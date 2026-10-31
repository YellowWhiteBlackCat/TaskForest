//! Startup impact badge: the three-tier boot-cost mark (BEVY-145).
//!
//! GPUI parity: the impact cell is a colored badge, not plain text — `High`
//! wears the error red, `Medium` the amber, `Low` the blue, and an
//! unclassified entry the dim ink. The Bevy palette publishes no `gpu` ink
//! (the GPUI skin's purple), so the three tiers map onto the palette's own
//! semantic inks: `danger_color`, `warning_color` and `cpu`. Every ink is a
//! palette token; no literal color exists in this module.
//!
//! The badge is a colored COLUMN plus the impact label — a bar-shaped
//! marker, not a text glyph, so the tier survives a clipped or translated
//! label. The column's extent rides the rem axis, so the badge re-scales
//! with the reading-size step like the rest of the type ladder.

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, Node, Overflow, UiRect, Val, px, rem,
};
use bevy::ui::widget::Text;
use taskforest_core::core::startup::StartupImpact;

use super::inks::{InkRole, StartupInk};
use super::tinted;
use crate::palette::{UiPalette, space_2, space_4, space_8};
use crate::window::{Role, TextRole};

/// The boot-cost tiers the impact badge distinguishes.
///
/// The `Default` seed only exists for the bsn! template mechanism; a badge
/// always carries an explicit tier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ImpactTier {
    /// The entry measurably dominates the boot cost.
    High,
    /// The entry contributes a noticeable share of the boot cost.
    Medium,
    /// The entry's boot cost is small.
    Low,
    /// The provider did not classify the entry's boot cost.
    #[default]
    Unclassified,
}

impl ImpactTier {
    /// The tier of one startup entry's shared impact classification.
    pub(crate) fn of(impact: StartupImpact) -> Self {
        match impact {
            StartupImpact::High => Self::High,
            StartupImpact::Medium => Self::Medium,
            StartupImpact::Low => Self::Low,
            StartupImpact::None => Self::Unclassified,
        }
    }
}

/// The palette ink of one tier — the single color source of the badge.
pub(crate) fn impact_tier_color(tier: ImpactTier, palette: &UiPalette) -> Color {
    match tier {
        ImpactTier::High => palette.danger_color,
        ImpactTier::Medium => palette.warning_color,
        ImpactTier::Low => palette.cpu,
        ImpactTier::Unclassified => palette.dim_color,
    }
}

/// The ink role of one tier — the badge's label wears it through the
/// page's ink painter (the shared type role owns the plain ink).
pub(crate) fn impact_tier_ink(tier: ImpactTier) -> InkRole {
    match tier {
        ImpactTier::High => InkRole::Danger,
        ImpactTier::Medium => InkRole::Warning,
        ImpactTier::Low => InkRole::Cpu,
        ImpactTier::Unclassified => InkRole::Dim,
    }
}

/// Marker on the impact badge: the tier the badge shows.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupImpactBadge(pub(crate) ImpactTier);

/// Marker on the badge's colored column: the tier mark itself.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupImpactMark(pub(crate) ImpactTier);

/// The impact badge: a tier-colored column plus the impact label.
///
/// The label keeps the shared impact text (its tier word plus its evidence
/// — a measured duration or the honest "unmeasured" marker), so the badge
/// never hides what the inventory already proved.
pub(crate) fn impact_badge_scene(
    tier: ImpactTier,
    label: String,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let ink = impact_tier_color(tier, palette);
    let fill = tinted(ink, 0.2);
    let label_ink = impact_tier_ink(tier);
    Box::new(bsn! {
        Node {
            height: rem(palette.control_height_rem),
            padding: UiRect::horizontal(Val::Px(space_8())),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
            overflow: Overflow::clip_x(),
        }
        BackgroundColor(fill)
        Children [
             Node {
                 width: rem(palette.control_height_rem * 0.18),
                 height: rem(palette.control_height_rem * 0.72),
                 flex_shrink: 0.0,
                 border_radius: BorderRadius::all(Val::Px(space_2())),
             }
             BackgroundColor(ink)
             StartupImpactMark(tier)
            --
             Text(label) TextRole(Role::Caption) StartupImpactBadge(tier) StartupInk(label_ink)
             TextLayout { linebreak: LineBreak::NoWrap }
        ]
    })
}

/// The impact cell: the badge inside the inventory's bounded column slot.
pub(crate) fn impact_badge_cell_scene(
    tier: ImpactTier,
    label: String,
    width: f32,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            width: px(width),
            align_items: AlignItems::Center,
            overflow: Overflow::clip_x(),
        }
        Children [
             @{ impact_badge_scene(tier, label, palette) }
        ]
    }
}
