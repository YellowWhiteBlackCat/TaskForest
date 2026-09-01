//! Process/info table: pure projection core + minimal bsn! render adapter.
//!
//! **Pure core** (no bevy types): the column vocabulary comes verbatim from
//! the ui-contract single source ([`PROCESS_COLUMNS`]) so the bevy table can
//! never drift from the GPUI/Iced/TUI column semantics; the sort projection
//! input is the contract's stable column token; and the virtual-scroll
//! window is a plain clamping function over (total, viewport, scroll).
//!
//! **Render adapter**: one header scene and one row scene. Row *material*
//! (which rows exist, their cell text) stays owned by the page + shell —
//! this layer only renders what it is handed, bounded by the window math.

use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn, template_value};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, FlexDirection, JustifyContent, Node, Overflow, Val, percent, px,
};
use bevy::ui::widget::Text;
use taskmanager_ui_contract::{PROCESS_COLUMNS, ProcessColumnSpec};

use crate::palette::{UiPalette, no_wrap_text, space_4, space_8};
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

/// Render adapter: the header row. One text cell per column, widths from the
/// contract's default-width tokens; numeric columns right-align; the sorted
/// column carries the semantic direction icon.
pub(crate) fn header_scene(
    columns: &[&ProcessColumnSpec],
    sort: Option<SortProjection>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let labels: Vec<String> = columns.iter().map(|column| header_label(column)).collect();
    let widths: Vec<f32> = columns.iter().map(|column| column.default_width).collect();
    let numeric: Vec<bool> = columns.iter().map(|column| column.numeric).collect();
    let directions: Vec<Option<bool>> = columns
        .iter()
        .map(|column| sorted_direction(column, sort))
        .collect();
    let cells = header_cells(&labels, &widths, &numeric, &directions, palette);
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
    labels: &[String],
    widths: &[f32],
    numeric: &[bool],
    directions: &[Option<bool>],
    palette: &UiPalette,
) -> Vec<impl Scene + use<>> {
    labels
        .iter()
        .zip(widths.iter().copied())
        .zip(numeric.iter().copied())
        .zip(directions.iter().copied())
        .map(|(((label, width), numeric_column), direction)| {
            header_cell_scene(label.clone(), width, numeric_column, direction, palette)
        })
        .collect()
}

/// One header cell: the pure column label plus, when sorted, the semantic
/// direction plate. Same bounded-line discipline as body cells — NoWrap +
/// clip — and the same main-axis alignment contract.
fn header_cell_scene(
    label: String,
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
    let indicator = crate::widgets::controls::sort_indicator_scene(direction, palette);
    bsn! {
        Node {
            width: px(width),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            justify_content: align,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
            overflow: Overflow::clip_x(),
        }
        Children [
            ( Text(label) TextRole(Role::Caption) template_value(no_wrap_text()) ),
            { indicator },
        ]
    }
}

/// Render adapter: one body row from pre-formatted cell strings. Cells pair
/// with the same column slice the header used; row height/spacing come from
/// the palette via the page (the row node here stays unstyled chrome).
pub(crate) fn row_scene(cells: &[String], columns: &[&ProcessColumnSpec]) -> impl Scene + use<> {
    let owned: Vec<String> = cells.to_vec();
    let widths: Vec<f32> = columns.iter().map(|column| column.default_width).collect();
    let numeric: Vec<bool> = columns.iter().map(|column| column.numeric).collect();
    let cells = row_cells(&owned, &widths, &numeric);
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
