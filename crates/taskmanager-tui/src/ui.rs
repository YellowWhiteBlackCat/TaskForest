//! Ratatui renderer for the live frontend state.

mod about;
mod about_data;
mod alerts;
mod app_history;
pub(crate) mod batch_menu;
mod boot_timeline;
mod column_menu;
mod confirmations;
mod containers;
mod footer;
mod header;
mod health;
mod health_data;
pub(crate) mod help;
mod highlight;
mod pages;
mod perf_battery;
mod perf_core_grid;
mod perf_data;
mod perf_disks;
mod perf_fan;
mod perf_gpu;
mod perf_memory;
mod perf_networks;
mod perf_overview;
mod perf_overview_data;
mod process_data;
pub(crate) mod process_details;
pub(crate) mod process_menu;
pub(crate) mod process_properties;
mod process_table;
pub(crate) mod service_menu;
pub(crate) mod session_menu;
pub(crate) mod settings;
mod sparkline;
pub(crate) mod startup_menu;
pub(crate) mod table_hit;
mod units;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, HighlightSpacing, Paragraph, Row, Table, TableState, Wrap,
};
use taskmanager_application::{AppPage, i18n::t};
use taskmanager_ui_contract::IconId;

use crate::PerfDevice;
use crate::TuiApp;
use crate::TuiTheme;
use crate::icon_glyph;

use units::observed_percentage;

pub fn render(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme) {
    let area = frame.area();
    frame.render_widget(Block::new().style(Style::new().bg(theme.bg)), area);
    if area.width < 54 || area.height < 16 {
        render_too_small(frame, theme, area);
        return;
    }

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(8),
        Constraint::Length(3),
    ])
    .areas(area);
    header::render(frame, app, theme, header);
    let collecting = app.telemetry_frame_state().is_collecting();
    if collecting {
        // The shared shell has not committed a complete immutable frame yet.
        // Keep chrome stable, but do not let page-local Option fallbacks make
        // a partial platform batch look like a real first frame.
        render_loading(frame, theme, body, t("common.telemetry_warming_up"));
    } else {
        render_body(frame, app, theme, body);
    }
    footer::render(frame, app, theme, footer);

    // Modal z-order: destructive confirmations first, then the TUI-local
    // overlays, then the shared informational overlays. Only one modal is
    // open at a time by construction, so the order is defensive. During
    // warm-up the frame mask owns the body and suppresses modal content as
    // well; otherwise a modal could reveal the same partial state we hide
    // above.
    if !collecting {
        render_overlays(frame, app, theme, area);
    }
}

