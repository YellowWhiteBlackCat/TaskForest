//! Destructive-action confirmation overlays: end-task, service control
//! (Stop / Restart), and session control (Disconnect / Lock). Extracted
//! from `ui.rs` to keep the renderer dispatch under the source line budget.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use taskmanager_application::i18n::t;

use super::{centered, service_menu, session_menu};
use crate::TuiApp;
use crate::TuiTheme;

pub(super) fn render_end_confirmation(
    frame: &mut Frame<'_>,
    _app: &TuiApp,
    theme: TuiTheme,
    name: &str,
    pid: u32,
    area: Rect,
) {
    let popup = centered(area, 58, 9);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                t("confirm.end_headline")
                    .replace("{name}", name)
                    .replace("{pid}", &pid.to_string()),
                Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Line::from(t("confirm.recheck_body")),
            Line::from(""),
            Line::from(vec![
                Span::styled(" y ", Style::new().fg(Color::Black).bg(theme.danger)),
                Span::raw(format!(" {}    ", t("common.confirm"))),
                Span::styled(" n / Esc ", Style::new().fg(Color::Black).bg(Color::White)),
                Span::raw(format!(" {}", t("common.cancel"))),
            ]),
        ])
        .alignment(Alignment::Center)
        .block(
            Block::new()
                .title(format!(" {} ", t("confirm.process_title")))
                .borders(Borders::ALL)
                .border_style(Style::new().fg(theme.danger))
                .style(Style::new().bg(theme.overlay_bg)),
        )
        .wrap(Wrap { trim: true }),
        popup,
    );
}

/// Shared confirmation overlay for a gated service action (Stop / Restart).
/// The request is only emitted by `ConfirmServiceControl` (y); n / Esc clear
/// the pending target without submitting.
pub(super) fn render_service_control_confirmation(
    frame: &mut Frame<'_>,
    _app: &TuiApp,
    theme: TuiTheme,
    pending: &taskmanager_application::ServiceControlTarget,
    area: Rect,
) {
    let popup = centered(area, 60, 9);
    frame.render_widget(Clear, popup);
    let action_label = service_menu::action_label(pending.action);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                t("confirm.action_headline")
                    .replace("{action}", action_label)
                    .replace("{target}", pending.service_id.as_str()),
                Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Line::from(t("confirm.provider_body")),
            Line::from(""),
            Line::from(vec![
                Span::styled(" y ", Style::new().fg(Color::Black).bg(theme.danger)),
                Span::raw(format!(" {}    ", t("common.confirm"))),
                Span::styled(" n / Esc ", Style::new().fg(Color::Black).bg(Color::White)),
                Span::raw(format!(" {}", t("common.cancel"))),
            ]),
        ])
        .alignment(Alignment::Center)
        .block(
            Block::new()
                .title(format!(" {} ", t("confirm.service_title")))
                .borders(Borders::ALL)
                .border_style(Style::new().fg(theme.danger))
                .style(Style::new().bg(theme.overlay_bg)),
        )
        .wrap(Wrap { trim: true }),
        popup,
    );
}

/// The shared confirmation overlay for a gated session action (Disconnect /
/// Lock). The shell's `pending_session` gate owns the frozen target
/// (`ShellApp::select_session_control`); the platform request is produced by
/// `ShellApp::confirm_session_control` only on confirm (y), and n / Esc clear
/// the pending gate without submitting.
pub(super) fn render_session_control_confirmation(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    pending: &taskmanager_application::SessionControlConfirmation,
    area: Rect,
) {
    let popup = centered(area, 60, 9);
    frame.render_widget(Clear, popup);
    let action_label = session_menu::action_label(pending.action);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                t("confirm.session_headline")
                    .replace("{action}", action_label)
                    .replace("{id}", pending.session.id.as_str())
                    .replace("{user}", pending.session.user.as_str()),
                Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Line::from(t("confirm.provider_body")),
            Line::from(""),
            Line::from(vec![
                Span::styled(" y ", Style::new().fg(Color::Black).bg(theme.danger)),
                Span::raw(format!(" {}    ", t("common.confirm"))),
                Span::styled(" n / Esc ", Style::new().fg(Color::Black).bg(Color::White)),
                Span::raw(format!(" {}", t("common.cancel"))),
            ]),
        ])
        .alignment(Alignment::Center)
        .block(
            Block::new()
                .title(format!(" {} ", t("confirm.session_title")))
                .borders(Borders::ALL)
                .border_style(Style::new().fg(theme.danger))
                .style(Style::new().bg(theme.overlay_bg)),
        )
        .wrap(Wrap { trim: true }),
        popup,
    );
}

