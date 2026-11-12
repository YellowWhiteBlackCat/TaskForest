//! Scene construction for the Applications card lane. Every surface here is
//! another presentation of shell-owned row facts: the cards address the same
//! visible rows and input seams as the dense table, and the focus dashboard
//! only displays typed facts.
//!
//! The focus dashboard and the atomic cards ask for the REAL optical tier.
//! Their painted appearance still stands in when the renderer cannot sample
//! a backdrop, and every surface also joins the offscreen lower composite
//! so the optical shader refracts the product's real surfaces. Nested
//! optical glass is not claimed: the mirrored layer stays painted.

use bevy::a11y::AccessibilityNode;
use bevy::color::Color;
use bevy::ecs::hierarchy::Children;
use bevy::math::Vec2;
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{InlineBox, InlineBoxKind, LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderColor, BorderRadius, BoxShadow, FlexDirection,
    JustifyContent, Node, Overflow, PositionType, UiRect, Val, percent, px, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Button;
use taskmanager_application::i18n::{key_value_label, more_rows_label, t};
use taskmanager_shell::ProcessStatusFilter;
use taskmanager_theme::GlassTier;
use taskmanager_ui_contract::IconId;
use taskmanager_ui_contract::presentation::card_flow::hidden_group_members;
use taskmanager_ui_contract::presentation::card_width::card_row_columns;

use crate::app::PageContext;
use crate::glass::{BackdropMirror, GlassPanelMaterial, OpticalGlassCard, card_material};
use crate::icons::{IconInk, InlineIcon};
use crate::input_contract::{SemanticAddress, stable_semantic_address};
use crate::menu_modal::MenuModalTrigger;
use crate::palette::{UiPalette, space_4, space_8};
use crate::tooltip::TooltipText;
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole};

use super::runtime::{on_group_overflow_activated, on_view_mode_activated};
use super::{
    CardGroupPlan, CardRowWidthMeasurements, FocusProjection, ProcessAtomicCard, ProcessCardFlow,
    ProcessCardGroup, ProcessCardLaneViewport, ProcessCardStateOverlay, ProcessFocusContent,
    ProcessFocusDashboard, ProcessFocusField, ProcessFocusSortDir, ProcessFocusValue,
    ProcessGroupOverflow, ProcessViewMode, ProcessViewModeButton, focus_projection,
};
use crate::pages::processes::projection::{self, ProcessRowView};
use crate::pages::processes::{ProcessRowLink, ProcessTableArtifact, input};

/// The static focus dashboard on the real optical tier. Its content is a
/// replaceable child scene, so runtime fact updates never rebuild the optical
/// marker or its fallback binding.
pub(crate) fn focus_projection_dashboard_scene(
    palette: &UiPalette,
    focus: &FocusProjection,
) -> Box<dyn Scene> {
    let GlassPanelMaterial {
        fill,
        edge,
        shadows,
        edge_width_rem,
        radius_rem,
        grain,
    } = card_material(palette);
    let content = focus_content_scene(palette, focus);
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_8())),
            border: UiRect::all(Val::Rem(edge_width_rem)),
            border_radius: BorderRadius::all(Val::Rem(radius_rem)),
        }
        BackgroundColor({ fill })
        BorderColor::all(edge)
        BoxShadow({ shadows })
        OpticalGlassCard { tier: GlassTier::Card, enabled: true }
        BackdropMirror { tier: GlassTier::Card }
        ProcessFocusDashboard
        MenuModalTrigger
        Children [ @{ grain } -- @{ content } ]
    })
}

