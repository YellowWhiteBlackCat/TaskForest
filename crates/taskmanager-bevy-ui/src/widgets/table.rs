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

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::system::Res;
use bevy::ecs::system::{Commands, NonSendMut, Query};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::TextColor;
use bevy::text::{LineBreak, TextLayout, TextSpan};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    TextBackgroundColor, UiRect, Val, percent, px, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_core::core::text::match_ranges_ascii_ci;
use taskmanager_shell::SortCol;
use taskmanager_ui_contract::{IconId, PROCESS_COLUMNS, ProcessColumnSpec};

use crate::app::FrontendTrack;
use crate::pages::settings::ThemePreferences;
use crate::palette::{UiPalette, space_4, space_8};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::WindowPalette;
use crate::window::{CustomTextInk, Role, TextRole};
use taskmanager_ui_contract::ColumnResizePolicy;

pub(crate) mod contrast;
pub(crate) mod ellipsis;
pub(crate) mod figures;
pub(crate) mod resize;
pub(crate) mod selection;

use contrast::calibrate_muted_ink;
use ellipsis::BoundedCellText;
use figures::TabularFigures;
use resize::ColumnWidthNode;
use selection::CellSelection;

#[derive(Component, Clone, Default)]
pub(crate) struct ZeroValueCell(pub(crate) bool);

/// Dimming follows the live preference; missing cells never become zeros.
/// The muted ink is calibrated against the surfaces the value can sit on, so
/// a dimmed zero stays readable instead of fading into the row.
pub(crate) fn paint_zero_values(
    preferences: Res<ThemePreferences>,
    palette: Res<WindowPalette>,
    mut cells: Query<(&ZeroValueCell, &mut TextColor)>,
) {
    let muted = calibrate_muted_ink(
        palette.inner.dim_color,
        palette.inner.body_color,
        &[palette.inner.content_bg, palette.inner.nav_active_bg],
    );
    for (zero, mut color) in &mut cells {
        let selected = if zero.0 && preferences.gray_zero_values {
            muted
        } else {
            palette.inner.body_color
        };
        if color.0 != selected {
            color.0 = selected;
        }
    }
}

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

/// The narrowest a column may become when the table DISTRIBUTES the
/// available width, in pixels.
///
/// This is the frontend's LAYOUT floor, not the shared resize policy: a
/// column squeezed by the layout keeps its text legible. The resize POLICY
/// (what a user may drag a column to) is the contract's
/// (`taskmanager_ui_contract::columns`), and every frontend reads it.
pub(crate) const LAYOUT_MIN_COLUMN_WIDTH_PX: f32 = 40.0;

/// Clamp a column width to the contract's resize policy.
#[must_use]
pub(crate) fn clamp_column_width(width: f32) -> f32 {
    ColumnResizePolicy::PROCESS.clamp(width)
}

