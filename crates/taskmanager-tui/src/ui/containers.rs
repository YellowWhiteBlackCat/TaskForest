//! Containers overlay: the aggregated cgroup-v2 rollup.
//!
//! The rollup is consumed from the renderer-independent `SystemProjectionStore::containers`
//! projection. The TUI does not mirror or mutate the platform event; the shell
//! applies the correlated event once and owns the refresh truth.
//!
//! Honesty contract (mirrors `ContainerRollup`): an `Unsupported` /
//! `PermissionDenied` state renders its typed marker, a healthy host with no
//! containers renders "no containers running", and per-container readings
//! that are unavailable render as `—`, never a fabricated zero.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table};
use taskmanager_application::i18n::t;
use taskmanager_application::{ContainerRollup, IsolationKind, container_row_window};
use taskmanager_shell::presentation::{bytes, missing_value};
use taskmanager_ui_contract::IconId;

use crate::TuiApp;
use crate::TuiTheme;
use crate::ui::{DeviceHealth, classify_device_state};

/// Render the containers overlay centred over `area`.
pub fn render_containers_overlay(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let popup = centered(area, 84, 22);
    frame.render_widget(Clear, popup);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.accent))
        .style(Style::new().bg(theme.overlay_bg))
        .title(format!(
            " {} {} ",
            crate::icon_glyph(IconId::Process),
            t("containers.title")
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(8), Constraint::Length(3)]).areas(inner);

    match app.shell.projection().containers.as_ref() {
        None => {
            let lines = vec![
                Line::from(Span::styled(
                    t("containers.telemetry_not_collected"),
                    Style::new().fg(theme.dim),
                )),
                Line::from(Span::styled(
                    t("containers.accrual_hint"),
                    Style::new().fg(theme.dim),
                )),
            ];
            frame.render_widget(Paragraph::new(lines), body);
        }
        Some(rollup) => {
            render_rollup(frame, rollup, theme, body);
        }
    }

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(" c / Esc ", Style::new().fg(Color::Black).bg(theme.accent)),
                Span::styled(
                    format!("  {}", t("chrome.close")),
                    Style::new().fg(theme.dim),
                ),
            ]),
        ])
        .alignment(Alignment::Center),
        footer,
    );
}

fn render_rollup(frame: &mut Frame<'_>, rollup: &ContainerRollup, theme: TuiTheme, area: Rect) {
    let state_line = state_line(rollup, theme);
    let [state, table_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(4)]).areas(area);
    frame.render_widget(Paragraph::new(state_line), state);

    if rollup.containers.is_empty() {
        let message = match classify_device_state(&rollup.state) {
            DeviceHealth::Healthy => t("containers.none_running"),
            _ => t("containers.none_listed"),
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(message, Style::new().fg(theme.dim))),
            ]),
            table_area,
        );
        return;
    }

    let (shown, hidden) = container_row_window(rollup.containers.len());
    let mut rows: Vec<Row<'_>> = rollup.containers[..shown]
        .iter()
        .map(|container| {
            Row::new([
                Cell::from(container.name.as_str()),
                Cell::from(runtime_label(container.runtime.as_ref(), theme)),
                Cell::from(
                    container
                        .cpu_percentage
                        .current_value()
                        .copied()
                        .map_or_else(missing_value, |value| format!("{value:>6.1}%")),
                ),
                Cell::from(
                    container
                        .memory_bytes
                        .current_value()
                        .copied()
                        .map_or_else(missing_value, bytes),
                ),
                Cell::from(t("containers.pids_count").replacen(
                    "{}",
                    &container.member_pids.len().to_string(),
                    1,
                )),
            ])
        })
        .collect();
    if hidden > 0 {
        rows.push(Row::new([
            Cell::from(more_rows_label(hidden)),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ]));
    }
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(28),
            Constraint::Length(18),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(10),
        ],
    )
    // Same two-blank gutter as the process/services tables — column
    // separation is a product-wide readability rule.
    .column_spacing(2)
    .header(
        Row::new([
            t("containers.name"),
            t("containers.runtime"),
            t("containers.cpu"),
            t("containers.memory"),
            t("containers.members"),
        ])
        .style(Style::new().fg(theme.accent).add_modifier(Modifier::BOLD))
        .bottom_margin(1),
    )
    .row_highlight_style(Style::new().bg(theme.highlight_bg).fg(Color::White));
    frame.render_widget(table, table_area);
}

fn state_line(rollup: &ContainerRollup, theme: TuiTheme) -> Line<'static> {
    let (label, color) = match classify_device_state(&rollup.state) {
        DeviceHealth::Healthy => (
            t("containers.source_healthy").replacen("{}", &rollup.containers.len().to_string(), 1),
            theme.good,
        ),
        DeviceHealth::Stale => (t("containers.source_stale").to_owned(), theme.warn),
        DeviceHealth::PermissionDenied => (
            t("containers.source_permission_denied").to_owned(),
            theme.danger,
        ),
        DeviceHealth::MissingTool => (t("containers.source_missing_tool").to_owned(), theme.warn),
        DeviceHealth::Unsupported => (t("containers.source_unsupported").to_owned(), theme.dim),
    };
    Line::from(Span::styled(
        format!("{} {label}", crate::icon_glyph(IconId::Process)),
        Style::new().fg(color),
    ))
}

fn runtime_label(runtime: Option<&IsolationKind>, theme: TuiTheme) -> Span<'static> {
    match runtime {
        Some(kind) => Span::styled(kind_label(kind), Style::new().fg(theme.dim)),
        None => Span::styled(t("containers.unknown_runtime"), Style::new().fg(theme.warn)),
    }
}

/// TUI-local label for the typed runtime family (the shared layer has no
/// presentation mapping for this enum).
fn kind_label(kind: &IsolationKind) -> &'static str {
    match kind {
        IsolationKind::Docker => "docker",
        IsolationKind::Podman => "podman",
        IsolationKind::Kubernetes => "k8s",
        IsolationKind::Lxc => "lxc",
        IsolationKind::SystemdNspawn => "systemd-nspawn",
        IsolationKind::Flatpak => "flatpak",
        IsolationKind::Snap => "snap",
        IsolationKind::Wsl => "wsl",
        IsolationKind::OtherContainer => "container",
    }
}

fn more_rows_label(hidden: usize) -> String {
    t("common.more_rows").replace("{count}", &hidden.to_string())
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

#[cfg(test)]
#[path = "../../tests/gui/ui/containers_tests.rs"]
mod tests;
