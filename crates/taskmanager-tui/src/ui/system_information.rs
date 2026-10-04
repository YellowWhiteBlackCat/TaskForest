//! Independent system-information surface over the shared frozen groups.

use crate::TuiTheme;
use crate::information::SystemInformationTargetView;
use ratatui::{Frame, layout::Rect};
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::system_information::copy_all_text;

pub(super) fn render(
    frame: &mut Frame<'_>,
    view: &SystemInformationTargetView,
    theme: TuiTheme,
    popup: Rect,
) {
    let text = if view.facts.is_empty() {
        t("system_about.unavailable").to_owned()
    } else {
        copy_all_text(&view.facts)
    };
    let actions = format!(
        "c {} · Esc {}\nUp/Down/PgUp/PgDn {}",
        t("system_about.copy_all"),
        t("common.close"),
        t("proc_insights.scroll_hint")
    );
    super::information_review::render(
        frame,
        theme,
        t("system_about.title"),
        &text,
        view.scroll,
        &actions,
        popup,
    );
}
