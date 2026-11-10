//! Window rebuild: table/card presentation over one pinned shell row page.
//!
//! This module owns the renderer-side materialization only. Row identity,
//! sorting, filtering and the virtual window remain shell-owned; the card
//! lane's whole-group verdict remains `presentation::card_flow`-owned.

use super::{
    ProcessRowLink, ProcessRowView, ProcessRowsProjection, ProcessRowsRoot, ProcessTableArtifact,
    ProcessTableHeaderArtifact, TableSurface, bootstrap_processes_page, card_lane,
    count_line_text_for_shell, empty_state_text, input, on_query_commit, on_scroll_intent,
    on_select_row, on_select_step, on_toggle_row_selection, rows_projection_with_hidden,
    sort_projection, visible_columns,
};
use crate::input_contract::SemanticAddress;
use crate::pages::processes::projection;
use crate::palette::UiPalette;
use crate::scene_attach::attach_child;
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::widgets::table::resize::ColumnWidthOverrides;
use crate::widgets::table::{header_scene_with_widths, row_scene_with_widths};
use crate::window::{Role, TextRole};
use bevy::a11y::AccessibilityNode;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::system::Commands;
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::ui::prelude::{BackgroundColor, FlexDirection, Node, Val, percent, px, rem};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Button;
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;
use taskmanager_ui_contract::ProcessColumnSpec;

use super::card_lane::{ProcessViewMode, card_flow_plan, empty_card_scene, group_scene};

/// Selected-row fill: the palette's elevated card surface (the same token the
/// active nav item uses — one "selected" surface per theme).
/// The row's resting fill, in one with the shared [`ControlTone::Row`]
/// contract: a selected row wears the theme's selection tint, every other
/// odd row wears the zebra stripe, and an even row stays open so the page
/// canvas reads through the table.
fn row_fill(selected: bool, zebra: bool, palette: &UiPalette) -> bevy::color::Color {
    if selected {
        palette.selection_bg
    } else if zebra {
        palette.zebra_bg
    } else {
        bevy::color::Color::NONE
    }
}

/// One row wrapper: the virtual window's row height and selection fill, with
/// the shared widgets row scene inside (cell widths/alignment stay contract-
/// owned). The wrapper is a Button — pointer picking activates the row seam —
/// and carries the link index plus the AccessKit identity node so tests,
/// pointer bridges, and assistive technology map a node to its row.
fn row_wrapper_scene(
    row: &ProcessRowView,
    columns: &[&ProcessColumnSpec],
    widths: &[f32],
    query: &str,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let zebra = row.index % 2 == 1;
    let fill = row_fill(row.selected, zebra, palette);
    let index = row.index;
    let cells = row.cells.clone();
    let app_icon = row.app_icon;
    let accessibility_node = crate::semantic::process_row_node(&row.name, &row.semantic_id);
    let semantic = crate::input_contract::SemanticAddress(
        crate::input_contract::stable_semantic_address("process-row", &row.semantic_id),
    );
    let inner = row_scene_with_widths(&cells, columns, widths, query, app_icon, Some(palette));
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: rem(palette.control_height_rem),
        }
        BackgroundColor({ fill })
        ProcessRowLink({ index })
        ProcessTableArtifact
        ControlVisual(ControlTone::Row { zebra: { zebra } }, { row.selected })
        Button
        on(input::on_row_activated)
        on(input::on_row_secondary_pressed)
        SemanticAddress({ semantic.0.clone() })
        AccessibilityNode({ accessibility_node })
        Children [
             @{ inner }
        ]
    })
}

/// The empty window's honest message node.
fn empty_state_scene(message: String) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
        }
        ProcessTableArtifact
        Children [
             Text({ message }) TextRole(Role::Body)
        ]
    })
}

/// Materialize one window: the row wrappers, or the empty-state node when the
/// filtered set is empty. One rule for the mount-time static rows and every
/// observer rebuild.
pub(crate) fn window_scenes(
    projection: &ProcessRowsProjection,
    columns: &[&ProcessColumnSpec],
    widths: &[f32],
    palette: &UiPalette,
    query: &str,
) -> Vec<Box<dyn Scene>> {
    if projection.total == 0 {
        return vec![empty_state_scene(empty_state_text(query))];
    }
    projection
        .rows
        .iter()
        .map(|row| row_wrapper_scene(row, columns, widths, query, palette))
        .collect()
}

/// Despawn the previous window's artifacts, spawn the new one under the rows
/// root, rewrite the count line, and normalize the stored scroll intent to
/// the clamped window start. Shared by every observer.
///
/// The header is NOT rebuilt while a column drag is live: the drag's gutter
/// lives in the header, and rebuilding it under the pointer would cancel the
/// drag mid-gesture. The dragged widths are written into the live cells
/// instead, and the header picks them up when the drag ends.
pub(crate) fn rebuild_table(
    commands: &mut Commands,
    root: Entity,
    shell: &ShellApp,
    surface: &mut TableSurface,
) {
    let owned_hidden_ids: Vec<String> = surface
        .hidden_cols
        .as_ref()
        .map_or_else(Vec::new, |hidden| hidden.0.iter().cloned().collect());
    let hidden_ids: Vec<&str> = owned_hidden_ids.iter().map(String::as_str).collect();
    let projection = rows_projection_with_hidden(
        shell,
        surface.scroll.viewport_rows,
        surface.scroll.top,
        &hidden_ids,
    );
    surface.scroll.top = projection.window.first();
    let columns = visible_columns(&hidden_ids);
    let widths = live_widths(&columns, surface.widths.as_deref());
    for artifact in surface.artifacts.iter() {
        commands.entity(artifact).despawn();
    }
    rebuild_window_scenes(
        commands,
        surface,
        WindowRenderInput {
            root,
            shell,
            projection: &projection,
            columns: &columns,
            widths: &widths,
            hidden_ids: &hidden_ids,
        },
    );
    if let Ok(mut line) = surface.count.single_mut() {
        line.0 = count_line_text_for_shell(shell, projection.total, &shell.query);
    }

    let dragging = surface
        .resize
        .as_ref()
        .is_some_and(|session| session.column.is_some());
    if dragging {
        return;
    }
    for header in surface.headers.iter() {
        commands.entity(header).despawn();
    }
    if let Ok(slot) = surface.header_slots.single() {
        let sort_proj = Some(sort_projection(shell.process_sort));
        let header = header_scene_with_widths(&columns, &widths, sort_proj, &surface.palette.inner);
        let header_node = bsn! {
            Node {
                width: percent(100),
            }
            ProcessTableHeaderArtifact
            Children [
                 @{ header }
            ]
        };
        let child = commands.spawn_scene(header_node).id();
        attach_child(commands, slot, child);
    }
}