/// The dashboard's three fact rows.
///
/// Every fact carries its label: an unlabelled value reads as a stray number
/// rather than as a measurement, and the hero is the only place the card
/// presentation states the count, the sort and the query.
fn focus_content_scene(palette: &UiPalette, focus: &FocusProjection) -> Box<dyn Scene> {
    let sort_icon = match focus.sort_descending {
        true => IconId::NavigateDown,
        false => IconId::NavigateUp,
    };
    let sort_direction = bsn! {
        InlineBox { kind: InlineBoxKind::InFlow, size: Vec2::splat(palette.icon_size_px()) }
        InlineIcon({ sort_icon })
        IconInk({ palette.dim_color })
        ProcessFocusSortDir({ focus.sort_descending })
        Pickable::IGNORE
    };
    let name = focus.value(ProcessFocusField::Name);
    let status = focus.value(ProcessFocusField::Status);
    let count = focus.value(ProcessFocusField::Count);
    let sort = focus.value(ProcessFocusField::Sort);
    let query = focus.value(ProcessFocusField::Query);
    let label = t_label;
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
        }
        ProcessFocusContent
        Children [
            Node {
                width: percent(100),
                height: rem(palette.control_height_rem),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
                flex_shrink: 0.0,
                overflow: Overflow::clip_x(),
            }
            Children [
                Node {
                    flex_grow: 1.0, flex_basis: px(0.0), min_width: px(0.0),
                    overflow: Overflow::clip_x(),
                }
                Children [
                    Text({ name }) ProcessFocusValue(ProcessFocusField::Name)
                    TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap }
                    Pickable::IGNORE
                ] --
                Node { flex_shrink: 0.0, overflow: Overflow::clip_x() }
                Children [
                    Text({ status }) ProcessFocusValue(ProcessFocusField::Status)
                    TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
                    Pickable::IGNORE
                ]
            ] --
            Node {
                width: percent(100),
                height: rem(palette.control_height_rem),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
                flex_shrink: 0.0,
                overflow: Overflow::clip_x(),
            }
            Children [
                @{ labeled_value(&label("proc.pid"), focus.value(ProcessFocusField::Pid), ProcessFocusField::Pid) } --
                @{ labeled_value(&label("common.cpu"), focus.value(ProcessFocusField::Cpu), ProcessFocusField::Cpu) } --
                @{ labeled_value(&label("common.memory"), focus.value(ProcessFocusField::Memory), ProcessFocusField::Memory) }
            ] --
            Node {
                width: percent(100),
                height: rem(palette.control_height_rem),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
                flex_shrink: 0.0,
                overflow: Overflow::clip_x(),
            }
            Children [
                @{ labeled_value(&label("proc.focus_visible"), count, ProcessFocusField::Count) } --
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_4()),
                    min_width: px(0.0),
                    flex_shrink: 1.0,
                    overflow: Overflow::clip_x(),
                }
                Children [
                    Text({ label("proc.focus_sort") }) TextRole(Role::Caption) Pickable::IGNORE --
                    Text({ sort }) ProcessFocusValue(ProcessFocusField::Sort) TextRole(Role::Body)
                    TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE
                ] --
                @{ sort_direction } --
                @{ labeled_value(&label("proc.focus_query"), query, ProcessFocusField::Query) }
            ]
        ]
    })
}

/// One labeled fact with a typed, runtime-refreshable value leaf.
fn labeled_value(label: &str, value: String, field: ProcessFocusField) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
            min_width: px(0.0),
            flex_shrink: 1.0,
            overflow: Overflow::clip_x(),
        }
        Children [
            Text({ label.to_owned() }) TextRole(Role::Caption) Pickable::IGNORE --
            Text({ value }) ProcessFocusValue({ field }) TextRole(Role::Body)
            TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE
        ]
    })
}

fn mode_button_scene(mode: ProcessViewMode, palette: &UiPalette) -> Box<dyn Scene> {
    let selected = mode == ProcessViewMode::Cards;
    Box::new(bsn! {
        Node {
            height: rem(palette.control_height_rem),
            padding: UiRect::horizontal(px(space_8())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_shrink: 0.0,
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ if selected { palette.nav_active_bg } else { palette.content_bg } })
        ControlVisual(ControlTone::Surface, { selected })
        Button
        ProcessViewModeButton({ mode })
        on(on_view_mode_activated)
        Children [
            Text({ mode.label().to_owned() }) TextRole(Role::Caption) Pickable::IGNORE
        ]
    })
}

