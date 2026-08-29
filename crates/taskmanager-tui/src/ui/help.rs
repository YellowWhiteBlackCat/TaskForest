//! Keyboard-help overlay rendered on top of the live TUI frame.
//!
//! The content is derived from the conflict-checked shared command router
//! (see [`taskmanager_shell::presentation::command_help`]) so the overlay can never advertise a binding
//! that no frontend actually wires. One shared entry is intentionally dropped
//! and the terminal-only bindings are appended — see [`help_rows`] for the
//! honesty rationale.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;

use taskmanager_shell::LocalBinding;

use super::containers::{KeyHint, KeyHintTone, Modal};
use crate::{TuiApp, TuiTheme};

use crate::command_palette::TUI_LOCAL_COMMANDS;

/// The honest, TUI-verified keyboard reference.
///
/// Shared commands come straight from the router table
/// ([`taskmanager_shell::presentation::command_help`]). Shared commands explicitly absent from the TUI
/// binding surface are filtered out because the terminal must never advertise
/// a chord it does not execute: confirmation uses `y` / `n` / `Esc`, the
/// terminal has a resource selector instead of a sidebar, and Alerts management
/// is not implemented yet. The terminal-only bindings (declared in [`taskmanager_shell::shell_local_bindings`] and
/// handled in `runtime::handle_key`) are appended instead, together with the
/// TUI-local overlay bindings above (localized through
/// [`localize_tui_binding`]).
#[must_use]
pub fn help_rows() -> Vec<LocalBinding> {
    let mut rows: Vec<LocalBinding> = taskmanager_shell::presentation::command_help()
        .into_iter()
        .filter(|help| !crate::bindings::is_deliberately_unbound(help.command))
        .map(|help| LocalBinding {
            shortcut: help.shortcut,
            label: help.label,
        })
        .collect();
    rows.extend(taskmanager_shell::shell_local_bindings().iter().copied());
    rows.extend(
        TUI_LOCAL_COMMANDS
            .into_iter()
            .map(|command| localize_tui_binding(command.binding)),
    );
    rows
}

/// Resolve a TUI-local overlay binding's label through the shared i18n
/// catalog. The registry stays English-labeled because the command palette
/// and help need one stable row inventory; this fold is the single shortcut →
/// localized-label mapping, so the overlay and the registry can never drift.
/// Reuses existing catalog keys where the copy already existed
/// (`chrome.settings`, `health.system_health_alerts`, `containers.title`,
/// `system.export_snapshot`). The shared router's own labels stay English
/// until the shell migrates them; an unknown shortcut keeps its const label.
fn localize_tui_binding(binding: LocalBinding) -> LocalBinding {
    let key = match binding.shortcut {
        "p" => "chrome.settings",
        "i" => "help.binding.about",
        "h" => "health.system_health_alerts",
        "c" => "containers.title",
        "x" => "system.export_snapshot",
        "Enter" => "help.binding.service_actions",
        "1-7" => "help.binding.perf_resource",
        "C" => "help.binding.columns",
        "m" => "help.binding.mark_batch",
        "B" => "help.binding.batch_actions",
        "y" => "help.binding.copy_clipboard",
        "a" => "help.binding.process_actions",
        "o" => "help.binding.service_logs",
        "e" => "help.binding.gpu_engines_escalate",
        "d" => "help.binding.directory_scan",
        "g" => "help.binding.gpu_chart_metric",
        _ => return binding,
    };
    LocalBinding {
        shortcut: binding.shortcut,
        label: t(key),
    }
}

/// Render the help overlay centred over `area`. Does nothing if the terminal
/// is too small for a readable box. The binding list is laid out as two
/// side-by-side columns and sliced by [`TuiApp::help_scroll`], so a short
/// terminal (or a growing binding list) scrolls instead of clipping the tail.
#[cfg(test)]
#[allow(dead_code)]
pub fn render_help_overlay(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    render_help_overlay_at(
        frame,
        app,
        theme,
        super::planned_popup(area, crate::TuiInputScope::Help),
    );
}

