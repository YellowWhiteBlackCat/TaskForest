//! Table and fact pages for the Ratatui renderer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Wrap};
use taskmanager_application::{SourceNotice, i18n::t, source_notice};
use taskmanager_core::core::source::SourceStatus;
use taskmanager_shell::presentation::MISSING_VALUE;

use super::containers::{
    WindowedTableOutcome, WindowedTableProps, render_windowed_table, sort_header_row,
};
use super::{TablePanelProjection, kv, panel};
use crate::{TuiApp, TuiTheme};

mod service_details;
mod service_log;
mod system_data;

fn source_title(notice: SourceNotice) -> &'static str {
    match notice {
        SourceNotice::Partial(_) => t("source.partial_title"),
        SourceNotice::Unavailable(_) => t("source.unavailable_title"),
    }
}

fn source_state_message(
    sources: Option<&[SourceStatus]>,
    fallback: &str,
    retryable: bool,
) -> String {
    let Some(notice) = sources.and_then(source_notice) else {
        return fallback.to_owned();
    };
    let reason = taskmanager_shell::presentation::control_error_detail(notice.failure());
    let action = if retryable && notice.is_retryable() {
        format!(" · r {}", t("common.refresh"))
    } else {
        format!(" · {}", t("source.retry_after_change"))
    };
    format!("{}: {reason}{action}", source_title(notice))
}

/// The source-notice split shared by the renderer and pointer hit-tests. A
/// source notice consumes four rows only when the area can afford it; otherwise
/// the page keeps its full area and the caller's empty/state text remains the
/// honest fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SourceNoticeLayout {
    pub(super) notice: Option<Rect>,
    pub(super) content: Rect,
}

#[must_use]
pub(super) fn source_notice_layout(
    area: Rect,
    sources: Option<&[SourceStatus]>,
) -> SourceNoticeLayout {
    let has_notice = sources.and_then(source_notice).is_some() && area.height >= 5;
    if !has_notice {
        return SourceNoticeLayout {
            notice: None,
            content: area,
        };
    }
    let [notice, content] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(1)]).areas(area);
    SourceNoticeLayout {
        notice: Some(notice),
        content,
    }
}

/// Header index of the shared Name/Status keyboard sort for the pages whose
/// first column is the name and second the state. Name → header index 0,
/// Status → index 1; the user-only columns can never land in these sorts (the
/// shell cycle excludes them), so the fallback is defensive only.
fn name_status_sort(
    sort: Option<(taskmanager_shell::InfoSortCol, taskmanager_shell::SortDir)>,
) -> Option<(usize, taskmanager_shell::SortDir)> {
    sort.map(|(column, direction)| {
        (
            match column {
                taskmanager_shell::InfoSortCol::Name => 0,
                taskmanager_shell::InfoSortCol::Status => 1,
                taskmanager_shell::InfoSortCol::Session | taskmanager_shell::InfoSortCol::Seat => 0,
            },
            direction,
        )
    })
}

/// Draw a compact, non-blocking source warning and return the area left for
/// the table. The TUI uses a keyboard affordance rather than a pointer button,
/// but the request scope remains the same as Iced/GPUI.
fn render_source_notice(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    layout: SourceNoticeLayout,
    sources: Option<&[SourceStatus]>,
) -> Rect {
    let Some(notice) = sources.and_then(source_notice) else {
        return layout.content;
    };
    let Some(notice_area) = layout.notice else {
        return layout.content;
    };
    let retryable = app.source_retry_request().is_some() && notice.is_retryable();
    let message = source_state_message(sources, "", retryable);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            message,
            Style::new().fg(theme.warn),
        )))
        .block(panel(source_title(notice), theme))
        .wrap(Wrap { trim: true }),
        notice_area,
    );
    layout.content
}

/// The Services page's table, the optional selected-service details column
/// and the optional log band. Source-notice geometry is resolved before the
/// details and log splits so both the renderer and `table_hit` address the
/// same table rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ServicesPageLayout {
    pub(super) area: Rect,
    pub(super) source: SourceNoticeLayout,
    pub(super) details: Option<Rect>,
    pub(super) table: Rect,
    pub(super) log: Option<Rect>,
}