/// The whole bounded lane: the mode row, the optical focus dashboard and one
/// clipped card-flow viewport.
///
/// The lane renders NO filter chips: the page chrome's canonical status
/// filter already classifies by the very same provider state the buckets
/// use, so a second chip row would paint the same fact twice.
pub(crate) fn card_lane_scene(context: &PageContext<'_>) -> impl Scene + use<> {
    let palette = context.palette;
    let focus = focus_projection(context.shell);
    let dashboard = focus_projection_dashboard_scene(palette, &focus);
    let cards_button = mode_button_scene(ProcessViewMode::Cards, palette);
    let dense_button = mode_button_scene(ProcessViewMode::Dense, palette);
    bsn! {
        Node {
            width: percent(100),
            flex_grow: 1.0,
            flex_basis: px(0.0),
            min_height: px(0.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            overflow: Overflow::clip(),
        }
        ProcessCardLaneViewport
        Children [
            Node {
                width: percent(100),
                height: rem(palette.control_height_rem),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(space_8()),
                flex_shrink: 0.0,
            }
            Children [ @{ cards_button } -- @{ dense_button } ] --
            @{ dashboard } --
            Node {
                width: percent(100),
                flex_grow: 1.0,
                flex_basis: px(0.0),
                min_height: px(0.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(space_8()),
                overflow: Overflow::clip(),
            }
            ProcessCardFlow
        ]
    }
}

/// One complete state group: a header that states the bucket, one row of
/// atomic cards, and the action that reveals the rows the row could not
/// hold. The card scenes interpolate directly into the row — a wrapper would
/// keep their markers from mounting.
pub(crate) fn group_scene(
    group: &CardGroupPlan,
    rows: &[ProcessRowView],
    hidden_columns: &[&str],
    query: &str,
    palette: &UiPalette,
    card_height_px: f32,
) -> Box<dyn Scene> {
    // A truncated row must SAY so: the header states the bucket's whole
    // size, and the overflow action names what the row leaves out. When the
    // focus dashboard represents the bucket's first row, its card is not
    // painted a second time and does not count toward the hidden remainder.
    let hidden = hidden_group_members(group.total, group.members.len(), group.focus_is_first);
    let header = key_value_label(group.group.label(), &group.total.to_string());
    let overflow = overflow_action_scene(group.group, hidden, palette);
    let row_width = if group.row_width.content_width_rem > 0.0 {
        group.row_width
    } else {
        let metrics = super::CardFlowMetrics::from_palette(palette);
        let spec = metrics.width_spec();
        let columns = card_row_columns(spec.content_width_cap, spec);
        metrics.width_rem(spec.row_width_bounded(columns, spec.content_width_cap))
    };
    let cards: Vec<Box<dyn Scene>> = group
        .members
        .iter()
        .filter_map(|index| {
            rows.iter().find(|row| row.index == *index).map(|row| {
                atomic_card_scene(
                    row,
                    hidden_columns,
                    palette,
                    query,
                    card_height_px,
                    row_width,
                    false,
                )
            })
        })
        .collect();
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            flex_shrink: 0.0,
        }
        ProcessCardGroup({ group.group })
        ProcessTableArtifact
        Children [
            Node {
                width: rem(row_width.content_width_rem),
                max_width: percent(100),
                height: rem(palette.control_height_rem),
                align_items: AlignItems::Center,
                column_gap: rem(row_width.gap_rem),
                flex_shrink: 1.0,
                min_width: px(0.0),
                overflow: Overflow::clip_x(),
            }
            Children [
                Text({ header }) TextRole(Role::Caption) Pickable::IGNORE --
                @{ overflow }
            ] --
            Node {
                width: rem(row_width.content_width_rem),
                max_width: percent(100),
                height: px(card_height_px),
                flex_direction: FlexDirection::Row,
                column_gap: rem(row_width.gap_rem),
                flex_shrink: 1.0,
                min_width: px(0.0),
                justify_content: JustifyContent::Start,
            }
            Children [ { cards } ]
        ]
    })
}