fn render_overlays(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    match app.input_scope() {
        crate::TuiInputScope::SharedSurface(
            taskmanager_application::SurfaceKind::Confirmation(_),
        ) => match app.shell.pending_confirmation() {
            Some(taskmanager_application::PendingConfirmation::EndTask(target)) => {
                confirmations::render_end_confirmation(
                    frame,
                    app,
                    theme,
                    target.name.as_str(),
                    target.pid,
                    area,
                );
            }
            Some(taskmanager_application::PendingConfirmation::ProcessTermination(intent)) => {
                confirmations::render_end_confirmation(
                    frame,
                    app,
                    theme,
                    intent.root.name.as_str(),
                    intent.root.pid,
                    area,
                );
            }
            Some(taskmanager_application::PendingConfirmation::ProcessBatch(intent)) => {
                confirmations::render_batch_confirmation(frame, theme, intent, area);
            }
            Some(taskmanager_application::PendingConfirmation::ServiceControl(pending)) => {
                confirmations::render_service_control_confirmation(
                    frame, app, theme, pending, area,
                );
            }
            Some(taskmanager_application::PendingConfirmation::StartupControl(pending)) => {
                confirmations::render_startup_control_confirmation(frame, theme, pending, area);
            }
            Some(taskmanager_application::PendingConfirmation::SessionControl(pending)) => {
                confirmations::render_session_control_confirmation(frame, theme, pending, area);
            }
            Some(taskmanager_application::PendingConfirmation::SmartSelfTest(pending)) => {
                confirmations::render_smart_self_test_confirmation(frame, theme, pending, area);
            }
            None => {}
        },
        crate::TuiInputScope::SharedSurface(
            taskmanager_application::SurfaceKind::ProcessProperties,
        ) => {
            if let Some(target) = app.process_properties() {
                process_properties::render_process_properties(frame, target, app, theme, area);
            }
        }
        crate::TuiInputScope::LocalSurface(_) => match app.local_surface() {
            Some(crate::TuiSurface::Settings) => {
                settings::render_settings_overlay(frame, &app.settings_form, theme, area);
            }
            Some(crate::TuiSurface::About) => {
                about::render_about_overlay(frame, app, theme, area);
            }
            Some(crate::TuiSurface::Health) => {
                health::render_health_overlay(frame, app, theme, area);
            }
            Some(crate::TuiSurface::Containers) => {
                containers::render_containers_overlay(frame, app, theme, area);
            }
            Some(crate::TuiSurface::ServiceMenu(menu)) => {
                service_menu::render_service_menu(frame, menu, theme, area);
            }
            Some(crate::TuiSurface::ProcessMenu(menu)) => {
                process_menu::render_process_menu(frame, menu, theme, area);
            }
            Some(crate::TuiSurface::BatchMenu(menu)) => {
                batch_menu::render_batch_menu(frame, menu, theme, area);
            }
            Some(crate::TuiSurface::SessionMenu(menu)) => {
                session_menu::render_session_menu(frame, menu, theme, area);
            }
            Some(crate::TuiSurface::StartupMenu(menu)) => {
                startup_menu::render_startup_menu(frame, menu, theme, area);
            }
            Some(crate::TuiSurface::ColumnMenu { .. }) => {
                column_menu::render_column_menu(frame, app, theme, area);
            }
            Some(crate::TuiSurface::CommandPalette(_)) => {
                help::render_command_palette(frame, app, theme, area);
            }
            None => {}
        },
        crate::TuiInputScope::Help => help::render_help_overlay(frame, app, theme, area),
        crate::TuiInputScope::Suggestions => {
            alerts::render_suggestions_overlay(frame, app, theme, area);
        }
        crate::TuiInputScope::ServiceLog
        | crate::TuiInputScope::Search
        | crate::TuiInputScope::DetailsPanel
        | crate::TuiInputScope::Content => {}
    }
}

fn render_body(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    match app.page() {
        AppPage::Performance => render_performance(frame, app, theme, area),
        AppPage::Applications => render_processes(frame, app, theme, area),
        AppPage::Services => pages::render_services(frame, app, theme, area),
        AppPage::System => pages::render_system(frame, app, theme, area),
        AppPage::Startup => pages::render_startup(frame, app, theme, area),
        AppPage::Users => pages::render_users(frame, app, theme, area),
        AppPage::AppHistory => app_history::render_app_history(frame, app, theme, area),
    }
}

fn render_performance(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let Some(snapshot) = app.projection().snapshot.as_ref() else {
        render_loading(frame, theme, area, t("common.collecting_telemetry"));
        return;
    };
    // The compact resource selector row sits above the selected resource's
    // detail; the area below shows ONLY that resource, reusing the existing
    // per-resource renderers (gauges + history graph for Cpu/Memory, the
    // dedicated perf_gpu/perf_disks/perf_networks panels for the device views).
    let [selector, content] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(area);
    render_perf_selector(frame, app, theme, selector);
    match app.perf_device {
        PerfDevice::Cpu | PerfDevice::Memory => {
            perf_overview::render_perf_overview(frame, app, theme, content, snapshot);
        }
        PerfDevice::Gpu => perf_gpu::render_gpu_section(frame, app, theme, content, &snapshot.gpu),
        PerfDevice::Disk => {
            // The directory-usage projection panel (render-only) rides under
            // the per-disk detail. Adaptive height: a projected snapshot needs
            // room for root + entries + totals + status; the common idle slot
            // (no scan projected yet) stays a slim 3-line panel so the disk
            // detail keeps nearly the whole content area. The data comes from
            // the SHARED `SystemProjectionStore::directory_usage` slot (latest-wins from
            // the platform batch fold).
            let usage_height: u16 = if app.projection().directory_usage.is_some() {
                12
            } else {
                3
            };
            let [disk_area, usage_area] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(usage_height)])
                    .areas(content);
            perf_disks::render_disk_section(frame, app, theme, disk_area, &snapshot.disks);
            perf_disks::render_directory_usage(frame, app, theme, usage_area);
        }
        PerfDevice::Network => {
            perf_networks::render_network_section(frame, app, theme, content, &snapshot.networks)
        }
        PerfDevice::Battery => perf_battery::render_battery_section(
            frame,
            app,
            theme,
            content,
            app.projection().power_supplies.as_ref(),
        ),
        PerfDevice::Fan => perf_fan::render_fan_section(
            frame,
            app,
            theme,
            content,
            app.projection().sensors.as_ref(),
        ),
    }
}

