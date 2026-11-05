//! Owned BSN controls shared by the Performance foundation.
//!
//! Bevy's official widgets supply behavior primitives (`Button`, `Activate`,
//! focus and scroll semantics). This module supplies the product language:
//! surfaces, pills, stat rows, device rows and graph-card chrome. Callers pass
//! already-composed `Scene` values; the module never creates children through
//! commands or a builder API.

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::picking::Pickable;
use bevy::picking::hover::PickingInteraction;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderColor, BorderRadius, BoxShadow, FlexDirection,
    JustifyContent, Node, Overflow, UiRect, Val, percent, px, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Button;
use zhuge_ui_kit_bevy::control::{ButtonLayout, button_scene};

use crate::glass::{GlassPanelMaterial, card_material};
use crate::palette::{UiPalette, space_2, space_4, space_8, space_12, space_16};
use crate::widgets::bounded_text::BoundedText;
use crate::window::{Role, TextRole};
use bevy::text::{LineBreak, TextLayout};

/// The sortable-header indicator: a semantic direction plate (arrow-up /
/// arrow-down) tinted with the dim ink, or empty when the sort rests
/// elsewhere. The arrow is an image, never a text codepoint — a glyph the
/// embedded faces do not guarantee is exactly how tofu bugs ship. It flows
/// INLINE with the column label: bevy_text reserves its box inside the text
/// layout, so label and direction clip, align and shift as one line under the
/// header's bounded-line discipline — never a flex sibling that outlives a
/// clipped label.
pub(crate) fn sort_indicator_scene(
    descending: Option<bool>,
    palette: &UiPalette,
) -> Vec<Box<dyn Scene>> {
    use taskmanager_ui_contract::IconId;
    let icon = match descending {
        Some(true) => IconId::NavigateDown,
        Some(false) => IconId::NavigateUp,
        None => return Vec::new(),
    };
    vec![crate::icons::inline_icon_scene(
        icon,
        palette,
        palette.dim_color,
    )]
}

/// Which idle/selected surface a shared interactive control belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ControlTone {
    /// Navigation uses the accent for the active route and the sidebar card
    /// surface for an idle item.
    Nav,
    /// Page controls use the sidebar-card surface for the selected item and
    /// the content surface while idle.
    #[default]
    Surface,
    /// A table row. `zebra` carries the row's parity inside the visible
    /// window, which is the only place the stripe has meaning: the stripe
    /// alternates what the user can see, not what the model indexes.
    Row { zebra: bool },
}

/// Stable visual state carried by every product-owned button. Interaction
/// itself remains Bevy's `Interaction`/`Pressed`; this component only records
/// the semantic selected bit and surface family for the shared visual system.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ControlVisual(pub(crate) ControlTone, pub(crate) bool);

/// Resolve one control fill from the theme tokens and Bevy 0.20 interaction
/// state. Hover and pressed are transient; selected is the page-owned state.
pub(crate) fn control_background(
    visual: &ControlVisual,
    interaction: PickingInteraction,
    pressed: bool,
    palette: &UiPalette,
) -> Color {
    if pressed || interaction == PickingInteraction::Pressed {
        return palette.selection_bg;
    }
    if interaction == PickingInteraction::Hovered {
        return palette.hover_bg;
    }
    if visual.1 {
        match visual.0 {
            ControlTone::Nav => palette.accent,
            ControlTone::Surface => palette.nav_active_bg,
            ControlTone::Row { .. } => palette.selection_bg,
        }
    } else {
        match visual.0 {
            ControlTone::Nav => palette.nav_active_bg,
            ControlTone::Surface => palette.content_bg,
            // A table row is not a control surface: the reference surface
            // leaves an unselected row open so the page canvas reads through
            // it, and marks every other row with the theme's zebra tint for
            // scanning. Painting every row with a panel fill turns the whole
            // table into one flat block and erases the stripe.
            ControlTone::Row { zebra: true } => palette.zebra_bg,
            ControlTone::Row { zebra: false } => Color::NONE,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SurfaceTone {
    #[default]
    Elevated,
    Content,
}

/// A token-backed panel that accepts a dynamic scene list as its children.
pub(crate) fn surface_scene(
    tone: SurfaceTone,
    children: Vec<Box<dyn Scene>>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let elevated = tone == SurfaceTone::Elevated;
    let surface_gap_rem = palette.rem_ratio(space_2());
    let surface_padding_rem = palette.rem_ratio(space_16());
    let radius_rem = palette.panel_radius_rem();
    let (fill, edge, shadows, edge_width_rem, grain) = if elevated {
        let GlassPanelMaterial {
            fill,
            edge,
            shadows,
            edge_width_rem,
            grain,
            ..
        } = card_material(palette);
        (fill, edge, shadows, edge_width_rem, Some(grain))
    } else {
        (
            palette.content_bg,
            palette.border_color,
            Vec::new(),
            0.0,
            None,
        )
    };
    bsn! {
        Node {
            width: percent(100),
            min_width: Val::Auto,
            max_width: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Rem(surface_gap_rem),
            padding: UiRect::all(Val::Rem(surface_padding_rem)),
            border: UiRect::all(Val::Rem(edge_width_rem)),
            border_radius: BorderRadius::all(Val::Rem(radius_rem)),
        }
        BackgroundColor({ fill })
        BorderColor::all(edge)
        BoxShadow({ shadows })
        Children [
            @{ grain } --
            { children }
        ]
    }
}

/// A compact key/value row. The label is caption ink; the value is body ink
/// and remains the only mutable leaf in the row. Both columns are strictly
/// single-line: the value clips at the row edge instead of wrapping the row.
pub(crate) fn stat_row_scene(
    label: String,
    value: Box<dyn Scene>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            min_height: rem(palette.control_height_rem),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: Val::Px(space_8()),
            padding: UiRect::vertical(Val::Px(space_2())),
        }
        Children [
                Node {
                    min_width: px(0.0),
                    flex_shrink: 1.0,
                    overflow: Overflow::clip_x(),
                }
                Children [  Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }  ]
            --
                Node {
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::FlexEnd,
                    overflow: Overflow::clip_x(),
                }
                Children [  @{ value }  ]
        ]
    }
}

