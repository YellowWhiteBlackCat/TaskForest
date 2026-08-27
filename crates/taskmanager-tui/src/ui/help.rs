//! Keyboard-help overlay rendered on top of the live TUI frame.
//!
//! The content is derived from the conflict-checked shared command router
//! (see [`crate::command_help`]) so the overlay can never advertise a binding
//! that no frontend actually wires. One shared entry is intentionally dropped
//! and the terminal-only bindings are appended — see [`help_rows`] for the
//! honesty rationale.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use taskmanager_application::CommandId;
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;

use taskmanager_shell::LocalBinding;

use crate::{TuiApp, TuiTheme};

/// TUI-local overlay bindings handled directly in `runtime::handle_key`.
/// Documented beside the shared router bindings so the overlay advertises
/// every genuinely-wired chord. `pub(crate)` so the command palette can list
/// the same rows.
pub(crate) const TUI_LOCAL_BINDINGS: [LocalBinding; 15] = [
    LocalBinding {
        shortcut: "p",
        label: "Settings",
    },
    LocalBinding {
        shortcut: "i",
        label: "About / system info",
    },
    LocalBinding {
        shortcut: "h",
        label: "System health & alerts",
    },
    LocalBinding {
        shortcut: "c",
        label: "Containers",
    },
    LocalBinding {
        shortcut: "x",
        label: "Export snapshot",
    },
    LocalBinding {
        shortcut: "Enter",
        label: "Service actions (Services page)",
    },
    LocalBinding {
        shortcut: "1-7",
        label: "Performance resource (Performance page)",
    },
    LocalBinding {
        shortcut: "C",
        label: "Columns (Applications page)",
    },
    LocalBinding {
        shortcut: "m",
        label: "Mark process for batch control (Applications page)",
    },
    LocalBinding {
        shortcut: "B",
        label: "Batch actions on marked processes (Applications page)",
    },
    LocalBinding {
        shortcut: "y",
        label: "Copy selected pid+name to clipboard (Applications page)",
    },
    LocalBinding {
        shortcut: "a",
        label: "Process actions · open location / search (Applications page)",
    },
    LocalBinding {
        shortcut: "o",
        label: "Service logs (Services)",
    },
    LocalBinding {
        shortcut: "e",
        label: "GPU engines (Performance·GPU) · network escalate (process)",
    },
    LocalBinding {
        shortcut: "d",
        label: "Directory usage scan (Performance·Disk)",
    },
];

/// The honest, TUI-verified keyboard reference.
///
/// Shared commands come straight from the router table
/// ([`crate::command_help`]). The router's dialog-scope `Confirm` (Enter) is
/// filtered out because the TUI handles its end-task confirmation locally with
/// `y` / `n` / `Esc` *before* the router is consulted — listing Enter-as-confirm
/// would describe a binding this frontend does not wire. `ToggleSidebar` (F9)
/// is filtered for the same reason: the TUI has no sidebar surface, so a
/// terminal user must never be promised an F9 chord that toggles nothing. The
/// terminal-only bindings (declared in [`crate::shell_local_bindings`] and
/// handled in `runtime::handle_key`) are appended instead, together with the
/// TUI-local overlay bindings above (localized through
/// [`localize_tui_binding`]).
#[must_use]
pub fn help_rows() -> Vec<LocalBinding> {
    let mut rows: Vec<LocalBinding> = crate::command_help()
        .into_iter()
        .filter(|help| {
            help.command != CommandId::Confirm && help.command != CommandId::ToggleSidebar
        })
        .map(|help| LocalBinding {
            shortcut: help.shortcut,
            label: help.label,
        })
        .collect();
    rows.extend(crate::shell_local_bindings().iter().copied());
    rows.extend(TUI_LOCAL_BINDINGS.map(localize_tui_binding));
    rows
}

/// Resolve a TUI-local overlay binding's label through the shared i18n
/// catalog. [`TUI_LOCAL_BINDINGS`] stays an English-labeled const because the
/// command palette (owned outside `ui/`) iterates it directly; this fold is
/// the single shortcut → key mapping, so the overlay and the const can never
/// drift. Reuses existing catalog keys where the copy already existed
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
pub fn render_help_overlay(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let rows = help_rows();
    let popup = centered(area, 68, 24);
    frame.render_widget(Clear, popup);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.accent))
        .style(Style::new().bg(theme.overlay_bg))
        .title(format!(
            " {} {} ",
            crate::icon_glyph(IconId::Settings),
            t("menu.keyboard_reference")
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

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
            Line::from(vec![
                Span::styled(
                    " F1 / ? / Esc ",
                    Style::new().fg(Color::Black).bg(theme.accent),
                ),
                Span::styled(
                    format!("  {}", t("help.close_hint")),
                    Style::new().fg(theme.dim),
                ),
            ]),
        ])
        .alignment(Alignment::Center),
        footer,
    );
}

fn help_line(row: &LocalBinding, theme: TuiTheme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{:<10}", row.shortcut),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(row.label.to_owned(), Style::new().fg(Color::White)),
    ])
}

/// Render the searchable command palette: a filter field over the keyboard
/// reference rows. Typing narrows the list; Enter runs the selected row's
/// shared action (when it has one); Esc closes. The palette is the keyboard
/// user's command entry point — the same rows the static help shows, but
/// filterable and (for shared commands) executable. Takes the crate's
/// [`crate::TuiApp`] (not the shell alias) because the palette state is
/// TUI-local (ADR-027).
pub fn render_command_palette(
    frame: &mut Frame<'_>,
    app: &crate::TuiApp,
    theme: TuiTheme,
    area: Rect,
) {
    let rows = app.filtered_palette_rows();
    let popup = centered(area, 72, 26);
    frame.render_widget(Clear, popup);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.accent))
        .style(Style::new().bg(theme.overlay_bg))
        .title(format!(
            " {} {} ",
            crate::icon_glyph(IconId::Search),
            t("menu.keyboard_reference")
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

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
                Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]))
        .style(Style::new().fg(theme.dim)),
        filter_area,
    );

    let selection = app.command_palette().map_or(0, |palette| palette.selection);
    let lines: Vec<Line<'static>> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let active = index == selection;
            let shortcut = format!("{:<10}", row.shortcut);
            let executable = row.action.is_some();
            let mut spans = vec![
                Span::styled(
                    shortcut,
                    Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    row.label.to_owned(),
                    if executable {
                        Style::new().fg(Color::White)
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
        Paragraph::new(vec![Line::from(vec![
            Span::styled(
                format!(" {} ", t("help.palette_type")),
                Style::new().fg(Color::Black).bg(theme.accent),
            ),
            Span::styled(
                format!(" {}", t("help.palette_footer")),
                Style::new().fg(theme.dim),
            ),
        ])])
        .alignment(Alignment::Center),
        footer,
    );
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
#[path = "../../tests/gui/ui/help_tests.rs"]
mod tests;