/// Compact resource tab row at the top of the Performance page. Each entry
/// shows its digit shortcut plus the resource label; the active resource is
/// highlighted like the page header. The shell router only binds digits with
/// Alt (the Alt+1..6 page chords), so bare digits 1..6 are free for this
/// selector and never collide with an existing chord.
fn render_perf_selector(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let mut spans = vec![Span::styled(
        format!(" {} ", t("perf.resource")),
        Style::new()
            .fg(Color::Black)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )];
    for (index, device) in PerfDevice::ALL.iter().enumerate() {
        let active = app.perf_device == *device;
        let text = format!(" {} {} ", index + 1, t(device.label_key()));
        spans.push(Span::styled(
            text,
            if active {
                Style::new()
                    .fg(Color::White)
                    .bg(theme.highlight_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme.dim)
            },
        ));
    }
    spans.push(Span::styled(
        format!(" {}", t("perf.select_range")),
        Style::new().fg(theme.dim),
    ));
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .block(
                Block::new()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::new().fg(theme.dim)),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_processes(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    process_table::render_processes(frame, app, theme, area);
}

/// A typed table with an honest empty state instead of a bare header.
pub(super) fn render_empty_panel(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    area: Rect,
    title: &str,
    message: &str,
) {
    render_centered_state(frame, theme, area, title, IconId::CircleX, message);
}

/// Render a small, centered state treatment inside a bordered panel. Keeping
/// the icon and message in the same helper makes loading, empty and unavailable
/// surfaces read as one product language instead of unrelated bare paragraphs.
pub(super) fn render_centered_state(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    area: Rect,
    title: &str,
    icon: IconId,
    message: &str,
) {
    let block = panel(title, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let state_height = inner.height.min(2);
    let state_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(state_height) / 2,
        width: inner.width,
        height: state_height,
    };
    let lines = if state_area.height >= 2 {
        vec![
            Line::from(Span::styled(
                format!("  {}  ", icon_glyph(icon)),
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(message, Style::new().fg(theme.dim))),
        ]
    } else {
        // A three-row panel has only one inner line. Preserve the state text
        // there instead of rendering only the icon and silently dropping the
        // diagnostic the user needs.
        vec![Line::from(vec![
            Span::styled(
                format!("{} ", icon_glyph(icon)),
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(message, Style::new().fg(theme.dim)),
        ])]
    };
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        state_area,
    );
}

/// A bounded slice of a table's canonical row order plus the selected index
/// relative to that slice. Ratatui will clip a Table at paint time, but row
/// and cell construction happens before that; keeping this window at the
/// renderer boundary prevents a 10k-row table from materializing 10k widgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TableWindow {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) selected: usize,
}

/// Compute the row window for a bordered table with one header row and the
/// shared header bottom margin. The selected row follows the viewport so
/// keyboard navigation remains global/canonical while only the local slice is
/// handed to Ratatui.
#[must_use]
pub(super) fn table_window(total: usize, selected: usize, area: Rect) -> TableWindow {
    if total == 0 {
        return TableWindow {
            start: 0,
            end: 0,
            selected: 0,
        };
    }
    let body_rows = usize::from(area.height.saturating_sub(4)).max(1);
    let visible = body_rows.min(total);
    let selected = selected.min(total - 1);
    let start = selected
        .saturating_sub(visible / 2)
        .min(total.saturating_sub(visible));
    TableWindow {
        start,
        end: start + visible,
        selected: selected - start,
    }
}

/// Complete geometry and interaction projection for a table render. Callers
/// name every axis so width/header/selection values cannot drift through a
/// positional argument list.
pub(super) struct TableRenderProps<'a, const WIDTHS: usize, const HEADERS: usize> {
    pub(super) theme: TuiTheme,
    pub(super) area: Rect,
    pub(super) title: &'a str,
    pub(super) rows: Vec<Row<'a>>,
    pub(super) widths: [Constraint; WIDTHS],
    pub(super) headers: [&'a str; HEADERS],
    pub(super) selected: usize,
    pub(super) sort: Option<(usize, taskmanager_shell::SortDir)>,
}

pub(super) fn render_table<'a, const WIDTHS: usize, const HEADERS: usize>(
    frame: &mut Frame<'_>,
    props: TableRenderProps<'a, WIDTHS, HEADERS>,
) {
    let TableRenderProps {
        theme,
        area,
        title,
        rows,
        widths,
        headers,
        selected,
        sort,
    } = props;
    let table = Table::new(rows, widths)
        .header(header_row(headers, theme.accent, sort))
        .row_highlight_style(Style::new().bg(theme.highlight_bg).fg(Color::White))
        // Same two-blank gutter as the process table — column separation is a
        // product-wide readability rule, not a per-table choice.
        .column_spacing(2)
        .highlight_symbol("› ")
        .highlight_spacing(HighlightSpacing::Always)
        .block(panel(title, theme));
    let mut state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, area, &mut state);
}