/// A complete inspection fact: the label and wrapped value own separate rows.
pub(crate) fn detail_row_scene(
    label: String,
    value: Box<dyn Scene>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100), min_width: px(0.0), min_height: rem(palette.control_height_rem), flex_shrink: 0.0,
            flex_direction: FlexDirection::Column, row_gap: px(space_2()),
            padding: UiRect::vertical(px(space_2())) }
        Children [
            Text(label) TextRole(Role::Caption) Node { width: percent(100), min_width: px(0.0) } --
            Node { width: percent(100), min_width: px(0.0) } Children [ @{ value } ]
        ]
    }
}

/// Unstyled Bevy button plus the product pill skin. Interaction wiring belongs
/// to the caller so the same visual control can carry different typed events.
pub(crate) fn pill_scene(label: String, active: bool, palette: &UiPalette) -> impl Scene + use<> {
    let scene = button_scene(
        ButtonLayout {
            min_width: rem(palette.control_height_rem * 2.8),
            height: rem(palette.control_height_rem),
            padding: UiRect::horizontal(Val::Px(space_12())),
            border: UiRect::default(),
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        },
        if active {
            palette.nav_active_bg
        } else {
            palette.content_bg
        },
        BorderColor::default(),
        bsn! { Text(label) TextRole(Role::Body) },
    );
    bsn! { @{ scene } ControlVisual(ControlTone::Surface, active) }
}

/// Device row variant with a bounded visual accessory. The accessory owns its
/// own chart/icon scene; this control only guarantees the same shrinkable
/// identity column and selection surface used by the plain row.
pub(crate) fn device_row_with_accessory_scene(
    title: String,
    accessory: Box<dyn Scene>,
    caption: Box<dyn Scene>,
    selected: bool,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            min_height: rem(palette.control_height_rem * 2.25),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_4())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({
            if selected { palette.nav_active_bg } else { palette.content_bg }
        })
        ControlVisual(ControlTone::Surface, selected)
        Button
        Children [
             @{ accessory } Pickable::IGNORE --

                Node {
                    flex_grow: 1.0,
                    min_width: px(0.0),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    row_gap: Val::Px(space_2()),
                    // Both lines of the identity column are bounded slots:
                    // the ellipsis system needs the line's own shaped width,
                    // which a stretched line never reports.
                    align_items: AlignItems::FlexStart,
                    overflow: Overflow::clip_x(),
                }
                Pickable::IGNORE
                Children [
                     Text(title) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap } BoundedText Pickable::IGNORE --
                     @{ caption } Pickable::IGNORE
                ]

        ]
    }
}

/// Chrome around one graph body. The graph itself remains a separate Scene so
/// the chart renderer can evolve without changing the layout contract. The
/// title stays at caption scale — section chrome, not a second page heading;
/// the page owns exactly one heading.
pub(crate) fn graph_card_scene(
    title: String,
    subtitle: String,
    graph: Box<dyn Scene>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let GlassPanelMaterial {
        fill,
        edge,
        shadows: _,
        edge_width_rem,
        radius_rem,
        grain,
    } = card_material(palette);
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_16())),
            border: UiRect::all(Val::Rem(edge_width_rem)),
            border_radius: BorderRadius::all(Val::Rem(radius_rem)),
        }
        BackgroundColor({ fill })
        BorderColor::all(edge)
        Children [
             @{ grain } --
             Text(title) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } --
             Text(subtitle) TextRole(Role::Caption) --
             @{ graph }
        ]
    }
}
