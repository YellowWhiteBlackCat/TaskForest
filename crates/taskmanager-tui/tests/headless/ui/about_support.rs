//! Test-only About overlay adapter: freeze the popup rectangle the production
//! renderer would compute and delegate to the `_at` renderer.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::render_about_overlay_at;
use crate::ui::frame_plan::overlay_popup;
use crate::{TuiApp, TuiTheme};

pub(crate) fn render_about_overlay(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
) {
    render_about_overlay_at(
        frame,
        app,
        theme,
        overlay_popup(
            area,
            crate::TuiInputScope::LocalSurface(crate::TuiSurfaceKind::About),
        )
        .unwrap_or(Rect::ZERO),
    );
}
