//! Services page: layout split, table, details column, and log band.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Cell, Row};
use taskmanager_application::i18n::t;
use taskmanager_application::source_notice;
use taskmanager_shell::service_cycle_members;

use super::super::containers::{
    WindowedTableOutcome, WindowedTableProps, render_windowed_table, sort_header_row,
};
use super::super::{TablePanelProjection, TuiFocusPlan, process_details};
use super::{
    SourceNoticeLayout, name_status_sort, render_source_notice, service_details, service_log,
    services_search, source_notice_layout, source_state_message,
};
use crate::{TuiApp, TuiTheme};

/// The Services page's table, the optional selected-service details column
/// and the optional log band. Source-notice geometry is resolved before the
/// details and log splits so both the renderer and `table_hit` address the
/// same table rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ServicesPageLayout {
    pub(crate) area: Rect,
    pub(crate) source: SourceNoticeLayout,
    /// The page's own search field (same geometry contract as the
    /// Applications field). Zero-height when the terminal is too short.
    pub(crate) search: Rect,
    pub(crate) details: Option<Rect>,
    pub(crate) table: Rect,
    pub(crate) log: Option<Rect>,
}

#[must_use]
pub(crate) fn services_page_layout(app: &TuiApp, area: Rect) -> ServicesPageLayout {
    let source = source_notice_layout(area, app.projection().services_source.as_deref());
    // The Services search field owns its own band above the table. A
    // terminal too short to spare three rows keeps the table and drops the
    // field (Ctrl+F still opens the slot; a later resize paints it).
    let search_height = if source.content.height >= 8 { 3 } else { 0 };
    let [search, content] =
        Layout::vertical([Constraint::Length(search_height), Constraint::Min(1)])
            .areas(source.content);
    // The selected-service details column (GPUI services_view/details
    // parity) shares the table's row band on the right. It is a pure
    // addition with one honest rule: a narrow or short terminal, or an empty
    // inventory, yields the WHOLE column back to the table — the panel never
    // overlaps it nor squeezes it below its usable width.
    let (content, details) = service_details::column_split(app, content);
    if app.shell.service_log.is_none() {
        return ServicesPageLayout {
            area,
            source,
            search,
            details,
            table: content,
            log: None,
        };
    }
    let log_height = (content.height / 2).clamp(8, 14);
    let [table, log] =
        Layout::vertical([Constraint::Min(4), Constraint::Length(log_height)]).areas(content);
    ServicesPageLayout {
        area,
        source,
        search,
        details,
        table,
        log: Some(log),
    }
}

pub(crate) fn render_services(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    layout: ServicesPageLayout,
    panel: TablePanelProjection,
    focus: TuiFocusPlan,
) {
    // The page query is shell-owned; this one filtered sorted view feeds the
    // table, the row count, the details column, and every row action.
    let query = app.shell.services_query.clone();
    let services = app.shell.visible_services();
    let query_filters = !query.trim().is_empty();
    let cycle_members = app
        .projection()
        .services
        .as_deref()
        .map(service_cycle_members)
        .unwrap_or_default();
    let source_message = source_state_message(
        app.projection().services_source.as_deref(),
        t("empty.no_services_reported"),
        app.source_retry_request().is_some(),
    );
    // A healthy source with an empty filtered list is honestly "no match";
    // a source failure keeps its own diagnostic even while a query is typed.
    let state_message = if query_filters
        && app
            .projection()
            .services_source
            .as_deref()
            .and_then(source_notice)
            .is_none()
    {
        t("empty.no_services_match_query").to_owned()
    } else {
        source_message
    };
    services_search::render(frame, app, theme, layout.search, focus);
    // The empty panel never covers the search field: a query that matches
    // nothing must stay editable/clearable in place.
    let state_area = if layout.search.height > 0 {
        Rect::new(
            layout.area.x,
            layout.search.bottom(),
            layout.area.width,
            layout.area.bottom().saturating_sub(layout.search.bottom()),
        )
    } else {
        layout.area
    };
    let painted = render_windowed_table(
        frame,
        WindowedTableProps {
            theme,
            panel,
            title: t("page.services_help"),
            header: sort_header_row(
                [
                    t("common.service"),
                    t("common.status"),
                    t("common.description"),
                ],
                theme,
                name_status_sort(app.shell.services_sort),
            ),
            widths: vec![
                Constraint::Percentage(38),
                Constraint::Length(12),
                Constraint::Min(20),
            ],
            column_spacing: 2,
            // With no rows the honest state panel owns the whole page area,
            // including the log band's slot below.
            state_area,
            state_message: &state_message,
        },
        |index| {
            let service = services[index];
            let color = match service.status.as_str() {
                "Active" => theme.good,
                "Failed" => theme.danger,
                _ => theme.dim,
            };
            let status = if cycle_members.contains(&service.id) {
                format!("{} · {}", service.status.as_str(), t("svc.cycle"))
            } else {
                service.status.as_str().to_owned()
            };
            Row::new([
                process_details::search_highlight_cell(&service.name, &query, query_filters, theme),
                Cell::from(status).style(Style::new().fg(if cycle_members.contains(&service.id) {
                    theme.warn
                } else {
                    color
                })),
                process_details::search_highlight_cell(
                    &service.description,
                    &query,
                    query_filters,
                    theme,
                ),
            ])
        },
    );
    if painted != WindowedTableOutcome::Table {
        return;
    }
    let _ = render_source_notice(
        frame,
        app,
        theme,
        layout.source,
        app.projection().services_source.as_deref(),
    );
    // The selected-service details column (GPUI services-view parity): the
    // state triplet plus the read-only relation rows, following the same
    // `selected` cursor as the table. Painted only when the table itself
    // painted rows — an empty or failed inventory leaves the state panel in
    // charge of the whole page, so the column's slot stays with it.
    if let Some(details_area) = layout.details {
        service_details::render(frame, app, theme, details_area);
    }
    // The open service-log stream (opened with `o` on the Services page) owns
    // the bottom band of the page; the table gets the rest. The log panel is
    // bounded and honest: entries render from the shared feed, an empty or
    // unavailable stream renders its state instead of fabricating lines.
    if let Some(log) = layout.log {
        service_log::render(frame, app, theme, log);
    }
}