/// The gated startup Enable/Disable confirmation overlay. The request is only
/// emitted by the shell's `confirm_startup_control` (y); n / Esc clear the
/// pending gate without submitting.
pub(super) fn render_startup_control_confirmation(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    pending: &taskmanager_application::StartupControlRequest,
    area: Rect,
) {
    let popup = centered(area, 60, 9);
    frame.render_widget(Clear, popup);
    let action_label = if pending.enabled {
        t("startup.enable")
    } else {
        t("startup.disable")
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                t("confirm.action_headline")
                    .replace("{action}", action_label)
                    .replace("{target}", pending.entry.name.as_str()),
                Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Line::from(t("confirm.provider_body")),
            Line::from(""),
            Line::from(vec![
                Span::styled(" y ", Style::new().fg(Color::Black).bg(theme.danger)),
                Span::raw(format!(" {}    ", t("common.confirm"))),
                Span::styled(" n / Esc ", Style::new().fg(Color::Black).bg(Color::White)),
                Span::raw(format!(" {}", t("common.cancel"))),
            ]),
        ])
        .alignment(Alignment::Center)
        .block(
            Block::new()
                .title(format!(" {} ", t("confirm.startup_title")))
                .borders(Borders::ALL)
                .border_style(Style::new().fg(theme.accent))
                .style(Style::new().bg(theme.overlay_bg)),
        )
        .wrap(Wrap { trim: true }),
        popup,
    );
}

/// The gated destructive batch (Kill) confirmation overlay. The request is
/// only emitted by the shell's `confirm_process_batch` (y); n / Esc clears
/// the pending intent without submitting. The target scope shows the full
/// frozen set so a multi-select Kill reads as "N processes" rather than the
/// single first row.
pub(super) fn render_batch_confirmation(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    intent: &taskmanager_application::ProcessBatchIntent,
    area: Rect,
) {
    let popup = centered(area, 62, 9);
    frame.render_widget(Clear, popup);
    let targets = &intent.targets;
    let scope = if targets.len() <= 1 {
        targets.first().map_or_else(
            || t("confirm.selected_process").to_owned(),
            |target| format!("{} ({})", target.name, target.pid),
        )
    } else {
        format!("{} {}", targets.len(), t("proc.process_count"))
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                t("confirm.action_headline")
                    .replace("{action}", t("proc.kill"))
                    .replace("{target}", &scope),
                Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Line::from(t("confirm.frozen_body")),
            Line::from(""),
            Line::from(vec![
                Span::styled(" y ", Style::new().fg(Color::Black).bg(theme.danger)),
                Span::raw(format!(" {}    ", t("common.confirm"))),
                Span::styled(" n / Esc ", Style::new().fg(Color::Black).bg(Color::White)),
                Span::raw(format!(" {}", t("common.cancel"))),
            ]),
        ])
        .alignment(Alignment::Center)
        .block(
            Block::new()
                .title(format!(" {} ", t("confirm.batch_title")))
                .borders(Borders::ALL)
                .border_style(Style::new().fg(theme.danger))
                .style(Style::new().bg(theme.overlay_bg)),
        )
        .wrap(Wrap { trim: true }),
        popup,
    );
}

pub(super) fn render_smart_self_test_confirmation(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    intent: &taskmanager_application::SmartSelfTestIntent,
    area: Rect,
) {
    let popup = centered(area, 62, 9);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(
                    "{:?} SMART self-test · {}",
                    intent.kind, intent.display_name
                ),
                Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Line::from(t("confirm.provider_body")),
            Line::from(""),
            Line::from(vec![
                Span::styled(" y ", Style::new().fg(Color::Black).bg(theme.danger)),
                Span::raw(format!(" {}    ", t("common.confirm"))),
                Span::styled(" n / Esc ", Style::new().fg(Color::Black).bg(Color::White)),
                Span::raw(format!(" {}", t("common.cancel"))),
            ]),
        ])
        .alignment(Alignment::Center)
        .block(
            Block::new()
                .title(" SMART self-test ")
                .borders(Borders::ALL)
                .border_style(Style::new().fg(theme.danger))
                .style(Style::new().bg(theme.overlay_bg)),
        )
        .wrap(Wrap { trim: true }),
        popup,
    );
}