#[must_use]
pub(super) fn services_page_layout(app: &TuiApp, area: Rect) -> ServicesPageLayout {
    let source = source_notice_layout(area, app.projection().services_source.as_deref());
    // The selected-service details column (GPUI services_view/details
    // parity) shares the table's row band on the right. It is a pure
    // addition with one honest rule: a narrow or short terminal, or an empty
    // inventory, yields the WHOLE column back to the table — the panel never
    // overlaps it nor squeezes it below its usable width.
    let (content, details) = service_details::column_split(app, source.content);
    if app.shell.service_log.is_none() {
        return ServicesPageLayout {
            area,
            source,
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
        details,
        table,
        log: Some(log),
    }
}

/// The Startup page's optional timeline and table areas. `table_before_notice`
/// is retained because an empty source uses the page-level state panel in that
/// area, while a non-empty source paints its notice and uses `table` below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StartupPageLayout {
    pub(super) timeline: Option<Rect>,
    pub(super) table_before_notice: Rect,
    pub(super) source: SourceNoticeLayout,
    pub(super) table: Rect,
}

#[must_use]
pub(super) fn startup_page_layout(
    area: Rect,
    timeline_rows: Option<usize>,
    sources: Option<&[SourceStatus]>,
) -> StartupPageLayout {
    let (timeline, table_before_notice) = match timeline_rows {
        Some(rows) if area.height >= 12 => {
            let height = rows
                .saturating_add(2)
                .min(usize::from(area.height / 2))
                .min(usize::from(u16::MAX));
            let [timeline, table] = Layout::vertical([
                Constraint::Length(u16::try_from(height).unwrap_or(u16::MAX)),
                Constraint::Min(1),
            ])
            .areas(area);
            (Some(timeline), table)
        }
        _ => (None, area),
    };
    let source = source_notice_layout(table_before_notice, sources);
    let table = source.content;
    StartupPageLayout {
        timeline,
        table_before_notice,
        source,
        table,
    }
}

/// The Users page's feedback band and source-notice/table split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UsersPageLayout {
    pub(super) table_before_notice: Rect,
    pub(super) source: SourceNoticeLayout,
    pub(super) table: Rect,
    pub(super) feedback: Option<Rect>,
}

#[must_use]
pub(super) fn users_page_layout(app: &TuiApp, area: Rect) -> UsersPageLayout {
    let (table_before_notice, feedback) =
        if app.shell.projection().session_control_feedback.is_some() {
            let [table, feedback] =
                Layout::vertical([Constraint::Min(5), Constraint::Length(1)]).areas(area);
            (table, Some(feedback))
        } else {
            (area, None)
        };
    let source = source_notice_layout(
        table_before_notice,
        app.projection().sessions_source.as_deref(),
    );
    let table = source.content;
    UsersPageLayout {
        table_before_notice,
        source,
        table,
        feedback,
    }
}