/// The immutable window facts one rebuild needs. Bundling them keeps the
/// renderer under Clippy's argument ceiling without a lint exemption.
struct WindowRenderInput<'a> {
    root: Entity,
    shell: &'a ShellApp,
    projection: &'a ProcessRowsProjection,
    columns: &'a [&'a ProcessColumnSpec],
    widths: &'a [f32],
    hidden_ids: &'a [&'a str],
}

/// Materialize one virtual window into the active presentation. Dense mode
/// uses the existing table cells; card mode admits complete groups from the
/// measured card-flow budget. Both paths use the same pinned `RowPage`.
fn rebuild_window_scenes(
    commands: &mut Commands,
    surface: &mut TableSurface,
    input: WindowRenderInput<'_>,
) {
    let WindowRenderInput {
        root,
        shell,
        projection,
        columns,
        widths,
        hidden_ids,
    } = input;
    let card_budget = surface.card_budget.as_deref().copied().unwrap_or_default();
    let mode = card_budget.effective_mode(
        surface
            .mode
            .as_deref()
            .copied()
            .unwrap_or(ProcessViewMode::Dense),
    );
    let scenes: Vec<Box<dyn Scene>> = match mode {
        ProcessViewMode::Dense => window_scenes(
            projection,
            columns,
            widths,
            &surface.palette.inner,
            &shell.query,
        ),
        ProcessViewMode::Cards => {
            let metrics = card_lane::CardFlowMetrics::from_palette(&surface.palette.inner);
            let card_height_rem = surface
                .palette
                .inner
                .rem_ratio(card_budget.card_height_px(metrics));
            // The card lane reads the shell's VISIBLE rows. The dense
            // table's measured window collapses to zero rows while the
            // card lane owns the screen, so a window-derived plan would
            // admit nothing at all.
            let window = surface.card_window.as_deref().copied().unwrap_or_default();
            let plan = card_flow_plan(shell, card_budget, window);
            if plan.empty || plan.groups.is_empty() {
                vec![empty_card_scene(
                    if plan.empty {
                        empty_state_text(&shell.query)
                    } else {
                        t("common.telemetry_waiting").to_owned()
                    },
                    &surface.palette.inner,
                )]
            } else {
                plan.groups
                    .iter()
                    .map(|group| {
                        let rows =
                            projection::row_views_for_indices(shell, &group.members, hidden_ids);
                        group_scene(
                            group,
                            &rows,
                            hidden_ids,
                            &shell.query,
                            &surface.palette.inner,
                            card_height_rem,
                        )
                    })
                    .collect()
            }
        }
    };
    // The cards live in the card lane's own bounded flow root; the dense rows
    // live in the table's rows root. One window, two presentations, one
    // pinned row page.
    let target = match mode {
        ProcessViewMode::Dense => Some(root),
        ProcessViewMode::Cards => surface.card_flows.iter().next(),
    };
    let Some(target) = target else {
        return;
    };
    for scene in scenes {
        let child = commands.spawn_scene(scene).id();
        attach_child(commands, target, child);
    }
}

/// The live widths of the visible columns: the user's drag overrides over the
/// contract defaults.
pub(crate) fn live_widths(
    columns: &[&ProcessColumnSpec],
    overrides: Option<&ColumnWidthOverrides>,
) -> Vec<f32> {
    match overrides {
        Some(overrides) => crate::widgets::table::resize::live_column_widths(columns, overrides),
        None => crate::widgets::table::distribute_column_widths(columns, None),
    }
}

/// The rows root scene: measured virtual viewport, the five `on()` seam
/// observers (bootstrap first, so it is registered before the marker's own
/// `Add` trigger dispatches), and the mount-time static window inside.
pub(crate) fn rows_root_scene(
    projection: &ProcessRowsProjection,
    columns: &[&ProcessColumnSpec],
    widths: &[f32],
    palette: &UiPalette,
    query: &str,
) -> impl Scene + use<> {
    let rows = window_scenes(projection, columns, widths, palette, query);
    bsn! {
        Node {
            width: percent(100),
            min_height: px(0.0), flex_grow: 1.0, flex_basis: px(0.0),
            flex_direction: FlexDirection::Column,
        }
        on(bootstrap_processes_page)
        on(on_select_step)
        on(on_select_row)
        on(on_toggle_row_selection)
        on(on_scroll_intent)
        on(on_query_commit)
        ProcessRowsRoot
        Children [
            { rows }
        ]
    }
}