fn header_row<'a, const N: usize>(
    headers: [&'a str; N],
    accent: Color,
    sort: Option<(usize, taskmanager_shell::SortDir)>,
) -> Row<'a> {
    let cells: Vec<Cell> = headers
        .into_iter()
        .enumerate()
        .map(|(index, header)| {
            let mut text = header.to_owned();
            let mut style = Style::new().fg(accent).add_modifier(Modifier::BOLD);
            if let Some((column, direction)) = sort
                && index == column
            {
                text.push_str(match direction {
                    taskmanager_shell::SortDir::Asc => " ▲",
                    taskmanager_shell::SortDir::Desc => " ▼",
                });
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            Cell::from(text).style(style)
        })
        .collect();
    Row::new(cells).bottom_margin(1)
}

pub(super) fn panel(title: &str, theme: TuiTheme) -> Block<'_> {
    Block::new()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.border))
}

fn render_loading(frame: &mut Frame<'_>, theme: TuiTheme, area: Rect, message: &str) {
    render_centered_state(
        frame,
        theme,
        area,
        t("common.loading"),
        IconId::Refresh,
        message,
    );
}

fn render_too_small(frame: &mut Frame<'_>, theme: TuiTheme, area: Rect) {
    frame.render_widget(
        Paragraph::new(format!(
            "{} TUI needs at least 54×16\n{}",
            taskmanager_assets::product::NAME,
            t("empty.terminal_resize_hint")
        ))
        .alignment(Alignment::Center)
        .block(panel(t("empty.terminal_too_small"), theme)),
        area,
    );
}

/// Key/value row shared by the System page and the about overlay.
pub(super) fn kv<'a>(label: &'a str, value: impl Into<String>, theme: TuiTheme) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label:<18} "), Style::new().fg(theme.dim)),
        Span::styled(value.into(), Style::new().fg(Color::White)),
    ])
}

/// Coarse device-health verdict used by the containers and health overlays.
///
/// The typed `DeviceStatus` enum lives behind the dependency firewall, so the
/// TUI classifies it through exported constructors/methods only: the healthy
/// reference state comes from `DeviceState::healthy`, and every non-healthy
/// status maps to an exported `FailureKind` through `DeviceStatus::failure()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeviceHealth {
    Healthy,
    Stale,
    PermissionDenied,
    MissingTool,
    Unsupported,
}

pub(crate) fn classify_device_state(state: &taskmanager_application::DeviceState) -> DeviceHealth {
    use taskmanager_application::{DeviceState, FailureKind};
    if state.status == DeviceState::healthy(0).status {
        return DeviceHealth::Healthy;
    }
    match state.status.failure() {
        Some(FailureKind::TemporarilyUnavailable) => DeviceHealth::Stale,
        Some(FailureKind::PermissionDenied) => DeviceHealth::PermissionDenied,
        Some(FailureKind::MissingDependency) => DeviceHealth::MissingTool,
        Some(FailureKind::Unsupported) => DeviceHealth::Unsupported,
        // No other failure kind can produce a device status; be honest about
        // an unexpected payload rather than inventing a verdict.
        _ => DeviceHealth::Unsupported,
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

/// Shared test-only support for the TUI render tests (the i18n guard).
#[cfg(test)]
#[path = "../tests/gui/ui/test_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "../tests/gui/ui/tests.rs"]
mod tests;
