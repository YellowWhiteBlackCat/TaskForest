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
use bevy::scene::{Scene, bsn, template_value};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Button;

use crate::palette::no_wrap_text;
use crate::palette::{UiPalette, space_2, space_4, space_8, space_12, space_16};
use crate::window::{Role, TextRole};

/// The sortable-header indicator: a semantic direction plate (arrow-up /
/// arrow-down) tinted with the dim ink, or empty when the sort rests
/// elsewhere. The arrow is an image, never a text codepoint — a glyph the
/// embedded faces do not guarantee is exactly how tofu bugs ship.
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
    vec![crate::icons::icon_scene(icon, 12.0, palette.dim_color)]
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
            min_height: px(palette.control_height_px),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: Val::Px(space_8()),
            padding: UiRect::vertical(Val::Px(space_2())),
        }
        Children [
            (
                Node {
                    min_width: px(0.0),
                    flex_shrink: 1.0,
                    overflow: Overflow::clip_x(),
                }
                Children [ ( Text(label) TextRole(Role::Caption) template_value(no_wrap_text()) ) ]
            ),
            (
                Node {
                    min_width: px(0.0),
                    flex_shrink: 1.0,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::FlexEnd,
                    overflow: Overflow::clip_x(),
                }
                Children [ ( { value } ) ]
            ),
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
                    overflow: Overflow::clip_x(),
                }
                Children [
                    ( Text(title) TextRole(Role::Body) template_value(no_wrap_text()) ),
                    ( { caption } ),
                ]
            ),
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
            ( Text(title) TextRole(Role::Caption) template_value(no_wrap_text()) ),
            ( Text(subtitle) TextRole(Role::Caption) ),
            ( { graph } ),
        ]
    }
}

/// Tooltip placement relative to its anchor target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum TooltipPlacement {
    /// Placed above the anchor target.
    #[default]
    Top,
    /// Placed below the anchor target.
    Bottom,
    /// Placed to the left of the anchor target.
    Left,
    /// Placed to the right of the anchor target.
    Right,
}

/// A tooltip's neutral content and layout specification.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct TooltipSpec {
    pub(crate) text: String,
    pub(crate) key_hint: Option<String>,
    pub(crate) placement: TooltipPlacement,
}

#[allow(dead_code)]
impl TooltipSpec {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            key_hint: None,
            placement: TooltipPlacement::Top,
        }
    }

    #[must_use]
    pub(crate) fn with_key_hint(mut self, hint: impl Into<String>) -> Self {
        self.key_hint = Some(hint.into());
        self
    }

    #[must_use]
    pub(crate) fn with_placement(mut self, placement: TooltipPlacement) -> Self {
        self.placement = placement;
        self
    }
}

/// Pure visibility state machine for an anchored tooltip.
/// Shows when either pointer hover or keyboard focus is active.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct TooltipState {
    pub(crate) hovered: bool,
    pub(crate) focused: bool,
}

#[allow(dead_code)]
impl TooltipState {
    #[must_use]
    pub(crate) fn is_visible(&self) -> bool {
        self.hovered || self.focused
    }

    pub(crate) fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    pub(crate) fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

/// An explanation banner styled with caption typography and theme tokens.
/// When `key_hint` is provided, renders the shortcut hint alongside the text.
#[allow(dead_code)]
pub(crate) fn tooltip_scene(spec: &TooltipSpec, palette: &UiPalette) -> impl Scene + use<> {
    let text = spec.text.clone();
    let hint = spec.key_hint.clone();
    let radius = palette.control_radius_px;
    let bg = palette.panel_fill;
    let hint_scenes: Vec<Box<dyn Scene>> = if let Some(h) = hint {
        vec![Box::new(bsn! {
            Node {
                padding: UiRect::left(Val::Px(space_4())),
            }
            Children [
                ( Text(h) TextRole(Role::Caption) template_value(no_wrap_text()) )
            ]
        })]
    } else {
        Vec::new()
    };

    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
            padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_4())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor(bg)
        Children [
            ( Text(text) TextRole(Role::Caption) template_value(no_wrap_text()) ),
            { hint_scenes }
        ]
    }
}

/// An anchored tooltip wrapper that conditionally presents the explanation
/// banner when active, offset according to its configured placement.
#[allow(dead_code)]
pub(crate) fn tooltip_anchored_scene(
    spec: &TooltipSpec,
    state: &TooltipState,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let tip = tooltip_scene(spec, palette);
    let scenes: Vec<Box<dyn Scene>> = if state.is_visible() {
        vec![Box::new(tip)]
    } else {
        Vec::new()
    };
    let margin = match spec.placement {
        TooltipPlacement::Top => UiRect::bottom(Val::Px(space_4())),
        TooltipPlacement::Bottom => UiRect::top(Val::Px(space_4())),
        TooltipPlacement::Left => UiRect::right(Val::Px(space_4())),
        TooltipPlacement::Right => UiRect::left(Val::Px(space_4())),
    };

    bsn! {
        Node {
            margin: margin,
            align_items: AlignItems::Center,
        }
        Children [
            { scenes }
        ]
    }
}

