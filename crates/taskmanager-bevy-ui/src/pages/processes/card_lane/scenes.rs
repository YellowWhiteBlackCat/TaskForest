//! Scene construction for the Applications card lane. Every surface here is
//! another presentation of shell-owned row facts: status chips reduce through
//! the canonical filter, cards address the same visible rows and input seams
//! as the dense table, and the focus dashboard only displays typed facts.
//!
//! The static hero asks for the real optical tier; the scrolled cards stay
//! painted. Nested optical glass is not claimed.

use bevy::a11y::AccessibilityNode;
use bevy::ecs::hierarchy::Children;
use bevy::math::Vec2;
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{InlineBox, InlineBoxKind, LineBreak, TextLayout};
use bevy::ui::BorderColor;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, BoxShadow, FlexDirection, FlexWrap, JustifyContent,
    Node, Overflow, UiRect, Val, percent, px, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Button;
use taskmanager_application::i18n::{more_rows_label, t};
use taskmanager_shell::ProcessStatusFilter;
use taskmanager_theme::GlassTier;
use taskmanager_ui_contract::IconId;

use crate::app::PageContext;
use crate::glass::{GlassPanelMaterial, OpticalGlassCard, card_material};
use crate::icons::{IconInk, InlineIcon};
use crate::input_contract::{SemanticAddress, stable_semantic_address};
use crate::palette::{UiPalette, space_4, space_8};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole};

use super::runtime::{on_group_chip_activated, on_view_mode_activated};
use super::{
    CardGroupPlan, FocusProjection, ProcessAtomicCard, ProcessCardFlow, ProcessCardGroup,
    ProcessFocusContent, ProcessFocusDashboard, ProcessFocusField, ProcessFocusSortDir,
    ProcessFocusValue, ProcessGroupChip, ProcessViewMode, ProcessViewModeButton, focus_projection,
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
        ProcessFocusDashboard
        Children [ @{ grain } -- @{ content } ]
    })
}

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
                Node { flex_shrink: 0.0, overflow: Overflow::clip_x() }
                Children [
                    Text({ count }) ProcessFocusValue(ProcessFocusField::Count)
                    TextRole(Role::Body) Pickable::IGNORE
                ] --
                Node { flex_shrink: 0.0, overflow: Overflow::clip_x() }
                Children [
                    Text({ sort }) ProcessFocusValue(ProcessFocusField::Sort)
                    TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap }
                    Pickable::IGNORE
                ] --
                @{ sort_direction } --
                Node {
                    flex_grow: 1.0, flex_basis: px(0.0), min_width: px(0.0),
                    overflow: Overflow::clip_x(),
                }
                Children [
                    Text({ query }) ProcessFocusValue(ProcessFocusField::Query)
                    TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
                    Pickable::IGNORE
                ]
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

fn chip_scene(
    filter: ProcessStatusFilter,
    palette: &UiPalette,
    focus: &FocusProjection,
) -> Box<dyn Scene> {
    let selected = filter == focus.active_group;
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
        ProcessGroupChip({ filter })
        on(on_group_chip_activated)
        Children [
            Text({ filter.label().to_owned() }) TextRole(Role::Caption) Pickable::IGNORE
        ]
    })
}

/// The whole bounded lane: mode buttons, optical focus dashboard, status
/// chips, and one clipped card-flow viewport. Its fixed chrome follows the
/// shared card-flow budget's ladder.
pub(crate) fn card_lane_scene(context: &PageContext<'_>) -> impl Scene + use<> {
    let palette = context.palette;
    let focus = focus_projection(context.shell);
    let dashboard = focus_projection_dashboard_scene(palette, &focus);
    let cards_button = mode_button_scene(ProcessViewMode::Cards, palette);
    let dense_button = mode_button_scene(ProcessViewMode::Dense, palette);
    let chips: Vec<Box<dyn Scene>> = ProcessStatusFilter::ALL
        .into_iter()
        .map(|filter| chip_scene(filter, palette, &focus))
        .collect();
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
                height: rem(palette.control_height_rem),
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(space_8()),
                row_gap: Val::Px(space_8()),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
            }
            Children [ { chips } ] --
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

/// One complete state group: a header plus one row of atomic cards. The card
/// scenes interpolate directly into the row — a wrapper would keep their
/// markers from mounting.
pub(crate) fn group_scene(
    group: &CardGroupPlan,
    rows: &[ProcessRowView],
    hidden_columns: &[&str],
    query: &str,
    palette: &UiPalette,
    card_height_rem: f32,
) -> Box<dyn Scene> {
    // A truncated row must SAY so: the header states the bucket's whole
    // size, and the shared overflow wording names what the row leaves out.
    let hidden = group.total.saturating_sub(group.members.len());
    let header = if hidden == 0 {
        group.group.label().to_owned()
    } else {
        format!("{} · {}", group.group.label(), more_rows_label(hidden))
    };
    let cards: Vec<Box<dyn Scene>> = group
        .members
        .iter()
        .filter_map(|index| {
            rows.iter()
                .find(|row| row.index == *index)
                .map(|row| atomic_card_scene(row, hidden_columns, palette, query, card_height_rem))
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
                width: percent(100),
                height: rem(palette.control_height_rem),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                overflow: Overflow::clip_x(),
            }
            Children [
                Text({ header }) TextRole(Role::Caption) Pickable::IGNORE
            ] --
            Node {
                width: percent(100),
                height: Val::Rem(card_height_rem),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(space_8()),
                flex_shrink: 0.0,
            }
            Children [ { cards } ]
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

/// One atomic process card on the painted card tier. It carries the same row
/// link, semantic address, accessibility node and activation seams as a dense
/// row, so selection, secondary press and keyboard focus never fork.
pub(crate) fn atomic_card_scene(
    row: &ProcessRowView,
    hidden_columns: &[&str],
    palette: &UiPalette,
    query: &str,
    card_height_rem: f32,
) -> Box<dyn Scene> {
    let columns = crate::widgets::table::visible_columns(hidden_columns);
    let status = projection::cell_by_id(row, &columns, "Status");
    let pid = projection::cell_by_id(row, &columns, "PID");
    let cpu = projection::cell_by_id(row, &columns, "CPU");
    let memory = projection::cell_by_id(row, &columns, "Memory");
    let name = row.name.clone();
    let index = row.index;
    let selected = row.selected;
    let accessibility_node = crate::semantic::process_row_node(&row.name, &row.semantic_id);
    let semantic = SemanticAddress(stable_semantic_address("process-row", &row.semantic_id));
    let fill = palette.glass_fill(GlassTier::Card);
    let shadows = palette.glass_shadow(GlassTier::Card);
    let label = t_label;
    let _ = query;
    Box::new(bsn! {
        Node {
            flex_grow: 1.0,
            flex_basis: px(0.0),
            min_width: px(0.0),
            height: Val::Rem(card_height_rem),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
            overflow: Overflow::clip(),
        }
        BackgroundColor({ fill })
        BoxShadow({ shadows })
        ControlVisual(ControlTone::Card, { selected })
        Button
        ProcessRowLink({ index })
        ProcessAtomicCard({ index })
        ProcessTableArtifact
        on(input::on_row_activated)
        on(input::on_row_secondary_pressed)
        SemanticAddress({ semantic.0.clone() })
        AccessibilityNode({ accessibility_node })
        Children [
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
            Self::Cards => t("common.processes"),
            Self::Dense => t("settings.density_compact"),
        }
    }
}

/// Resolve shared copy before a scene template interpolates it.
fn t_label(key: &'static str) -> String {
    t(key).to_owned()
}
