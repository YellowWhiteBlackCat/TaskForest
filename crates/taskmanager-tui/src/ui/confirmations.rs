//! Destructive-action confirmation overlays: end-task, service control
//! (Stop / Restart), and session control (Disconnect / Lock). Extracted
//! from `ui.rs` to keep the renderer dispatch under the source line budget.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use taskmanager_application::i18n::t;

use super::containers::{KeyHint, KeyHintTone, Modal};
use super::{service_menu, session_menu};
use crate::TuiApp;
use crate::TuiTheme;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../tests/headless/ui/confirmations_support.rs"]
pub(crate) mod confirmations_support;

/// The confirmation family's shared confirm/dismiss hint line: the
/// black-on-danger `y` chord and the black-on-white `n / Esc` chord over the
/// default-foreground labels the popups have always painted, routed through
/// the shared [`KeyHint`] component's typed [`KeyHintTone`] vocabulary.
fn confirm_hint_line(theme: TuiTheme) -> Line<'static> {
    KeyHint::line_toned(
        theme,
        vec![
            (
                KeyHintTone::Danger,
                " y ",
                format!(" {}    ", t("common.confirm")),
            ),
            (
                KeyHintTone::Inverse,
                " n / Esc ",
                format!(" {}", t("common.cancel")),
            ),
        ],
    )
}

pub(super) fn render_end_confirmation_at(
    frame: &mut Frame<'_>,
    _app: &TuiApp,
    theme: TuiTheme,
    name: &str,
    pid: u32,
    popup: Rect,
) {
    let inner = Modal::alert(theme, theme.danger, t("confirm.process_title")).render(frame, popup);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                t("confirm.end_headline")
                    .replace("{name}", name)
                    .replace("{pid}", &pid.to_string()),
                Style::new()
                    .fg(theme.color(Color::White))
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(t("confirm.recheck_body")),
            Line::from(""),
            confirm_hint_line(theme),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true }),
        inner,
    );
}

/// Shared confirmation overlay for a gated service action (Stop / Restart).
/// The request is only emitted by `ConfirmServiceControl` (y); n / Esc clear
/// the pending target without submitting.
pub(super) fn render_service_control_confirmation_at(
    frame: &mut Frame<'_>,
    _app: &TuiApp,
    theme: TuiTheme,
    pending: &taskmanager_application::ServiceControlTarget,
    popup: Rect,
) {
    let inner = Modal::alert(theme, theme.danger, t("confirm.service_title")).render(frame, popup);
    let action_label = service_menu::action_label(pending.action);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                t("confirm.action_headline")
                    .replace("{action}", action_label)
                    .replace("{target}", pending.service_id.as_str()),
                Style::new()
                    .fg(theme.color(Color::White))
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(t("confirm.provider_body")),
            Line::from(""),
            confirm_hint_line(theme),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true }),
        inner,
    );
}

/// The shared confirmation overlay for a gated session action (Disconnect /
/// Lock). The shell's `pending_session` gate owns the frozen target
/// (`ShellApp::select_session_control`); the platform request is produced by
/// `ShellApp::confirm_session_control` only on confirm (y), and n / Esc clear
/// the pending gate without submitting.
pub(super) fn render_session_control_confirmation_at(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    pending: &taskmanager_application::SessionControlConfirmation,
    popup: Rect,
) {
    let inner = Modal::alert(theme, theme.danger, t("confirm.session_title")).render(frame, popup);
    let action_label = session_menu::action_label(pending.action);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                t("confirm.session_headline")
                    .replace("{action}", action_label)
                    .replace("{id}", pending.session.id.as_str())
                    .replace("{user}", pending.session.user.as_str()),
                Style::new()
                    .fg(theme.color(Color::White))
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(t("confirm.provider_body")),
            Line::from(""),
            confirm_hint_line(theme),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true }),
        inner,
    );
}

/// The gated startup Enable/Disable confirmation overlay. The request is only
/// emitted by the shell's `confirm_startup_control` (y); n / Esc clear the
/// pending gate without submitting.
pub(super) fn render_startup_control_confirmation_at(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    pending: &taskmanager_application::StartupControlRequest,
    popup: Rect,
) {
    let inner = Modal::alert(theme, theme.accent, t("confirm.startup_title")).render(frame, popup);
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
                Style::new()
                    .fg(theme.color(Color::White))
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(t("confirm.provider_body")),
            Line::from(""),
            confirm_hint_line(theme),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true }),
        inner,
    );
}

/// The gated destructive batch (Kill) confirmation overlay. The request is
/// only emitted by the shell's `confirm_process_batch` (y); n / Esc clears
/// the pending intent without submitting. The target scope shows the full
/// frozen set so a multi-select Kill reads as "N processes" rather than the
/// single first row.
pub(super) fn render_batch_confirmation_at(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    intent: &taskmanager_core::core::process::ProcessBatchIntent,
    popup: Rect,
) {
    let inner = Modal::alert(theme, theme.danger, t("confirm.batch_title")).render(frame, popup);
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
                Style::new()
                    .fg(theme.color(Color::White))
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(t("confirm.frozen_body")),
            Line::from(""),
            confirm_hint_line(theme),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true }),
        inner,
    );
}

pub(super) fn render_smart_self_test_confirmation_at(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    intent: &taskmanager_core::core::system_health::SmartSelfTestIntent,
    popup: Rect,
) {
    let inner = Modal::alert(theme, theme.danger, "SMART self-test").render(frame, popup);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(
                    "{:?} SMART self-test · {}",
                    intent.kind, intent.display_name
                ),
                Style::new()
                    .fg(theme.color(Color::White))
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(t("confirm.provider_body")),
            Line::from(""),
            confirm_hint_line(theme),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true }),
        inner,
    );
}