/// Distribute available horizontal width across table columns.
///
/// If `available_width` is `None` or non-positive, returns contract `default_width` for each column.
/// When `available_width` is `Some(w)`:
/// - If `w` exceeds total default width:
///   Fixed/numeric columns keep their legible default widths; the primary identity column
///   (`Name`) flexes to absorb the surplus space.
/// - If `w` is less than total default width:
///   Columns are scaled proportionally down to the layout floor.
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
    let total_min = LAYOUT_MIN_COLUMN_WIDTH_PX * (columns.len() as f32);

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
        vec![LAYOUT_MIN_COLUMN_WIDTH_PX; columns.len()]
    } else {
        let factor = (avail - total_min) / (total_base - total_min).max(1e-6);
        base_widths
            .iter()
            .map(|&base| {
                let shrunk =
                    LAYOUT_MIN_COLUMN_WIDTH_PX + (base - LAYOUT_MIN_COLUMN_WIDTH_PX) * factor;
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
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn header_scene(
    columns: &[&ProcessColumnSpec],
    sort: Option<SortProjection>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let widths = distribute_column_widths(columns, None);
    header_scene_with_widths(columns, &widths, sort, palette)
}

/// One cell's render inputs: the value, its slot, and the two pieces of
/// context the cell needs — the active search query (the highlight engine's
/// input) and whether the row carries an application identity (the inline
/// app icon).
pub(crate) struct CellRender<'a> {
    pub(crate) text: String,
    pub(crate) width: f32,
    pub(crate) numeric: bool,
    pub(crate) label: bool,
    pub(crate) column: &'static str,
    pub(crate) query: &'a str,
    pub(crate) app_icon: bool,
}

/// The search-match ranges of one cell value: the shared core match engine
/// (the single source of search-match geometry, ADR-020) over the cell's own
/// text. Only the identity column carries the highlight, exactly like the
/// reference surfaces.
pub(crate) fn highlight_ranges(
    column: &str,
    text: &str,
    query: &str,
) -> Vec<std::ops::Range<usize>> {
    if column != "Name" || query.trim().is_empty() {
        return Vec::new();
    }
    match_ranges_ascii_ci(text, query.trim())
}

/// The value split into its plain and highlighted segments. The segments are
/// contiguous and cover the whole value, so the cell renders exactly the text
/// it was handed.
pub(crate) fn highlight_segments(
    text: &str,
    ranges: &[std::ops::Range<usize>],
) -> Vec<(String, bool)> {
    if ranges.is_empty() {
        return vec![(text.to_owned(), false)];
    }
    let mut segments: Vec<(String, bool)> = Vec::new();
    let mut cursor = 0;
    for range in ranges {
        let start = range.start.min(text.len());
        let end = range.end.min(text.len());
        if start < cursor {
            continue;
        }
        if start > cursor {
            segments.push((text[cursor..start].to_owned(), false));
        }
        segments.push((text[start..end].to_owned(), true));
        cursor = end;
    }
    if cursor < text.len() {
        segments.push((text[cursor..].to_owned(), false));
    }
    segments
}

/// A data cell: the value in a bounded slot.
///
/// The value is a bounded slot: a wider value is cut at the column edge with
/// an ellipsis (see `ellipsis`), never wrapped into a second line. The
/// identity cell additionally carries the app icon as an INLINE flow box: the
/// icon shares the text's layout, so it is aligned with the name's baseline
/// instead of floating a pixel above it.
///
/// The palette is optional: a caller that renders a plain value (no search
/// highlight, no application icon) needs no theme at all.
fn cell_scene(cell: CellRender<'_>, palette: Option<&UiPalette>) -> impl Scene + use<> {
    let align = if cell.numeric {
        JustifyContent::FlexEnd
    } else {
        JustifyContent::FlexStart
    };
    let is_zero = cell.numeric && !cell.label && is_zero_value(&cell.text);
    let role = if cell.label {
        Role::Caption
    } else {
        Role::Body
    };
    let ranges = highlight_ranges(cell.column, &cell.text, cell.query);
    // The value lives in the root text unless a flow needs the paragraph: an
    // inline icon has to come BEFORE the value, and the highlighted segments
    // are spans of their own.
    let segments = if ranges.is_empty() {
        Vec::new()
    } else {
        highlight_segments(&cell.text, &ranges)
    };
    // The flow children: the app icon (a semantic plate, never a text glyph
    // — the tofu law) followed by the value's segments.
    let mut flow: Vec<Box<dyn Scene>> = Vec::new();
    if cell.app_icon
        && let Some(palette) = palette
    {
        flow.push(crate::icons::inline_icon_scene(
            IconId::Applications,
            palette,
            palette.dim_color,
        ));
    }
    for (segment, is_match) in &segments {
        let background = if *is_match {
            palette.map_or(Color::NONE, |palette| palette.selection_bg)
        } else {
            Color::NONE
        };
        // The theme's own query-highlight ink for a matched run.
        let highlight = palette.map_or(Color::NONE, |palette| palette.highlight_fg);
        if *is_match {
            // The matched run owns its ink: the window layer's custom-ink
            // seam keeps the declared highlight ink, the role observer would
            // otherwise stamp the role ink over it.
            flow.push(Box::new(bsn! {
                TextSpan({ segment })
                TextRole({ role })
                TextBackgroundColor({ background })
                TextColor({ highlight })
                CustomTextInk
            }) as Box<dyn Scene>);
        } else {
            flow.push(Box::new(bsn! {
                TextSpan({ segment })
                TextRole({ role })
                TextBackgroundColor({ background })
            }) as Box<dyn Scene>);
        }
    }
    // With a flow (an inline icon or highlighted segments) the root text is
    // empty and the value lives in the spans: the engine lays the root, the
    // spans and the flow boxes out as ONE paragraph.
    let root_text = if flow.is_empty() {
        cell.text.clone()
    } else {
        String::new()
    };

    let selection_id = cell.column.to_owned();
    let selection_content = cell.text.clone();
    bsn! {
        Node {
            width: px(cell.width),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            justify_content: align,
            align_items: AlignItems::Center,
            overflow: Overflow::clip_x(),
        }
        ColumnWidthNode({ cell.column })
        CellSelection { id: { selection_id }, content: { selection_content } }
        on(selection::on_cell_double_clicked)
        Children [
             Text({ root_text })
             TextRole({ role })
             TextLayout { linebreak: LineBreak::NoWrap }
             Node { flex_shrink: 0.0 }
             BoundedCellText
             ZeroValueCell({ is_zero })
             TabularFigures({ cell.numeric })
             Children [
                  { flow }
             ]
        ]
    }
}

/// Whether a value reads as a measured zero (the dimming policy's input).
/// A missing value is a dash, never a zero.
fn is_zero_value(text: &str) -> bool {
    matches!(text, "0" | "0.0%" | "0 B" | "0 B/s" | "0.0 B/s")
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
///
/// The direction plate and the resize gutter are part of the LABEL's text
/// flow: the plate is an inline flow box inside the label (so it shares the
/// label's baseline and clips with it), and the gutter rides the cell's
/// trailing edge as the drag affordance for the column width.
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
    let handle = vec![resize::resize_handle_scene(column_id)];
    let bg = if is_sorted {
        palette.nav_active_bg
    } else {
        bevy::color::Color::NONE
    };
    let radius = palette.control_radius_px;
    bsn! {
        Node {
            width: px(width),
            height: rem(palette.control_height_rem),
            flex_direction: FlexDirection::Row,
            justify_content: align,
            align_items: AlignItems::Center,
            padding: UiRect::horizontal(Val::Px(space_4())),
            column_gap: Val::Px(space_4()),
            border_radius: BorderRadius::all(Val::Px(radius)),
            overflow: Overflow::clip_x(),
        }
        BackgroundColor({ bg })
        ColumnWidthNode({ column_id })
        ControlVisual(ControlTone::Surface, is_sorted)
        Button
        ProcessSortHeader(column_id)
        on(on_process_sort_header_activated)
        Children [
             Text(label)
             TextRole(Role::Caption)
             TextLayout { linebreak: LineBreak::NoWrap }
             Node { flex_shrink: 0.0 }
             Pickable::IGNORE
             Children [
                  { indicator }
             ]
             --
             { handle }
        ]
    }
}

/// Body row with explicit column widths.
pub(crate) fn row_scene_with_widths(
    cells: &[String],
    columns: &[&ProcessColumnSpec],
    widths: &[f32],
    query: &str,
    app_icon: bool,
    palette: Option<&UiPalette>,
) -> impl Scene + use<> {
    let owned: Vec<String> = cells.to_vec();
    let cells = row_cells(&owned, columns, widths, query, app_icon, palette);
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
///
/// This is the plain form: the contract's default widths, no search
/// highlight and no application icon. A caller that wants the live table
/// treatment uses [`row_scene_with_widths`].
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn row_scene(cells: &[String], columns: &[&ProcessColumnSpec]) -> impl Scene + use<> {
    let widths = distribute_column_widths(columns, None);
    row_scene_with_widths(cells, columns, &widths, "", false, None)
}

fn row_cells(
    cells: &[String],
    columns: &[&ProcessColumnSpec],
    widths: &[f32],
    query: &str,
    app_icon: bool,
    palette: Option<&UiPalette>,
) -> Vec<impl Scene + use<>> {
    cells
        .iter()
        .zip(widths.iter().copied())
        .zip(columns.iter())
        .map(|((cell, width), column)| {
            cell_scene(
                CellRender {
                    text: cell.clone(),
                    width,
                    numeric: column.numeric,
                    label: false,
                    column: column.id,
                    query,
                    app_icon: app_icon && column.id == "Name",
                },
                palette,
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/headless/table.rs"]
mod tests;
