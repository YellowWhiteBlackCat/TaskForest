//! Test-only help overlay adapter.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::render_help_overlay_at;
use crate::ui::frame_plan::overlay_popup;
use crate::{TuiApp, TuiTheme};

pub(crate) fn render_help_overlay(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
) {
    render_help_overlay_at(
        frame,
        app,
        theme,
        overlay_popup(area, crate::TuiInputScope::Help).unwrap_or(Rect::ZERO),
    );
}