pub(super) fn render_help_overlay_at(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    popup: Rect,
) {
    let rows = help_rows();
    let inner =
        Modal::new(theme, IconId::Settings, t("menu.keyboard_reference")).render(frame, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(6), Constraint::Length(2)]).areas(inner);

    // Two side-by-side columns keep the listing inside a modest popup height.
    // Each column shows `body.height` rows; the scroll offset walks both
    // columns together (row N of the listing lives in the left column when
    // N < the column height, otherwise in the right).
    let half = rows.len().div_ceil(2);
    let (left, right) = rows.split_at(half);
    let column_height = usize::from(body.height);
    let offset = app.help_scroll.min(half.saturating_sub(column_height));
    let left_visible = &left[offset.min(left.len())..left.len().min(offset + column_height)];
    let right_visible = &right[offset.min(right.len())..right.len().min(offset + column_height)];
    let [left_col, right_col] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(body);
    frame.render_widget(
        Paragraph::new(
            left_visible
                .iter()
                .map(|row| help_line(row, theme))
                .collect::<Vec<_>>(),
        )
        .wrap(Wrap { trim: true }),
        left_col,
    );
    frame.render_widget(
        Paragraph::new(
            right_visible
                .iter()
                .map(|row| help_line(row, theme))
                .collect::<Vec<_>>(),
        )
        .wrap(Wrap { trim: true }),
        right_col,
    );

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            KeyHint::line(
                theme,
                vec![(" F1 / ? / Esc ", format!("  {}", t("help.close_hint")))],
            ),
        ])
        .alignment(Alignment::Center),
        footer,
    );
}

fn help_line(row: &LocalBinding, theme: TuiTheme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            super::text::pad_cells(row.shortcut, 10),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            row.label.to_owned(),
            Style::new().fg(theme.color(Color::White)),
        ),
    ])
}

/// Render the searchable command palette: a filter field over the keyboard
/// reference rows. Typing narrows the list; Enter runs the selected row's
/// shared action (when it has one); Esc closes. The palette is the keyboard
/// user's command entry point — the same rows the static help shows, but
/// filterable and (for shared commands) executable. Takes the crate's
/// [`crate::TuiApp`] (not the shell alias) because the palette state is
/// TUI-local (ADR-027).
pub(super) fn render_command_palette_at(
    frame: &mut Frame<'_>,
    app: &crate::TuiApp,
    theme: TuiTheme,
    focus: super::TuiFocusPlan,
    popup: Rect,
) {
    let rows = app.filtered_palette_rows();
    let inner =
        Modal::new(theme, IconId::Search, t("menu.keyboard_reference")).render(frame, popup);

    let filter = app
        .command_palette()
        .map_or("", |palette| palette.filter.as_str());
    let [filter_area, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ▸ ", Style::new().fg(theme.accent)),
            Span::styled(
                format!("{filter}▌"),
                Style::new()
                    .fg(theme.color(Color::White))
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .style(Style::new().fg(theme.dim)),
        filter_area,
    );

    let selection = focus.palette_item();
    let lines: Vec<Line<'static>> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let active = selection == Some(index);
            let shortcut = super::text::pad_cells(row.shortcut, 10);
            let executable = row.action.is_some();
            let mut spans = vec![
                Span::styled(
                    shortcut,
                    Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    row.label.to_owned(),
                    if executable {
                        Style::new().fg(theme.color(Color::White))
                    } else {
                        Style::new().fg(theme.dim)
                    },
                ),
            ];
            if active {
                spans.insert(0, Span::styled("› ", Style::new().fg(theme.accent)));
                Line::from(spans).style(Style::new().bg(theme.highlight_bg))
            } else {
                spans.insert(0, Span::styled("  ", Style::new().fg(theme.dim)));
                Line::from(spans)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), body);

    frame.render_widget(
        Paragraph::new(vec![KeyHint::line_toned(
            theme,
            vec![(
                KeyHintTone::Accent,
                format!(" {} ", t("help.palette_type")),
                format!(" {}", t("help.palette_footer")),
            )],
        )])
        .alignment(Alignment::Center),
        footer,
    );
}

#[cfg(test)]
#[path = "../../tests/gui/ui/help_tests.rs"]
mod tests;