/// The overflow action for a bucket the admitted row could not hold.
///
/// The action reduces through the shell's canonical status filter and hands
/// the page to its dense table, so every row of the bucket stays reachable
/// instead of being summarized away.
fn overflow_action_scene(
    filter: ProcessStatusFilter,
    hidden: usize,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    if hidden == 0 {
        return Box::new(bsn! {
            Node { width: px(0.0), height: px(0.0) }
            Pickable::IGNORE
        });
    }
    let mut node = accesskit::Node::new(accesskit::Role::Button);
    node.set_label(t("proc.group_show_all").replace("{status}", filter.label()));
    node.set_description(more_rows_label(hidden));
    Box::new(bsn! {
        Node {
            height: rem(palette.control_height_rem),
            padding: UiRect::horizontal(px(space_8())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_shrink: 0.0,
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.content_bg })
        ControlVisual(ControlTone::Surface, { false })
        Button
        ProcessGroupOverflow({ filter })
        on(on_group_overflow_activated)
        AccessibilityNode({ node })
        Children [
            Text({ more_rows_label(hidden) }) TextRole(Role::Caption) Pickable::IGNORE
        ]
    })
}

/// The bounded flow's honest empty state; the message already comes from the
/// shared empty-state seam.
pub(crate) fn empty_card_scene(message: String, palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            min_height: rem(palette.control_height_rem),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.zebra_bg })
        ProcessTableArtifact
        Children [
            Text({ message }) TextRole(Role::Caption)
            TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE
        ]
    })
}

/// The card's state tint overlay and its readable state edge.
///
/// An optical card has no painted fill to tint, so the hover, pressed and
/// selected ink rides this overlay above the glass instead of replacing the
/// surface the material owns. The overlay also owns a state border above the
/// glass: a translucent tint alone is too quiet on a light glass panel, while
/// the border keeps the active card legible without changing its size.
fn state_overlay_scene(palette: &UiPalette) -> Box<dyn Scene> {
    let edge_width = GlassTier::Card.edge_width_rem();
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: percent(0),
            top: percent(0),
            border: UiRect::all(Val::Rem(edge_width)),
            border_radius: BorderRadius::all(Val::Rem(palette.panel_radius_rem())),
        }
        BackgroundColor(Color::NONE)
        BorderColor::DEFAULT
        ProcessCardStateOverlay
        Pickable::IGNORE
    })
}