/// Pure state model for a bounded numeric slider control.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub(crate) struct SliderState {
    pub(crate) min: f32,
    pub(crate) max: f32,
    pub(crate) step: f32,
    pub(crate) value: f32,
}

#[allow(dead_code)]
impl SliderState {
    pub(crate) fn new(min: f32, max: f32, step: f32, value: f32) -> Self {
        let span = (max - min).abs().max(1e-6);
        let actual_max = min + span;
        let clamped = value.clamp(min, actual_max);
        let mut s = Self {
            min,
            max: actual_max,
            step: if step > 0.0 { step } else { span / 20.0 },
            value: clamped,
        };
        s.set_value(clamped);
        s
    }

    #[must_use]
    pub(crate) fn clamped_value(&self) -> f32 {
        self.value.clamp(self.min, self.max)
    }

    /// Progress fraction in [0.0, 1.0].
    #[must_use]
    pub(crate) fn fraction(&self) -> f32 {
        let span = (self.max - self.min).max(1e-6);
        ((self.clamped_value() - self.min) / span).clamp(0.0, 1.0)
    }

    /// Step forward by `count` step increments, clamped.
    pub(crate) fn step_forward(&mut self, count: f32) -> f32 {
        self.set_value(self.value + self.step * count)
    }

    /// Step backward by `count` step increments, clamped.
    pub(crate) fn step_backward(&mut self, count: f32) -> f32 {
        self.set_value(self.value - self.step * count)
    }

    /// Set value from a normalized fraction in [0.0, 1.0].
    pub(crate) fn set_fraction(&mut self, fraction: f32) -> f32 {
        let clamped_frac = fraction.clamp(0.0, 1.0);
        let raw = self.min + clamped_frac * (self.max - self.min);
        self.set_value(raw)
    }

    /// Set value directly, snapping to nearest step and clamping to [min, max].
    pub(crate) fn set_value(&mut self, value: f32) -> f32 {
        self.value = snap_slider_value(value, self.min, self.max, self.step);
        self.value
    }
}

#[allow(dead_code)]
pub(crate) fn snap_slider_value(val: f32, min: f32, max: f32, step: f32) -> f32 {
    if step <= 0.0 {
        val.clamp(min, max)
    } else {
        let steps = ((val - min) / step).round();
        (min + steps * step).clamp(min, max)
    }
}

/// A bounded numeric slider control scene with track, active progress fill,
/// and value readout caption.
#[allow(dead_code)]
pub(crate) fn slider_scene(state: &SliderState, palette: &UiPalette) -> impl Scene + use<> {
    let frac = state.fraction();
    let fill_pct = frac * 100.0;
    let radius = palette.control_radius_px;
    let track_fill = palette.nav_bg;
    let active_fill = palette.accent;
    let value_str = format!("{:.1}", state.clamped_value());

    bsn! {
        Node {
            width: percent(100),
            height: px(palette.control_height_px),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_2())),
        }
        Children [
            (
                Node {
                    flex_grow: 1.0,
                    height: px(space_8()),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(radius)),
                    overflow: Overflow::clip_x(),
                }
                BackgroundColor(track_fill)
                Children [
                    (
                        Node {
                            width: percent(fill_pct),
                            height: percent(100),
                            border_radius: BorderRadius::all(Val::Px(radius)),
                        }
                        BackgroundColor(active_fill)
                    ),
                    (
                        Node {
                            width: px(12.0),
                            height: px(12.0),
                            border_radius: BorderRadius::all(Val::Px(6.0)),
                        }
                        BackgroundColor(active_fill)
                    ),
                ]
            ),
            (
                Node {
                    min_width: px(40.0),
                    justify_content: JustifyContent::FlexEnd,
                }
                Children [
                    ( Text(value_str) TextRole(Role::Caption) template_value(no_wrap_text()) )
                ]
            ),
        ]
    }
}

/// Orientation of a scrollbar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ScrollbarOrientation {
    #[default]
    Vertical,
    Horizontal,
}

/// Resolved geometry for a visible scrollbar rail and thumb.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub(crate) struct ScrollbarGeometry {
    pub(crate) rail_length_px: f32,
    pub(crate) thumb_offset_px: f32,
    pub(crate) thumb_size_px: f32,
    pub(crate) visible: bool,
}

#[allow(dead_code)]
impl ScrollbarGeometry {
    /// Normalized scroll fraction in [0.0, 1.0].
    #[must_use]
    pub(crate) fn scroll_ratio(&self) -> f32 {
        let track_span = (self.rail_length_px - self.thumb_size_px).max(0.0);
        if track_span <= 0.0 {
            0.0
        } else {
            (self.thumb_offset_px / track_span).clamp(0.0, 1.0)
        }
    }
}

