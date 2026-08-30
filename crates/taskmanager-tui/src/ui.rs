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
mod frame_plan;
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
mod perf_selector_instances;
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
mod text;
mod units;

pub(crate) use frame_plan::{
    TablePanelProjection, TableWindow, TuiFocusPlan, TuiFramePlan, TuiHitTarget, TuiPageLayout,
    table_window,
};

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, HighlightSpacing, Paragraph, Row, Table, TableState, Wrap,
};
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;

use crate::PerfDevice;
use crate::TuiApp;
use crate::TuiTheme;

use units::{memory_text_pref, observed_percentage};

use perf_selector_instances::perf_selector_instances;

pub fn render(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme) {
    let plan = TuiFramePlan::build(app, frame.area());
    render_with_plan(frame, app, theme, &plan);
}

/// Render a frame from a caller-supplied immutable plan. The runtime uses this
/// entry after building the plan that becomes the committed pointer/focus
/// geometry; the public wrapper above keeps existing headless callers simple.
pub(crate) fn render_with_plan(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    plan: &TuiFramePlan,
) {
    let area = plan.area;
    debug_assert_eq!(frame.area(), area, "frame plan must match the painted area");
    debug_assert_eq!(
        plan.focus,
        TuiFocusPlan::build(app, plan.input_scope),
        "frame plan focus must match its input scope"
    );
    frame.render_widget(Block::new().style(Style::new().bg(theme.bg)), area);
    if area.width < 54 || area.height < 16 {
        render_too_small(frame, theme, area);
        sanitize_ascii_cells(frame, theme);
        return;
    }

    let chrome = plan.chrome;
    header::render(frame, app, theme, chrome.header);
    let collecting = app.telemetry_frame_state().is_collecting();
    if collecting {
        // The shared shell has not committed a complete immutable frame yet.
        // Keep chrome stable, but do not let page-local Option fallbacks make
        // a partial platform batch look like a real first frame.
        render_loading(frame, theme, chrome.body, t("common.telemetry_warming_up"));
    } else {
        render_body(frame, app, theme, plan);
    }
    footer::render(frame, app, theme, chrome.footer);

    // Modal z-order: destructive confirmations first, then the TUI-local
    // overlays, then the shared informational overlays. Only one modal is
    // open at a time by construction, so the order is defensive. During
    // warm-up the frame mask owns the body and suppresses modal content as
    // well; otherwise a modal could reveal the same partial state we hide
    // above.
    if !collecting {
        render_overlays(frame, app, theme, plan);
    }
    sanitize_ascii_cells(frame, theme);
}

/// Apply the terminal profile after every widget has painted.  Ratatui's
/// border symbols and shared missing-value text do not all pass through the
/// semantic icon helper, so the final cell pass is the only complete guard for
/// an ASCII-only terminal. Each replacement is one cell and therefore cannot
/// invalidate the already-committed frame geometry.
fn sanitize_ascii_cells(frame: &mut Frame<'_>, theme: TuiTheme) {
    if theme.terminal.glyphs != crate::TuiGlyphMode::Ascii {
        return;
    }
    for cell in &mut frame.buffer_mut().content {
        let symbol = cell.symbol();
        if !symbol.is_ascii() {
            cell.set_symbol(crate::TuiTerminalProfile::ascii_cell_symbol(symbol));
        }
    }
}

