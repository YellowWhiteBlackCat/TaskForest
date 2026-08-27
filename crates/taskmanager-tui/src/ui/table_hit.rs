//! Click-to-select hit-testing (input line): the pure projection from a
//! terminal cell to the table row the renderer painted there. This module
//! is the SINGLE SOURCE of that mapping for pointer input — it recomputes
//! the table panel Rect with the SAME `Layout` constraint sequences the
//! renderers use (`render`'s header/body/footer split, the per-page bands,
//! `table_window`'s viewport), so a click and the keyboard address the same
//! visual row projection. The alignment is pinned behaviorally by
//! `runtime::seam` tests that render real frames and check the highlight
//! row against this projection.
//!
//! Modeled surface (honest boundaries, each locked by a test):
//! - Bare left click on a DATA row of Applications, Services, Startup, and
//!   Users selects through the same page-specific projection as the keyboard.
//! - Not modeled: the App-history page (the shell clamps its keyboard cursor there, so
//!   pointer selection would diverge from the keyboard), clicks on headers,
//!   borders, and anything outside the panel, and every click while any
//!   modal or overlay owns the screen.

use ratatui::layout::{Constraint, Layout, Rect};
use taskmanager_application::AppPage;

use crate::TuiApp;

use super::table_window;

/// A panel border row, a header row, and the header's bottom margin sit
/// above the first data row inside every table panel (`render_table`'s
/// `Block::borders(ALL)` + header row with `bottom_margin(1)`).
const DATA_ROW_OFFSET: u16 = 3;

/// The Applications page's fixed bands above/below the table
/// (`process_table::render_processes`): search field, table, details.
const PROCESS_DETAILS_HEIGHT: u16 = 18;

/// Where the page's main table panel lives and how many rows it is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TablePanelProjection {
    /// The panel (bordered block) Rect inside the frame.
    pub(crate) area: Rect,
    /// Rows in the renderer's current projection for the page.
    pub(crate) total: usize,
}

/// Split a frame exactly like `render`: header (4) / body (Min 8) / footer (3).
fn body_area(frame: Rect) -> Rect {
    let [_header, body, _footer] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(8),
        Constraint::Length(3),
    ])
    .areas(frame);
    body
}

/// The source-notice band `pages::render_source_notice` carves off before a
/// table when a source needs to speak (None leaves the area untouched).
fn after_source_notice(
    area: Rect,
    sources: Option<&[taskmanager_application::SourceStatus]>,
) -> Rect {
    if sources
        .and_then(taskmanager_application::source_notice)
        .is_none()
        || area.height < 5
    {
        return area;
    }
    let [_notice, table] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(1)]).areas(area);
    table
}

/// Project the current page's table panel inside `frame`. `None` on pages
/// without a keyboard-addressable table.
pub(crate) fn table_panel_projection(app: &TuiApp, frame: Rect) -> Option<TablePanelProjection> {
    let body = body_area(frame);
    match app.page() {
        AppPage::Performance | AppPage::System | AppPage::AppHistory => None,
        AppPage::Applications => {
            let [_search, table, _details] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(PROCESS_DETAILS_HEIGHT),
            ])
            .areas(body);
            Some(TablePanelProjection {
                area: table,
                total: app.visual_row_count(),
            })
        }
        AppPage::Services => {
            let mut area = after_source_notice(body, app.projection().services_source.as_deref());
            let total = app.sorted_services().len();
            if app.shell.service_log.is_some() {
                let log_height = (area.height / 2).clamp(8, 14);
                let [table, _log] =
                    Layout::vertical([Constraint::Min(4), Constraint::Length(log_height)])
                        .areas(area);
                area = table;
            }
            Some(TablePanelProjection { area, total })
        }
        AppPage::Startup => {
            let timeline = super::boot_timeline::project_timeline(
                app.projection().startup_boot_evidence.as_ref(),
            );
            let mut area = body;
            if let Some(ref projection) = timeline
                && area.height >= 12
            {
                let height = (projection.rows.len() + 2).min(usize::from(area.height / 2));
                let [_timeline, table] = Layout::vertical([
                    Constraint::Length(u16::try_from(height).unwrap_or(u16::MAX)),
                    Constraint::Min(1),
                ])
                .areas(area);
                area = table;
            }
            area = after_source_notice(area, app.projection().startup_source.as_deref());
            Some(TablePanelProjection {
                area,
                total: app.sorted_startup_entries().len(),
            })
        }
        AppPage::Users => {
            let mut area = body;
            if app.shell.projection().session_control_feedback.is_some() {
                let [table, _feedback] =
                    Layout::vertical([Constraint::Min(5), Constraint::Length(1)]).areas(area);
                area = table;
            }
            area = after_source_notice(area, app.projection().sessions_source.as_deref());
            Some(TablePanelProjection {
                area,
                total: app.sorted_sessions().len(),
            })
        }
    }
}

/// Map a bare left click at absolute cell (`column`, `row`) to the global
/// row index in the renderer's current projection. `None` when the click is
/// not on a visible data row (header, border, margin, outside the panel, or
/// a page/shape without pointer-addressable rows).
pub(crate) fn row_at(app: &TuiApp, frame: Rect, column: u16, row: u16) -> Option<usize> {
    let panel = table_panel_projection(app, frame)?;
    if column < panel.area.x
        || column >= panel.area.x + panel.area.width
        || row < panel.area.y + DATA_ROW_OFFSET
    {
        return None;
    }
    let window = table_window(panel.total, app.selected, panel.area);
    let offset = row - (panel.area.y + DATA_ROW_OFFSET);
    let visible = window.end - window.start;
    if offset >= u16::try_from(visible).unwrap_or(u16::MAX) {
        return None;
    }
    Some(window.start + usize::from(offset))
}
