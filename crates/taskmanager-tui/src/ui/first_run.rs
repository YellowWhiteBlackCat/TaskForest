//! Bounded optional-setup review; only the descriptor viewport scrolls.

use super::containers::Modal;
use super::process_details::{clamped_scroll, wrapped_content_height};
use crate::first_run::FirstRunTargetView;
use crate::{TuiApp, TuiTheme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use taskmanager_application::first_run::FirstRunPhase;
use taskmanager_application::i18n::t;
use taskmanager_core::core::setup::SetupScriptAction;
use taskmanager_shell::presentation::first_run::first_run_phase_key;
use taskmanager_ui_contract::IconId;

pub(super) fn render_first_run_overlay_at(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    view: &FirstRunTargetView,
    theme: TuiTheme,
    popup: Rect,
) {
    let state = app.first_run.view();
    let inner = Modal::new(theme, IconId::Settings, t("first_run.title")).render(frame, popup);
    let mut text = t("first_run.description").to_owned();
    if let Some(info) = &state.info {
        text.push_str(&format!(
            "\n\n{}\n{}\n\n{}\n{}\n\n{}\n{}",
            t("first_run.location"),
            info.path.display(),
            t("first_run.run_command"),
            info.run_command,
            t("first_run.revert_command"),
            info.revert_command
        ));
        text.push_str(&format!(
            "\n\n1 {} · 2 {} · 3 {}\nd {}",
            t("first_run.copy_location"),
            t("first_run.copy_command"),
            t("first_run.copy_revert_command"),
            t("first_run.open_docs")
        ));
    }
    if let Some(key) = first_run_phase_key(&state.phase) {
        text.push_str(&format!("\n\n{}", t(key)));
    }
    let mut actions = if state.action_pending() {
        format!("Esc {}", t("common.close"))
    } else {
        format!(
            "v {} · r {}\nu {} · Esc {}",
            t("first_run.view_script"),
            t("first_run.run_setup"),
            t("first_run.revert_setup"),
            t("common.close")
        )
    };
    if state.phase == FirstRunPhase::RestartRequired {
        actions.push_str(&format!("\nR {}", t("first_run.restart")));
    }
    if matches!(state.phase, FirstRunPhase::Failed(_))
        && state.last_action.is_some_and(|action| {
            matches!(action, SetupScriptAction::Run | SetupScriptAction::Revert)
        })
    {
        actions.push_str(&format!("\nt {}", t("first_run.retry")));
    }
    actions.push_str(&format!(
        "\nUp/Down/PgUp/PgDn {}",
        t("proc_insights.scroll_hint")
    ));
    let footer = Paragraph::new(actions).wrap(Wrap { trim: true });
    let height = u16::try_from(footer.line_count(inner.width).saturating_add(1))
        .unwrap_or(u16::MAX)
        .min(inner.height);
    let [body, actions] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(height)]).areas(inner);
    let lines: Vec<_> = text.lines().map(Line::from).collect();
    let (scroll, _) = clamped_scroll(
        wrapped_content_height(&lines, body.width),
        body.height.saturating_sub(1),
        view.scroll,
    );
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        body,
    );
    frame.render_widget(footer, actions);
}
