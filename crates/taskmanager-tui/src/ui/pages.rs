//! Table and fact pages for the Ratatui renderer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Wrap};
use taskmanager_application::{SourceNotice, SourceStatus, i18n::t, source_notice};
use taskmanager_shell::presentation::MISSING_VALUE;

use super::{TableRenderProps, kv, panel, render_empty_panel, render_table, table_window};
use crate::{TuiApp, TuiTheme};

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

/// Draw a compact, non-blocking source warning and return the area left for
/// the table. The TUI uses a keyboard affordance rather than a pointer button,
/// but the request scope remains the same as Iced/GPUI.
fn render_source_notice(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    sources: Option<&[SourceStatus]>,
) -> Rect {
    let Some(notice) = sources.and_then(source_notice) else {
        return area;
    };
    if area.height < 5 {
        return area;
    }
    let [notice_area, table_area] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(1)]).areas(area);
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
    table_area
}

pub(super) fn render_services(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    // Rows project through the shared sort order (provider order until a
    // header click picks a column), so the selection index always maps to the
    // same visible order the Iced frontend renders.
    let sorted_services = app.sorted_services();
    if sorted_services.is_empty() {
        let message = source_state_message(
            app.projection().services_source.as_deref(),
            t("empty.no_services_reported"),
            app.source_retry_request().is_some(),
        );
        render_empty_panel(frame, theme, area, t("page.services_help"), &message);
        return;
    }
    let area = render_source_notice(
        frame,
        app,
        theme,
        area,
        app.projection().services_source.as_deref(),
    );
    // The open service-log stream (opened with `o` on the Services page) owns
    // the bottom band of the page; the table gets the rest. The log panel is
    // bounded and honest: entries render from the shared feed, an empty or
    // unavailable stream renders its state instead of fabricating lines.
    let log_height = if app.shell.service_log.is_some() {
        (area.height / 2).clamp(8, 14)
    } else {
        0
    };
    let table_area = if log_height > 0 {
        let [table, log] =
            Layout::vertical([Constraint::Min(4), Constraint::Length(log_height)]).areas(area);
        service_log::render(frame, app, theme, log);
        table
    } else {
        area
    };
    let row_window = table_window(sorted_services.len(), app.selected, table_area);
    let services: Vec<Row<'_>> = sorted_services[row_window.start..row_window.end]
        .iter()
        .map(|service| {
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
        })
        .collect();
    render_table(
        frame,
        TableRenderProps {
            theme,
            area: table_area,
            title: t("page.services_help"),
            rows: services,
            widths: [
                Constraint::Percentage(38),
                Constraint::Length(12),
                Constraint::Min(20),
            ],
            headers: [
                t("common.service"),
                t("common.status"),
                t("common.description"),
            ],
            selected: row_window.selected,
            // The keyboard sort (`s`/`S`) writes the shared slot; the header
            // marks the active column with ▲/▼. Name → header index 0, Status →
            // index 1; Description is not sortable. The user-only columns can
            // never land in a Services sort (the shell cycle excludes them), so
            // the fallback is defensive only.
            sort: app.shell.services_sort.map(|(column, direction)| {
                (
                    match column {
                        taskmanager_shell::InfoSortCol::Name => 0,
                        taskmanager_shell::InfoSortCol::Status => 1,
                        taskmanager_shell::InfoSortCol::Session
                        | taskmanager_shell::InfoSortCol::Seat => 0,
                    },
                    direction,
                )
            }),
        },
    );
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

pub(super) fn render_startup(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    // Boot-timeline waterfall (BN-05): a bounded display-only block above the
    // table. It never joins the selection domain — arrow keys keep moving the
    // table cursor exactly as before the block existed. The block keeps at
    // most half the page height so the table stays usable on small terms.
    let timeline =
        super::boot_timeline::project_timeline(app.projection().startup_boot_evidence.as_ref());
    let (timeline_area, table_area) = match timeline {
        Some(ref projection) if area.height >= 12 => {
            let height = (projection.rows.len() + 2)
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
    if let Some(timeline_area) = timeline_area {
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
    if sorted_startup.is_empty() {
        let message = source_state_message(
            app.projection().startup_source.as_deref(),
            t("empty.no_startup_reported"),
            app.source_retry_request().is_some(),
        );
        render_empty_panel(
            frame,
            theme,
            table_area,
            t("startup.applications"),
            &message,
        );
        return;
    }
    let table_area = render_source_notice(
        frame,
        app,
        theme,
        table_area,
        app.projection().startup_source.as_deref(),
    );
    let row_window = table_window(sorted_startup.len(), app.selected, table_area);
    let rows: Vec<Row<'_>> = sorted_startup[row_window.start..row_window.end]
        .iter()
        .map(|entry| {
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
        })
        .collect();
    render_table(
        frame,
        TableRenderProps {
            theme,
            area: table_area,
            title: t("startup.applications"),
            rows,
            widths: [
                Constraint::Percentage(24),
                Constraint::Length(10),
                Constraint::Length(22),
                Constraint::Length(16),
                Constraint::Min(18),
            ],
            headers: [
                t("common.name"),
                t("common.state"),
                t("startup.source"),
                t("startup.impact"),
                t("startup.command"),
            ],
            selected: row_window.selected,
            // The keyboard sort marks the active header column. Name → index 0,
            // State → index 1; the user-only columns never reach a Startup sort.
            sort: app.shell.startup_sort.map(|(column, direction)| {
                (
                    match column {
                        taskmanager_shell::InfoSortCol::Name => 0,
                        taskmanager_shell::InfoSortCol::Status => 1,
                        taskmanager_shell::InfoSortCol::Session
                        | taskmanager_shell::InfoSortCol::Seat => 0,
                    },
                    direction,
                )
            }),
        },
    );
}

/// The source column with its scope suffix (GPUI parity: the row reads
/// `Desktop Entry · User` instead of the bare provider label).
pub(super) fn startup_source_text(entry: &taskmanager_application::StartupEntry) -> String {
    format!(
        "{} · {}",
        entry.source.as_str(),
        startup_scope_text(entry.scope)
    )
}

fn startup_scope_text(scope: taskmanager_application::StartupScope) -> &'static str {
    match scope {
        taskmanager_application::StartupScope::User => t("startup.scope_user"),
        taskmanager_application::StartupScope::System => t("startup.scope_system"),
        taskmanager_application::StartupScope::Session => t("startup.scope_session"),
        taskmanager_application::StartupScope::Unknown => t("startup.scope_unknown"),
    }
}

/// The impact column with its evidence (GPUI parity: `Low · 42 ms` for a
/// measured boot impact, `Low · unmeasured` when the provider could not
/// instrument it — never a fabricated duration).
pub(super) fn startup_impact_text(entry: &taskmanager_application::StartupEntry) -> String {
    match entry.impact_evidence {
        taskmanager_application::StartupImpactEvidence::Measured { duration_ms } => {
            format!("{} · {duration_ms} ms", t(entry.impact.i18n_key()))
        }
        taskmanager_application::StartupImpactEvidence::Unknown { .. } => {
            format!(
                "{} · {}",
                t(entry.impact.i18n_key()),
                t("startup.impact_unmeasured")
            )
        }
    }
}

pub(super) fn render_users(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let feedback = session_feedback_line(app, theme);
    // One row at the bottom carries the last accepted session-control outcome
    // (GPUI's feedback_status_line parity); the table gets the rest.
    let (table_area, feedback_area) = match feedback {
        Some(_) => {
            let [table_area, feedback_area] =
                Layout::vertical([Constraint::Min(5), Constraint::Length(1)]).areas(area);
            (table_area, Some(feedback_area))
        }
        None => (area, None),
    };
    // Project the canonical order first, then materialize only the terminal
    // viewport below. The selected index remains global and is remapped to
    // the bounded table slice at the render boundary.
    let sorted_sessions = app.sorted_sessions();
    if sorted_sessions.is_empty() {
        // An empty list from a FAILED source must not read as "no sessions":
        // render the typed reason (GPUI empty_state_failure parity).
        let message = source_state_message(
            app.projection().sessions_source.as_deref(),
            t("users.no_sessions"),
            app.source_retry_request().is_some(),
        );
        render_empty_panel(
            frame,
            theme,
            table_area,
            t("users.sessions_title"),
            &message,
        );
    } else {
        let table_area = render_source_notice(
            frame,
            app,
            theme,
            table_area,
            app.projection().sessions_source.as_deref(),
        );
        let row_window = table_window(sorted_sessions.len(), app.selected, table_area);
        let rows: Vec<Row<'_>> = sorted_sessions[row_window.start..row_window.end]
            .iter()
            .map(|session| {
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
            })
            .collect();
        render_table(
            frame,
            TableRenderProps {
                theme,
                area: table_area,
                title: t("users.sessions_title"),
                rows,
                widths: [
                    Constraint::Length(8),
                    Constraint::Percentage(22),
                    Constraint::Length(10),
                    Constraint::Length(10),
                    Constraint::Length(9),
                    Constraint::Min(16),
                ],
                headers: [
                    t("users.session"),
                    t("common.user"),
                    t("users.seat"),
                    t("users.tty"),
                    t("common.type"),
                    t("users.since"),
                ],
                selected: row_window.selected,
                // The keyboard sort marks the active header column. Session →
                // index 0, User → index 1, Seat → index 2; the service-only
                // Status column never reaches a Users sort.
                sort: app.shell.sessions_sort.map(|(column, direction)| {
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
            },
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
        taskmanager_application::SessionControlAction::Disconnect => t("users.disconnect"),
        taskmanager_application::SessionControlAction::Lock => t("users.lock"),
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
