//! Process/info table: pure projection core + minimal bsn! render adapter.
//!
//! **Pure core** (no bevy types): the column vocabulary comes verbatim from
//! the ui-contract single source ([`PROCESS_COLUMNS`]) so the bevy table can
//! never drift from the shared frontend column semantics; the sort projection
//! input is the contract's stable column token; and the virtual-scroll
//! window is a plain clamping function over (total, viewport, scroll).
//!
//! **Render adapter**: one header scene and one row scene. Row *material*
//! (which rows exist, their cell text) stays owned by the page + shell —
//! this layer only renders what it is handed, bounded by the window math.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::system::{Commands, NonSendMut, Query};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on, template_value};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_shell::SortCol;
use taskmanager_ui_contract::{PROCESS_COLUMNS, ProcessColumnSpec};

use crate::app::FrontendTrack;
use crate::palette::{UiPalette, no_wrap_text, space_4, space_8};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole};

/// Active sort as a table-projection input: the ui-contract column token
/// plus direction. Pages translate their shell sort slot (`SortCol`,
/// `InfoSortCol`, …) into this neutral shape at the boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SortProjection {
    /// Stable column token (`ProcessColumnSpec::id`).
    pub column: &'static str,
    /// `true` when the sort is reversed (descending).
    pub descending: bool,
}

/// Hideable-column projection: drop hidden columns, keep contract order, and
/// keep the identity column (`Name`, `hideable == false`) always visible — a
/// caller cannot accidentally render an anonymous table.
pub(crate) fn visible_columns(hidden: &[&str]) -> Vec<&'static ProcessColumnSpec> {
    PROCESS_COLUMNS
        .iter()
        .filter(|spec| spec.hideable && !hidden.contains(&spec.id) || !spec.hideable)
        .collect()
}

/// Minimum width in pixels for any rendered column to remain legible.
pub(crate) const MIN_COLUMN_WIDTH_PX: f32 = 40.0;
/// Maximum width in pixels for any column when resizing or distributing.
pub(crate) const MAX_COLUMN_WIDTH_PX: f32 = 1200.0;

/// Clamp a column width to the supported range.
#[must_use]
pub(crate) fn clamp_column_width(width: f32) -> f32 {
    width.clamp(MIN_COLUMN_WIDTH_PX, MAX_COLUMN_WIDTH_PX)
}

/// Distribute available horizontal width across table columns.
///
/// If `available_width` is `None` or non-positive, returns contract `default_width` for each column.
/// When `available_width` is `Some(w)`:
/// - If `w` exceeds total default width:
///   Fixed/numeric columns keep their legible default widths; the primary identity column
///   (`Name`) flexes to absorb the surplus space.
/// - If `w` is less than total default width:
///   Columns are scaled proportionally down to `MIN_COLUMN_WIDTH_PX`.
#[must_use]
pub(crate) fn distribute_column_widths(
    columns: &[&ProcessColumnSpec],
    available_width: Option<f32>,
) -> Vec<f32> {
    distribute_column_widths_with_overrides(columns, None, available_width)
}

/// Distribute column widths while respecting optional per-column width overrides.
#[must_use]
pub(crate) fn distribute_column_widths_with_overrides(
    columns: &[&ProcessColumnSpec],
    overrides: Option<&std::collections::HashMap<&str, f32>>,
    available_width: Option<f32>,
) -> Vec<f32> {
    if columns.is_empty() {
        return Vec::new();
    }
    let base_widths: Vec<f32> = columns
        .iter()
        .map(|spec| {
            if let Some(ovr) = overrides.and_then(|map| map.get(spec.id).copied()) {
                clamp_column_width(ovr)
            } else {
                clamp_column_width(spec.default_width)
            }
        })
        .collect();

    let Some(avail) = available_width else {
        return base_widths;
    };
    if avail <= 0.0 {
        return base_widths;
    }

    let total_base: f32 = base_widths.iter().sum();
    let total_min = MIN_COLUMN_WIDTH_PX * (columns.len() as f32);

    if avail >= total_base {
        let surplus = avail - total_base;
        let name_idx = columns.iter().position(|spec| spec.id == "Name");
        let mut result = base_widths;
        if let Some(idx) = name_idx {
            result[idx] = clamp_column_width(result[idx] + surplus);
        } else {
            let add_per_col = surplus / (columns.len() as f32);
            for w in &mut result {
                *w = clamp_column_width(*w + add_per_col);
            }
        }
        result
    } else if avail <= total_min {
        vec![MIN_COLUMN_WIDTH_PX; columns.len()]
    } else {
        let factor = (avail - total_min) / (total_base - total_min).max(1e-6);
        base_widths
            .iter()
            .map(|&base| {
                let shrunk = MIN_COLUMN_WIDTH_PX + (base - MIN_COLUMN_WIDTH_PX) * factor;
                clamp_column_width(shrunk)
            })
            .collect()
    }
}

