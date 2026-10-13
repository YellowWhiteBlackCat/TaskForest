//! Bootstrap TaskForest's owned UI registries and bundled fonts.

use gpui::App;
use taskmanager_assets::embedded_fonts;
use taskmanager_ui::init;
use tracing::error;

pub(super) fn bootstrap_owned_ui(cx: &mut App) {
    init(cx);
    if let Err(font_error) = cx.text_system().add_fonts(embedded_fonts()) {
        error!(%font_error, "embedded font registration failed; falling back to system fonts");
    }
}