/// One atomic process card on the real optical tier. It carries the same row
/// link, semantic address, accessibility node and activation seams as a dense
/// row, so selection, secondary press and keyboard focus never fork. A hover
/// hint above this optical card is the production two-layer glass case: the
/// hint samples the chain target that carries this card's real optical pixels.
pub(crate) fn atomic_card_scene(
    row: &ProcessRowView,
    hidden_columns: &[&str],
    palette: &UiPalette,
    query: &str,
    card_height_px: f32,
    row_width: CardRowWidthMeasurements,
    compact_focus: bool,
) -> Box<dyn Scene> {
    let columns = crate::widgets::table::visible_columns(hidden_columns);
    let status = projection::cell_by_id(row, &columns, "Status");
    let pid = projection::cell_by_id(row, &columns, "PID");
    let cpu = projection::cell_by_id(row, &columns, "CPU");
    let memory = projection::cell_by_id(row, &columns, "Memory");
    let name = row.name.clone();
    let tooltip = card_tooltip_text(row);
    let index = row.index;
    let selected = row.selected;
    let accessibility_node = crate::semantic::process_row_node(&row.name, &row.semantic_id);
    let semantic = SemanticAddress(stable_semantic_address("process-row", &row.semantic_id));
    let GlassPanelMaterial {
        fill,
        edge,
        shadows,
        radius_rem,
        ..
    } = card_material(palette);
    let label = t_label;
    let _ = query;
    let overlay = state_overlay_scene(palette);
    let vertical_padding = if compact_focus { space_4() } else { space_8() };
    let card_basis = if compact_focus {
        px(card_height_px)
    } else {
        rem(row_width.card_width_rem)
    };
    Box::new(bsn! {
        Node {
            width: rem(row_width.card_width_rem),
            max_width: rem(row_width.card_width_rem),
            flex_grow: 0.0,
            flex_shrink: 0.0,
            flex_basis: card_basis,
            height: px(card_height_px),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::axes(Val::Px(space_8()), Val::Px(vertical_padding)),
            border: UiRect::all(Val::Rem(GlassTier::Card.edge_width_rem())),
            border_radius: BorderRadius::all(Val::Rem(radius_rem)),
            overflow: Overflow::clip(),
        }
        BackgroundColor({ fill })
        BorderColor::all(edge)
        BoxShadow({ shadows })
        OpticalGlassCard { tier: GlassTier::Card, enabled: true }
        BackdropMirror { tier: GlassTier::Card }
        ControlVisual(ControlTone::Card, { selected })
        Button
        ProcessRowLink({ index })
        ProcessAtomicCard({ index })
        ProcessTableArtifact
        MenuModalTrigger
        TooltipText({ tooltip })
        on(input::on_row_activated)
        on(input::on_row_secondary_pressed)
        SemanticAddress({ semantic.0.clone() })
        AccessibilityNode({ accessibility_node })
        Children [
            @{ overlay } --
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
                flex_shrink: 0.0,
                overflow: Overflow::clip_x(),
            }
            Children [
                Node {
                    flex_grow: 1.0, flex_basis: px(0.0), min_width: px(0.0),
                    overflow: Overflow::clip_x(),
                }
                Children [
                    Text({ name }) TextRole(Role::Body)
                    TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE
                ] --
                Node { flex_shrink: 0.0, overflow: Overflow::clip_x() }
                Children [
                    Text({ status }) TextRole(Role::Caption)
                    TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE
                ]
            ] --
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
                flex_shrink: 0.0,
                overflow: Overflow::clip_x(),
            }
            Children [
                @{ card_fact(&label("proc.pid"), pid) } --
                @{ card_fact(&label("common.cpu"), cpu) } --
                @{ card_fact(&label("common.memory"), memory) }
            ]
        ]
    })
}

/// Resolve the card's hover hint from facts the shell row already owns.
///
/// A longer command line is the same extended-detail tooltip the GPUI process
/// row paints. A card whose name already carries the whole command uses its
/// labeled name/status/PID facts instead, so no tooltip is empty or invented.
#[must_use]
fn card_tooltip_text(row: &ProcessRowView) -> String {
    let details_label = t_label("common.details");
    if row.cmdline.len() > row.name.len() {
        return format!(
            "{} · {}",
            key_value_label(&details_label, &row.name),
            row.cmdline
        );
    }
    key_value_label(&details_label, &row.name)
}

fn card_fact(label: &str, value: String) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
            min_width: px(0.0),
            flex_shrink: 1.0,
            overflow: Overflow::clip_x(),
        }
        Children [
            Text({ label.to_owned() }) TextRole(Role::Caption) Pickable::IGNORE --
            Text({ value }) TextRole(Role::Body)
            TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE
        ]
    })
}

impl ProcessViewMode {
    /// The mode button's shared copy. No frontend-local dictionary.
    fn label(self) -> &'static str {
        match self {
            Self::Cards => t("proc.view_cards"),
            Self::Dense => t("proc.view_table"),
        }
    }
}

/// Resolve shared copy before a scene template interpolates it.
fn t_label(key: &'static str) -> String {
    t(key).to_owned()
}