/// How many rows of `row_height_px` fit in `viewport_height_px`. A
/// non-positive row height renders nothing (never a divide-by-zero panic,
/// never a fabricated giant viewport). The `as` cast is a saturating f32→usize
/// cast on an already-floored positive value.
pub(crate) fn rows_in_viewport(viewport_height_px: f32, row_height_px: f32) -> usize {
    if row_height_px <= 0.0 || viewport_height_px <= 0.0 {
        return 0;
    }
    (viewport_height_px / row_height_px).floor() as usize
}

/// Half-open visible row range `[first, last)` over a row space of `total`
/// rows with `viewport_rows` capacity at scroll offset `scroll_top` (the
/// index of the first row the caller asked to show).
///
/// Clamping contract: an empty table or zero-capacity viewport yields an
/// empty window; a scroll offset past the end pins to the last full page
/// (never an empty tail window, never an out-of-bounds range).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RowWindow {
    pub(crate) first: usize,
    pub(crate) last: usize,
}

/// Compute the visible [`RowWindow`]. Pure; the M1 virtual scroller feeds it
/// `total` from the shell projection and `scroll_top` from its scroll state.
pub(crate) fn row_window(total: usize, viewport_rows: usize, scroll_top: usize) -> RowWindow {
    if total == 0 || viewport_rows == 0 {
        return RowWindow::default();
    }
    let visible = viewport_rows.min(total);
    let max_top = total - visible;
    let first = scroll_top.min(max_top);
    RowWindow {
        first,
        last: first + visible,
    }
}

/// One header cell's pure label: the column's own word. Sort direction is a
/// separate render decision ([`sorted_direction`]) so identity and
/// decoration never entangle in one string.
pub(crate) fn header_label(column: &ProcessColumnSpec) -> String {
    column.id.to_owned()
}

/// The active sort's direction *when it rests on this column*: `Some(true)`
/// is descending, `Some(false)` ascending, `None` unsorted. Pure; the header
/// scene renders it as a semantic direction plate, never a text glyph.
pub(crate) fn sorted_direction(
    column: &ProcessColumnSpec,
    sort: Option<SortProjection>,
) -> Option<bool> {
    match sort {
        Some(active) if active.column == column.id => Some(active.descending),
        _ => None,
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessSortHeader(pub(crate) &'static str);

pub(crate) fn sort_col_from_id(id: &str) -> Option<SortCol> {
    match id {
        "Name" => Some(SortCol::Name),
        "User" => Some(SortCol::User),
        "PID" => Some(SortCol::Pid),
        "Threads" => Some(SortCol::Threads),
        "StartTime" => Some(SortCol::StartTime),
        "Status" => Some(SortCol::State),
        "CPU" => Some(SortCol::Cpu),
        "Memory" => Some(SortCol::Memory),
        "Swap" => Some(SortCol::Swap),
        "MemoryPss" => Some(SortCol::Pss),
        "DiskRead" => Some(SortCol::DiskRead),
        "DiskWrite" => Some(SortCol::DiskWrite),
        "Network" => Some(SortCol::Network),
        "CPUTime" => Some(SortCol::CpuTime),
        "FDs" => Some(SortCol::Fds),
        "Nice" => Some(SortCol::Nice),
        _ => None,
    }
}

pub(crate) fn on_process_sort_header_activated(
    activate: On<Activate>,
    headers: Query<&ProcessSortHeader>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    let header = headers
        .get(activate.entity)
        .or_else(|_| headers.get(activate.event().entity));
    let Ok(header) = header else {
        return;
    };
    if let Some(col) = sort_col_from_id(header.0) {
        if track.shell.process_sort.0 == col {
            track.shell.toggle_sort_direction();
        } else {
            track.shell.set_sort_column(col);
        }
        commands.trigger(crate::input::ShellInteractionApplied);
    }
}

/// Header row with explicit column widths.
pub(crate) fn header_scene_with_widths(
    columns: &[&ProcessColumnSpec],
    widths: &[f32],
    sort: Option<SortProjection>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let directions: Vec<Option<bool>> = columns
        .iter()
        .map(|column| sorted_direction(column, sort))
        .collect();
    let cells = header_cells(columns, widths, &directions, palette);
    bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_8()),
        }
        Children [
            { cells }
        ]
    }
}