pub(super) fn render_services(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    layout: ServicesPageLayout,
    panel: TablePanelProjection,
) {
    // Rows project through the shared sort order (provider order until a
    // header click picks a column), so the selection index always maps to the
    // same visible order the Iced frontend renders.
    let sorted_services = app.sorted_services();
    let state_message = source_state_message(
        app.projection().services_source.as_deref(),
        t("empty.no_services_reported"),
        app.source_retry_request().is_some(),
    );
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
            state_area: layout.area,
            state_message: &state_message,
        },
        |index| {
            let service = sorted_services[index];
            let color = match service.status.as_str() {
                "Active" => theme.good,
                "Failed" => theme.danger,
                _ => theme.dim,
            };
            Row::new([
                Cell::from(service.name.as_str()),
                Cell::from(service.status.as_str()).style(Style::new().fg(color)),
                Cell::from(service.description.as_str()),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SystemFactViewport {
    start: usize,
    end: usize,
    total: usize,
}

impl SystemFactViewport {
    fn resolve(total: usize, requested: usize, area: Rect) -> Self {
        let rows = usize::from(area.height.saturating_sub(2));
        let visible = total.min(rows);
        let start = requested.min(total.saturating_sub(visible));
        Self {
            start,
            end: start.saturating_add(visible),
            total,
        }
    }

    const fn is_windowed(self) -> bool {
        self.end.saturating_sub(self.start) < self.total
    }
}

pub(super) fn render_system(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let sections = system_data::system_sections(
        app.projection().hardware.as_ref(),
        app.projection().snapshot.as_ref(),
        app.projection().npu_inventory.as_ref(),
    );
    let mut lines = Vec::new();
    for section in &sections {
        lines.push(Line::from(Span::styled(
            section.title.as_str(),
            Style::new().fg(theme.accent),
        )));
        lines.extend(
            section
                .facts
                .iter()
                .map(|fact| kv(&fact.label, fact.value.clone(), theme)),
        );
    }

    let viewport = SystemFactViewport::resolve(lines.len(), app.system_scroll, area);
    let title = if viewport.is_windowed() {
        format!(
            "{} · ↑/↓ {}–{} / {}",
            t("page.system_help"),
            viewport.start.saturating_add(1),
            viewport.end,
            viewport.total,
        )
    } else {
        t("page.system_help").to_owned()
    };
    frame.render_widget(
        Paragraph::new(lines[viewport.start..viewport.end].to_vec())
            .block(panel(&title, theme))
            .wrap(Wrap { trim: true }),
        area,
    );
}

pub(super) fn render_startup(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    page_layout: StartupPageLayout,
    panel: TablePanelProjection,
) {
    // Boot-timeline waterfall (BN-05): a bounded display-only block above the
    // table. It never joins the selection domain — arrow keys keep moving the
    // table cursor exactly as before the block existed. The block keeps at
    // most half the page height so the table stays usable on small terms.
    if let Some(timeline_area) = page_layout.timeline {
        super::boot_timeline::render_boot_timeline(
            frame,
            theme,
            app.projection().startup_boot_evidence.as_ref(),
            timeline_area,
        );
    }
    // Project the canonical order first, then materialize only the terminal
    // viewport below. Sorting remains complete for keyboard semantics; row
    // and cell construction is bounded by the visible window.
    let sorted_startup = app.sorted_startup_entries();
    let state_message = source_state_message(
        app.projection().startup_source.as_deref(),
        t("empty.no_startup_reported"),
        app.source_retry_request().is_some(),
    );
    let painted = render_windowed_table(
        frame,
        WindowedTableProps {
            theme,
            panel,
            title: t("startup.applications"),
            header: sort_header_row(
                [
                    t("common.name"),
                    t("common.state"),
                    t("startup.source"),
                    t("startup.impact"),
                    t("startup.command"),
                ],
                theme,
                // The keyboard sort marks the active header column. Name →
                // index 0, State → index 1; the user-only columns never reach
                // a Startup sort.
                name_status_sort(app.shell.startup_sort),
            ),
            widths: vec![
                Constraint::Percentage(24),
                Constraint::Length(10),
                Constraint::Length(22),
                Constraint::Length(16),
                Constraint::Min(18),
            ],
            column_spacing: 2,
            state_area: page_layout.table_before_notice,
            state_message: &state_message,
        },
        |index| {
            let entry = sorted_startup[index];
            Row::new([
                Cell::from(entry.name.as_str()),
                Cell::from(if entry.enabled {
                    t("common.enabled")
                } else {
                    t("common.disabled")
                }),
                Cell::from(startup_source_text(entry)),
                Cell::from(startup_impact_text(entry)),
                Cell::from(entry.exec.as_str()),
            ])
        },
    );
    if painted == WindowedTableOutcome::Table {
        let _ = render_source_notice(
            frame,
            app,
            theme,
            page_layout.source,
            app.projection().startup_source.as_deref(),
        );
    }
}

/// The source column with its scope suffix (GPUI parity: the row reads
/// `Desktop Entry · User` instead of the bare provider label).
pub(super) fn startup_source_text(entry: &taskmanager_core::core::startup::StartupEntry) -> String {
    format!(
        "{} · {}",
        entry.source.as_str(),
        startup_scope_text(entry.scope)
    )
}

fn startup_scope_text(scope: taskmanager_core::core::startup::StartupScope) -> &'static str {
    match scope {
        taskmanager_core::core::startup::StartupScope::User => t("startup.scope_user"),
        taskmanager_core::core::startup::StartupScope::System => t("startup.scope_system"),
        taskmanager_core::core::startup::StartupScope::Session => t("startup.scope_session"),
        taskmanager_core::core::startup::StartupScope::Unknown => t("startup.scope_unknown"),
    }
}

/// The impact column with its evidence (GPUI parity: `Low · 42 ms` for a
/// measured boot impact, `Low · unmeasured` when the provider could not
/// instrument it — never a fabricated duration).
pub(super) fn startup_impact_text(entry: &taskmanager_core::core::startup::StartupEntry) -> String {
    match entry.impact_evidence {
        taskmanager_core::core::startup::StartupImpactEvidence::Measured { duration_ms } => {
            format!("{} · {duration_ms} ms", t(entry.impact.i18n_key()))
        }
        taskmanager_core::core::startup::StartupImpactEvidence::Unknown { .. } => {
            format!(
                "{} · {}",
                t(entry.impact.i18n_key()),
                t("startup.impact_unmeasured")
            )
        }
    }
}

pub(super) fn render_users(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    page_layout: UsersPageLayout,
    panel: TablePanelProjection,
) {
    let feedback = session_feedback_line(app, theme);
    let feedback_area = page_layout.feedback;
    // Project the canonical order first, then materialize only the terminal
    // viewport below. The selected index remains global and is remapped to
    // the bounded table slice at the render boundary.
    let sorted_sessions = app.sorted_sessions();
    // An empty list from a FAILED source must not read as "no sessions":
    // the state panel carries the typed reason (GPUI empty_state_failure
    // parity).
    let state_message = source_state_message(
        app.projection().sessions_source.as_deref(),
        t("users.no_sessions"),
        app.source_retry_request().is_some(),
    );
    let painted = render_windowed_table(
        frame,
        WindowedTableProps {
            theme,
            panel,
            title: t("users.sessions_title"),
            header: sort_header_row(
                [
                    t("users.session"),
                    t("common.user"),
                    t("users.seat"),
                    t("users.tty"),
                    t("common.type"),
                    t("users.since"),
                ],
                theme,
                // The keyboard sort marks the active header column. Session →
                // index 0, User → index 1, Seat → index 2; the service-only
                // Status column never reaches a Users sort.
                app.shell.sessions_sort.map(|(column, direction)| {
                    (
                        match column {
                            taskmanager_shell::InfoSortCol::Session => 0,
                            taskmanager_shell::InfoSortCol::Name => 1,
                            taskmanager_shell::InfoSortCol::Seat => 2,
                            taskmanager_shell::InfoSortCol::Status => 1,
                        },
                        direction,
                    )
                }),
            ),
            widths: vec![
                Constraint::Length(8),
                Constraint::Percentage(22),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(9),
                Constraint::Min(16),
            ],
            column_spacing: 2,
            state_area: page_layout.table_before_notice,
            state_message: &state_message,
        },
        |index| {
            let session = sorted_sessions[index];
            Row::new([
                Cell::from(session.id.as_str()),
                Cell::from(session.user.as_str()),
                Cell::from(session.seat.as_deref().unwrap_or(MISSING_VALUE)),
                Cell::from(session.tty.as_deref().unwrap_or(MISSING_VALUE)),
                Cell::from(if session.remote {
                    t("users.remote")
                } else {
                    t("users.local")
                }),
                Cell::from(session.timestamp.as_deref().unwrap_or(MISSING_VALUE)),
            ])
        },
    );
    if painted == WindowedTableOutcome::Table {
        let _ = render_source_notice(
            frame,
            app,
            theme,
            page_layout.source,
            app.projection().sessions_source.as_deref(),
        );
    }
    if let (Some(feedback), Some(feedback_area)) = (feedback, feedback_area) {
        frame.render_widget(Paragraph::new(feedback), feedback_area);
    }
}

/// The last accepted session-control outcome, rendered under the Users table
/// (GPUI feedback_status_line parity): success green / failure red, `None`
/// while no outcome has been accepted.
fn session_feedback_line(app: &TuiApp, theme: TuiTheme) -> Option<Line<'static>> {
    let outcome = app.shell.projection().session_control_feedback.as_ref()?;
    let target = outcome.session_id.to_string();
    let action = match outcome.action {
        taskmanager_core::core::session::SessionControlAction::Disconnect => t("users.disconnect"),
        taskmanager_core::core::session::SessionControlAction::Lock => t("users.lock"),
    };
    match &outcome.result {
        Ok(()) => Some(Line::from(Span::styled(
            t("feedback.action_succeeded")
                .replace("{action}", action)
                .replace("{target}", &target),
            Style::new().fg(theme.good),
        ))),
        Err(error) => Some(Line::from(Span::styled(
            t("feedback.action_failed_detail")
                .replace("{action}", action)
                .replace("{target}", &target)
                .replace("{detail}", &format!("{error:?}")),
            Style::new().fg(theme.danger),
        ))),
    }
}