fn render_overlays(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, plan: &TuiFramePlan) {
    let Some(overlay) = plan.overlay() else {
        return;
    };
    debug_assert_eq!(overlay.scope, plan.input_scope);
    let input_scope = overlay.scope;
    let popup = overlay.popup;
    match input_scope {
        crate::TuiInputScope::SharedSurface(
            taskmanager_application::SurfaceKind::Confirmation(_),
        ) => match app.shell.pending_confirmation() {
            Some(taskmanager_application::PendingConfirmation::EndTask(target)) => {
                confirmations::render_end_confirmation_at(
                    frame,
                    app,
                    theme,
                    target.name.as_str(),
                    target.pid,
                    popup,
                );
            }
            Some(taskmanager_application::PendingConfirmation::ProcessTermination(intent)) => {
                confirmations::render_end_confirmation_at(
                    frame,
                    app,
                    theme,
                    intent.root.name.as_str(),
                    intent.root.pid,
                    popup,
                );
            }
            Some(taskmanager_application::PendingConfirmation::ProcessBatch(intent)) => {
                confirmations::render_batch_confirmation_at(frame, theme, intent, popup);
            }
            Some(taskmanager_application::PendingConfirmation::ServiceControl(pending)) => {
                confirmations::render_service_control_confirmation_at(
                    frame, app, theme, pending, popup,
                );
            }
            Some(taskmanager_application::PendingConfirmation::StartupControl(pending)) => {
                confirmations::render_startup_control_confirmation_at(frame, theme, pending, popup);
            }
            Some(taskmanager_application::PendingConfirmation::SessionControl(pending)) => {
                confirmations::render_session_control_confirmation_at(frame, theme, pending, popup);
            }
            Some(taskmanager_application::PendingConfirmation::SmartSelfTest(pending)) => {
                confirmations::render_smart_self_test_confirmation_at(frame, theme, pending, popup);
            }
            None => {}
        },
        crate::TuiInputScope::SharedSurface(
            taskmanager_application::SurfaceKind::ProcessProperties,
        ) => {
            if let Some(target) = app.process_properties() {
                process_properties::render_process_properties_at(
                    frame, target, app, theme, plan.focus, popup,
                );
            }
        }
        crate::TuiInputScope::LocalSurface(_) => match app.local_surface() {
            Some(crate::TuiSurface::Settings) => {
                settings::render_settings_overlay_at(
                    frame,
                    &app.settings_form,
                    theme,
                    plan.focus,
                    popup,
                );
            }
            Some(crate::TuiSurface::About) => {
                about::render_about_overlay_at(frame, app, theme, popup);
            }
            Some(crate::TuiSurface::Health) => {
                health::render_health_overlay_at(frame, app, theme, popup);
            }
            Some(crate::TuiSurface::Containers) => {
                containers::render_containers_overlay_at(frame, app, theme, popup);
            }
            Some(crate::TuiSurface::ServiceMenu(menu)) => {
                service_menu::render_service_menu_at(frame, menu, theme, plan.focus, popup);
            }
            Some(crate::TuiSurface::ProcessMenu(menu)) => {
                process_menu::render_process_menu_at(frame, menu, theme, plan.focus, popup);
            }
            Some(crate::TuiSurface::BatchMenu(menu)) => {
                batch_menu::render_batch_menu_at(frame, menu, theme, plan.focus, popup);
            }
            Some(crate::TuiSurface::SessionMenu(menu)) => {
                session_menu::render_session_menu_at(frame, menu, theme, plan.focus, popup);
            }
            Some(crate::TuiSurface::StartupMenu(menu)) => {
                startup_menu::render_startup_menu_at(frame, menu, theme, plan.focus, popup);
            }
            Some(crate::TuiSurface::ColumnMenu { .. }) => {
                column_menu::render_column_menu_at(frame, app, theme, plan.focus, popup);
            }
            Some(crate::TuiSurface::CommandPalette(_)) => {
                help::render_command_palette_at(frame, app, theme, plan.focus, popup);
            }
            None => {}
        },
        crate::TuiInputScope::Help => help::render_help_overlay_at(frame, app, theme, popup),
        crate::TuiInputScope::Suggestions => {
            alerts::render_suggestions_overlay_at(frame, app, theme, popup);
        }
        crate::TuiInputScope::ServiceLog
        | crate::TuiInputScope::Search
        | crate::TuiInputScope::DetailsPanel
        | crate::TuiInputScope::Content => {}
    }
}

fn render_body(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, plan: &TuiFramePlan) {
    match plan.page {
        TuiPageLayout::Performance { .. } => render_performance(frame, app, theme, plan),
        TuiPageLayout::Applications { process, table } => {
            process_table::render_processes(frame, app, theme, process, table, plan.focus)
        }
        TuiPageLayout::Services { page, table } => {
            pages::render_services(frame, app, theme, page, table)
        }
        TuiPageLayout::System { content } => pages::render_system(frame, app, theme, content),
        TuiPageLayout::Startup { page, table } => {
            pages::render_startup(frame, app, theme, page, table)
        }
        TuiPageLayout::Users { page, table } => pages::render_users(frame, app, theme, page, table),
        TuiPageLayout::AppHistory { content } => {
            app_history::render_app_history(frame, app, theme, content)
        }
    }
}