#[allow(dead_code)]
pub(crate) const MIN_SCROLLBAR_THUMB_PX: f32 = 18.0;
#[allow(dead_code)]
pub(crate) const SCROLLBAR_THICKNESS_PX: f32 = 6.0;

/// Compute scrollbar geometry from viewport size, content extent, scroll offset,
/// rail length, and minimum thumb size.
#[allow(dead_code)]
pub(crate) fn compute_scrollbar_geometry(
    viewport_size: f32,
    content_size: f32,
    scroll_offset: f32,
    rail_length: f32,
    min_thumb: f32,
) -> ScrollbarGeometry {
    if content_size <= 0.0 || viewport_size <= 0.0 || rail_length <= 0.0 {
        return ScrollbarGeometry {
            rail_length_px: rail_length.max(0.0),
            thumb_offset_px: 0.0,
            thumb_size_px: rail_length.max(0.0),
            visible: false,
        };
    }

    if content_size <= viewport_size {
        return ScrollbarGeometry {
            rail_length_px: rail_length,
            thumb_offset_px: 0.0,
            thumb_size_px: rail_length,
            visible: false,
        };
    }

    let max_scroll = (content_size - viewport_size).max(1.0);
    let clamped_scroll = scroll_offset.clamp(0.0, max_scroll);
    let view_ratio = (viewport_size / content_size).clamp(0.0, 1.0);
    let ideal_thumb = rail_length * view_ratio;
    let thumb_size = ideal_thumb.clamp(min_thumb.min(rail_length), rail_length);
    let track_span = (rail_length - thumb_size).max(0.0);
    let scroll_fraction = clamped_scroll / max_scroll;
    let thumb_offset = track_span * scroll_fraction;

    ScrollbarGeometry {
        rail_length_px: rail_length,
        thumb_offset_px: thumb_offset,
        thumb_size_px: thumb_size,
        visible: true,
    }
}

/// Map a drag position along the rail back to a scroll offset.
#[allow(dead_code)]
pub(crate) fn thumb_drag_to_scroll(
    thumb_offset_px: f32,
    rail_length: f32,
    thumb_size: f32,
    viewport_size: f32,
    content_size: f32,
) -> f32 {
    let max_scroll = (content_size - viewport_size).max(0.0);
    let track_span = (rail_length - thumb_size).max(0.0);
    if track_span <= 0.0 || max_scroll <= 0.0 {
        0.0
    } else {
        let fraction = (thumb_offset_px / track_span).clamp(0.0, 1.0);
        fraction * max_scroll
    }
}

/// A visible scrollbar rail + thumb component reflecting viewport offset and content extent.
#[allow(dead_code)]
pub(crate) fn scrollbar_scene(
    geometry: &ScrollbarGeometry,
    orientation: ScrollbarOrientation,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let rail_fill = palette.content_bg;
    let thumb_fill = if geometry.visible {
        palette.dim_color
    } else {
        Color::NONE
    };
    let radius = palette.control_radius_px;

    match orientation {
        ScrollbarOrientation::Vertical => {
            let rail_len = geometry.rail_length_px;
            let offset = geometry.thumb_offset_px;
            let size = geometry.thumb_size_px;

            Box::new(bsn! {
                Node {
                    width: px(SCROLLBAR_THICKNESS_PX),
                    height: px(rail_len),
                    flex_direction: FlexDirection::Column,
                    border_radius: BorderRadius::all(Val::Px(radius)),
                    overflow: Overflow::clip(),
                }
                BackgroundColor(rail_fill)
                Children [
                    (
                        Node {
                            width: percent(100),
                            height: px(offset),
                        }
                    ),
                    (
                        Node {
                            width: percent(100),
                            height: px(size),
                            border_radius: BorderRadius::all(Val::Px(radius)),
                        }
                        BackgroundColor(thumb_fill)
                    ),
                ]
            })
        }
        ScrollbarOrientation::Horizontal => {
            let rail_len = geometry.rail_length_px;
            let offset = geometry.thumb_offset_px;
            let size = geometry.thumb_size_px;

            Box::new(bsn! {
                Node {
                    width: px(rail_len),
                    height: px(SCROLLBAR_THICKNESS_PX),
                    flex_direction: FlexDirection::Row,
                    border_radius: BorderRadius::all(Val::Px(radius)),
                    overflow: Overflow::clip(),
                }
                BackgroundColor(rail_fill)
                Children [
                    (
                        Node {
                            width: px(offset),
                            height: percent(100),
                        }
                    ),
                    (
                        Node {
                            width: px(size),
                            height: percent(100),
                            border_radius: BorderRadius::all(Val::Px(radius)),
                        }
                        BackgroundColor(thumb_fill)
                    ),
                ]
            })
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/widgets/controls.rs"]
mod tests;