/// Render adapter: the header row. One text cell per column, widths from the
/// contract's default-width tokens; numeric columns right-align; the sorted
/// column carries the semantic direction icon.
pub(crate) fn header_scene(
    columns: &[&ProcessColumnSpec],
    sort: Option<SortProjection>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let widths = distribute_column_widths(columns, None);
    header_scene_with_widths(columns, &widths, sort, palette)
}

/// A data cell: contract width, right-aligned when numeric, and strictly
/// single-line — `NoWrap` + x-clip means a value wider than its column is
/// clipped at the column edge, never wrapped into a second line and never
/// allowed to stretch the row or shift sibling cells. Horizontal alignment
/// is the MAIN axis (`justify_content`); the text is vertically centered
/// (`align_items` is the cross axis), so numeric and text cells share one
/// baseline inside the fixed-height row.
fn cell_scene(cell: String, width: f32, numeric_column: bool, label: bool) -> impl Scene + use<> {
    let align = if numeric_column {
        JustifyContent::FlexEnd
    } else {
        JustifyContent::FlexStart
    };
    let no_wrap = TextLayout {
        linebreak: LineBreak::NoWrap,
        ..TextLayout::default()
    };
    let role = if label { Role::Caption } else { Role::Body };
    bsn! {
        Node {
            width: px(width),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            justify_content: align,
            align_items: AlignItems::Center,
            overflow: Overflow::clip_x(),
        }
        Children [
            ( Text(cell) TextRole({ role }) template_value(no_wrap) ),
        ]
    }
}

fn header_cells(
    columns: &[&ProcessColumnSpec],
    widths: &[f32],
    directions: &[Option<bool>],
    palette: &UiPalette,
) -> Vec<impl Scene + use<>> {
    columns
        .iter()
        .zip(widths.iter().copied())
        .zip(directions.iter().copied())
        .map(|((column, width), direction)| {
            let label = header_label(column);
            header_cell_scene(label, column.id, width, column.numeric, direction, palette)
        })
        .collect()
}

/// One header cell: the pure column label plus, when sorted, the semantic
/// direction plate. Same bounded-line discipline as body cells — NoWrap +
/// clip — and the same main-axis alignment contract.
fn header_cell_scene(
    label: String,
    column_id: &'static str,
    width: f32,
    numeric_column: bool,
    direction: Option<bool>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let align = if numeric_column {
        JustifyContent::FlexEnd
    } else {
        JustifyContent::FlexStart
    };
    let is_sorted = direction.is_some();
    let indicator = crate::widgets::controls::sort_indicator_scene(direction, palette);
    let bg = if is_sorted {
        palette.nav_active_bg
    } else {
        bevy::color::Color::NONE
    };
    let radius = palette.control_radius_px;
    bsn! {
        Node {
            width: px(width),
            height: px(palette.control_height_px),
            flex_direction: FlexDirection::Row,
            justify_content: align,
            align_items: AlignItems::Center,
            padding: UiRect::horizontal(Val::Px(space_4())),
            column_gap: Val::Px(space_4()),
            border_radius: BorderRadius::all(Val::Px(radius)),
            overflow: Overflow::clip_x(),
        }
        BackgroundColor({ bg })
        ControlVisual(ControlTone::Surface, is_sorted)
        Button
        ProcessSortHeader(column_id)
        on(on_process_sort_header_activated)
        Children [
            ( Text(label) TextRole(Role::Caption) template_value(no_wrap_text()) Pickable::IGNORE ),
            { indicator },
        ]
    }
}

/// Body row with explicit column widths.
pub(crate) fn row_scene_with_widths(
    cells: &[String],
    columns: &[&ProcessColumnSpec],
    widths: &[f32],
) -> impl Scene + use<> {
    let owned: Vec<String> = cells.to_vec();
    let numeric: Vec<bool> = columns.iter().map(|column| column.numeric).collect();
    let cells = row_cells(&owned, widths, &numeric);
    bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_8()),
        }
        Children [
            { cells }
        ]
    }
}

/// Render adapter: one body row from pre-formatted cell strings. Cells pair
/// with the same column slice the header used; row height/spacing come from
/// the palette via the page (the row node here stays unstyled chrome).
pub(crate) fn row_scene(cells: &[String], columns: &[&ProcessColumnSpec]) -> impl Scene + use<> {
    let widths = distribute_column_widths(columns, None);
    row_scene_with_widths(cells, columns, &widths)
}

fn row_cells(cells: &[String], widths: &[f32], numeric: &[bool]) -> Vec<impl Scene + use<>> {
    cells
        .iter()
        .zip(widths.iter().copied())
        .zip(numeric.iter().copied())
        .map(|((cell, width), numeric_column)| {
            cell_scene(cell.clone(), width, numeric_column, false)
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/headless/table.rs"]
mod tests;