fn render_performance(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, plan: &TuiFramePlan) {
    let TuiPageLayout::Performance { selector, content } = plan.page else {
        return;
    };
    let Some(snapshot) = app.projection().snapshot.as_ref() else {
        render_loading(frame, theme, content, t("common.collecting_telemetry"));
        return;
    };
    // The compact resource selector row sits above the selected resource's
    // detail; the area below shows ONLY that resource, reusing the existing
    // per-resource renderers (gauges + history graph for Cpu/Memory, the
    // dedicated perf_gpu/perf_disks/perf_networks panels for the device views).
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
///
/// Below the tab row (same reserved band, wide terminals only) the selector
/// paints a live per-instance strip for the ACTIVE resource: one segment per
/// device (disk / NIC / GPU / battery / fan channel; CPU and memory are
/// singletons), each carrying the family icon, an inline history sparkline
/// and the same typed caption fields the gpui device sidebar reads. The
/// strip is presentation-only — the digit keys still select resource
/// classes, and no selection semantics are added. When the tab row wraps
/// (narrow tier) or the active class has no instances, the strip collapses
/// honestly and the wrapped tab row keeps the band.
fn render_perf_selector(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let spans = selector_tab_spans(app, theme);
    let block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(Style::new().fg(theme.dim));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let tab_width: usize = spans
        .iter()
        .map(|span| text::cell_width(span.content.as_ref()))
        .sum();
    let instances = perf_selector_instances(app, theme);
    if inner.height < 2 || tab_width > usize::from(inner.width) || instances.is_empty() {
        // Narrow tier (the tab row wraps onto the whole band) or a resource
        // with no live instances: the tab row fills the band and the strip
        // collapses. Byte-for-byte the historical wrapped render.
        frame.render_widget(
            Paragraph::new(Line::from(spans)).wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    let [tabs_area, strip_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
    frame.render_widget(Paragraph::new(Line::from(spans)), tabs_area);
    let strip = selector_strip_line(instances, usize::from(inner.width));
    if !strip.is_empty() {
        frame.render_widget(Paragraph::new(Line::from(strip)), strip_area);
    }
}

/// The selector's resource-class tab row: the accent chip, one tab per
/// resource with its digit shortcut, and the digit-range hint.
fn selector_tab_spans(app: &TuiApp, theme: TuiTheme) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(
        format!(" {} ", t("perf.resource")),
        Style::new()
            .fg(theme.color(Color::Black))
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
                    .fg(theme.color(Color::White))
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
    spans
}

/// The gap painted between two live instance segments of the strip.
const SELECTOR_SEGMENT_SEPARATOR: &str = "  │  ";

/// The separator's cell width, kept in lockstep with the text above.
const SELECTOR_SEGMENT_SEPARATOR_WIDTH: usize = 5;

/// Heading width cap, so a long vendor string cannot starve the caption on
/// the one strip line the reserved selector band offers.
const SELECTOR_HEADING_MAX_CELLS: usize = 22;

/// One live per-device instance segment of the selector strip, pre-measured
/// so the strip can admit whole segments only and never paint past its band.
struct SelectorInstance {
    width: usize,
    spans: Vec<Span<'static>>,
}

/// Build one strip segment: family icon, truncated heading, the device's own
/// generation-scoped history sparkline, and the live caption.
fn selector_instance(
    icon: IconId,
    heading: &str,
    trend: &str,
    caption: String,
    theme: TuiTheme,
) -> SelectorInstance {
    let heading = text::truncate_cells(heading, SELECTOR_HEADING_MAX_CELLS);
    let spans = vec![
        Span::styled(
            format!(" {} ", theme.glyph(icon)),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(heading, Style::new().fg(theme.color(Color::White))),
        Span::raw(" "),
        Span::styled(trend.to_owned(), Style::new().fg(theme.accent)),
        Span::raw(" "),
        Span::styled(caption, Style::new().fg(theme.dim)),
    ];
    let width = spans
        .iter()
        .map(|span| text::cell_width(span.content.as_ref()))
        .sum();
    SelectorInstance { width, spans }
}

/// Flatten whole instance segments onto one strip line: a segment is admitted
/// only when it fits entirely, so a crowded class degrades by dropping its
/// trailing instances (the resource detail below still lists every one)
/// instead of painting a half-clipped value.
fn selector_strip_line(instances: Vec<SelectorInstance>, width: usize) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for (index, instance) in instances.into_iter().enumerate() {
        let separator = if index == 0 {
            0
        } else {
            SELECTOR_SEGMENT_SEPARATOR_WIDTH
        };
        if used + separator + instance.width > width {
            break;
        }
        if index > 0 {
            spans.push(Span::styled(
                SELECTOR_SEGMENT_SEPARATOR,
                Style::new().fg(Color::DarkGray),
            ));
        }
        used += separator + instance.width;
        spans.extend(instance.spans);
    }
    spans
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
                format!("  {}  ", theme.glyph(icon)),
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
                format!("{} ", theme.glyph(icon)),
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
        .header(header_row(
            headers,
            theme.accent,
            theme.color(Color::White),
            sort,
        ))
        .row_highlight_style(
            Style::new()
                .bg(theme.highlight_bg)
                .fg(theme.color(Color::White)),
        )
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
    text_color: Color,
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
                style = Style::new()
                    .fg(text_color)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
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
        Span::styled(
            format!("{} ", text::pad_cells(label, 18)),
            Style::new().fg(theme.dim),
        ),
        Span::styled(value.into(), Style::new().fg(theme.color(Color::White))),
    ])
}

/// Coarse device-health verdict used by the containers and health overlays.
///
/// The TUI classifies the typed `DeviceStatus` directly through its core
/// accessors: the healthy reference state comes from `DeviceState::healthy`,
/// and every non-healthy status maps to `FailureKind` through
/// `DeviceStatus::failure()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeviceHealth {
    Healthy,
    Stale,
    PermissionDenied,
    MissingTool,
    Unsupported,
}

pub(crate) fn classify_device_state(
    state: &taskmanager_core::core::device_state::DeviceState,
) -> DeviceHealth {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::failure::FailureKind;
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

/// Shared test-only support for the TUI render tests (the i18n guard).
#[cfg(test)]
#[path = "../tests/gui/ui/test_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "../tests/gui/ui/tests.rs"]
mod tests;
