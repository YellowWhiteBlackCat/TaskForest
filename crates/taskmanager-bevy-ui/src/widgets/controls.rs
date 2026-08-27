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
use bevy::picking::hover::PickingInteraction;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, UiRect, Val,
    percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Button;

use crate::palette::{UiPalette, space_2, space_4, space_8, space_12, space_16};
use crate::window::{Role, TextRole};

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
}

/// Stable visual state carried by every product-owned button. Interaction
/// itself remains Bevy's `Interaction`/`Pressed`; this component only records
/// the semantic selected bit and surface family for the shared visual system.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ControlVisual(pub(crate) ControlTone, pub(crate) bool);

/// Resolve one control fill from the theme tokens and Bevy 0.19 interaction
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
        }
    } else {
        match visual.0 {
            ControlTone::Nav => palette.nav_active_bg,
            ControlTone::Surface => palette.content_bg,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SurfaceTone {
    #[default]
    Elevated,
    Content,
}

fn surface_fill(tone: SurfaceTone, palette: &UiPalette) -> bevy::color::Color {
    match tone {
        SurfaceTone::Elevated => palette.panel_fill,
        SurfaceTone::Content => palette.content_bg,
    }
}

/// A token-backed panel that accepts a dynamic scene list as its children.
pub(crate) fn surface_scene(
    tone: SurfaceTone,
    children: Vec<Box<dyn Scene>>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_16())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ surface_fill(tone, palette) })
        Children [
            { children },
        ]
    }
}

/// A compact key/value row. The label is caption ink; the value is body ink
/// and remains the only mutable leaf in the row.
pub(crate) fn stat_row_scene(
    label: String,
    value: Box<dyn Scene>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            min_height: px(palette.control_height_px),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: Val::Px(space_8()),
            padding: UiRect::vertical(Val::Px(space_2())),
        }
        Children [
            ( Text(label) TextRole(Role::Caption) ),
            ( { value } ),
        ]
    }
}

/// Unstyled Bevy button plus the product pill skin. Interaction wiring belongs
/// to the caller so the same visual control can carry different typed events.
pub(crate) fn pill_scene(label: String, active: bool, palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node {
            min_width: px(palette.control_height_px * 2.8),
            height: px(palette.control_height_px),
            padding: UiRect::horizontal(Val::Px(space_12())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({
            if active { palette.nav_active_bg } else { palette.content_bg }
        })
        ControlVisual(ControlTone::Surface, active)
        Button
        Children [
            ( Text(label) TextRole(Role::Body) ),
        ]
    }
}

/// A selectable device row: identity on the first line, live caption below.
/// The selection fill comes from the same token as pills and the navigation
/// rail, so selected state is legible across the whole Performance surface.
#[allow(dead_code)]
pub(crate) fn device_row_scene(
    title: String,
    caption: Box<dyn Scene>,
    selected: bool,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            min_height: px(palette.control_height_px * 2.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            row_gap: Val::Px(space_2()),
            padding: UiRect::axes(Val::Px(space_12()), Val::Px(space_4())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({
            if selected { palette.nav_active_bg } else { palette.content_bg }
        })
        ControlVisual(ControlTone::Surface, selected)
        Button
        Children [
            ( Text(title) TextRole(Role::Body) ),
            ( { caption } ),
        ]
    }
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
            min_height: px(palette.control_height_px * 2.25),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::axes(Val::Px(space_12()), Val::Px(space_4())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({
            if selected { palette.nav_active_bg } else { palette.content_bg }
        })
        ControlVisual(ControlTone::Surface, selected)
        Button
        Children [
            ( { accessory } ),
            (
                Node {
                    flex_grow: 1.0,
                    min_width: px(0.0),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    row_gap: Val::Px(space_2()),
                }
                Children [
                    ( Text(title) TextRole(Role::Body) ),
                    ( { caption } ),
                ]
            ),
        ]
    }
}

/// One bounded per-core cell. The value is a caller-owned Scene so the page
/// can attach its typed dynamic marker without making this generic control
/// depend on a page module.
pub(crate) fn per_core_cell_scene(
    label: String,
    value: Box<dyn Scene>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            flex_grow: 1.0,
            width: px(palette.control_height_px * 4.5),
            min_width: px(palette.control_height_px * 4.5),
            min_height: px(palette.control_height_px * 2.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.content_bg })
        Children [
            ( Text(label) TextRole(Role::Caption) ),
            ( { value } ),
        ]
    }
}

/// Chrome around one graph body. The graph itself remains a separate Scene so
/// the chart renderer can evolve without changing the layout contract.
pub(crate) fn graph_card_scene(
    title: String,
    subtitle: String,
    graph: Box<dyn Scene>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_16())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ surface_fill(SurfaceTone::Content, palette) })
        Children [
            ( Text(title) TextRole(Role::Heading) ),
            ( Text(subtitle) TextRole(Role::Caption) ),
            ( { graph } ),
        ]
    }
}
