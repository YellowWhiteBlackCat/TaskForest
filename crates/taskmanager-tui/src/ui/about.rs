//! Independent application About over the shared build metadata.

use crate::command_palette::{TuiSurfaceAction, TuiSurfaceScope, surface_hint_pairs};
use crate::information::AboutTargetView;
use crate::{TuiApp, TuiTheme};
use ratatui::{Frame, layout::Rect};
use taskmanager_application::i18n::t;
use taskmanager_assets::product;
use taskmanager_shell::presentation::about::metadata;

pub(super) const VERSION: &str = env!("CARGO_PKG_VERSION");
pub(super) fn render_about_overlay_at(
    frame: &mut Frame<'_>,
    _app: &TuiApp,
    view: &AboutTargetView,
    theme: TuiTheme,
    popup: Rect,
) {
    let metadata = metadata(VERSION, product::LICENSE_SPDX, product::REPOSITORY_URL);
    let text = format!("{}\n\n{}", metadata.description, metadata.details_text());
    let hints = [
        TuiSurfaceAction::OpenRepository,
        TuiSurfaceAction::OpenSystemInformation,
        TuiSurfaceAction::CopyInformation,
        TuiSurfaceAction::ToggleAbout,
    ]
    .into_iter()
    .map(|action| {
        surface_hint_pairs(TuiSurfaceScope::About, action)
            .into_iter()
            .map(|(token, label)| format!("{token}{label}"))
            .collect::<String>()
    })
    .collect::<Vec<_>>()
    .join("\n");
    let actions = format!("{hints}\nUp/Down");
    super::information_review::render(
        frame,
        theme,
        t("about.title"),
        &text,
        view.scroll,
        &actions,
        popup,
    );
}

#[cfg(test)]
#[path = "../../tests/headless/ui/about_support.rs"]
pub(crate) mod about_support;
#[cfg(test)]
#[path = "../../tests/gui/ui/about_tests.rs"]
mod tests;
