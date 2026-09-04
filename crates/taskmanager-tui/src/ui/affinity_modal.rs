//! CPU-affinity editor modal overlay (Applications page).

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;

use super::containers::{KeyHint, Modal};
use crate::surface::AFFINITY_GRID_COLS;
use crate::{AffinityModalState, TuiApp, TuiTheme};

/// Render the interactive CPU affinity editor modal overlay.
pub(super) fn render_affinity_modal_at(
    frame: &mut Frame<'_>,
    _app: &TuiApp,
    state: &AffinityModalState,
    theme: TuiTheme,
    popup: Rect,
) {
    let title = format!("{} · {}", t("dialog.cpu_affinity"), state.target.name);
    let inner = Modal::new(theme, IconId::Cpu, &title).render(frame, popup);

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .areas(inner);

    let header_lines = vec![
        Line::from(vec![
            Span::styled("  ", Style::new().fg(theme.dim)),
            Span::styled(
                if state.target.name.trim().is_empty() {
                    t("proc.unknown_process")
                } else {
                    state.target.name.as_str()
                },
                Style::new()
                    .fg(theme.color(Color::White))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  PID: {}", state.target.pid),
                Style::new().fg(theme.dim),
            ),
            Span::styled(
                if state.mask_observed {
                    format!(
                        "  ({}/{} CPUs enabled)",
                        state.selected_mask.len(),
                        state.logical_cpu_count
                    )
                } else {
                    format!("  {}", t("common.collecting_telemetry"))
                },
                Style::new().fg(theme.accent),
            ),
        ]),
        Line::from(""),
    ];
    frame.render_widget(Paragraph::new(header_lines), header);

    let total_rows = state.logical_cpu_count.div_ceil(AFFINITY_GRID_COLS);
    let visible_height = usize::from(body.height);
    let current_row = state.selected_cpu / AFFINITY_GRID_COLS;
    let scroll = if current_row < state.scroll {
        current_row
    } else if current_row >= state.scroll.saturating_add(visible_height) {
        current_row.saturating_add(1).saturating_sub(visible_height)
    } else {
        state.scroll
    };
    let scroll = scroll.min(total_rows.saturating_sub(visible_height));

    let mut lines = Vec::with_capacity(visible_height);
    let end_row = (scroll + visible_height).min(total_rows);

    for row in scroll..end_row {
        let mut spans = Vec::with_capacity(AFFINITY_GRID_COLS);
        for col in 0..AFFINITY_GRID_COLS {
            let cpu = row * AFFINITY_GRID_COLS + col;
            if cpu < state.logical_cpu_count {
                let Ok(cpu_id) = u32::try_from(cpu) else {
                    break;
                };
                let is_selected = state.selected_mask.contains(&cpu_id);
                let is_cursor = state.selected_cpu == cpu;

                let marker = if is_cursor { "▸" } else { " " };
                let check = if is_selected { "[x]" } else { "[ ]" };
                let cell_text = format!("{marker} {check} CPU {:<2}   ", cpu);

                let style = if is_cursor {
                    Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
                } else if is_selected {
                    Style::new().fg(theme.color(Color::White))
                } else {
                    Style::new().fg(theme.dim)
                };

                spans.push(Span::styled(cell_text, style));
            }
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), body);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            KeyHint::line(
                theme,
                vec![
                    (" Space ", "Toggle".to_string()),
                    (" a ", "Toggle All".to_string()),
                    (" Enter ", "Apply".to_string()),
                    (" Esc ", "Cancel".to_string()),
                ],
            ),
        ])
        .alignment(Alignment::Center),
        footer,
    );
}
