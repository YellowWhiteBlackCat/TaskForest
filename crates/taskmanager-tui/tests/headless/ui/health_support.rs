//! Test-only health overlay adapter.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::render_health_overlay_at;
use crate::ui::frame_plan::overlay_popup;
use crate::{TuiApp, TuiTheme};

pub(crate) fn render_health_overlay(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
) {
    render_health_overlay_at(
        frame,
        app,
        theme,
        overlay_popup(
            area,
            crate::TuiInputScope::LocalSurface(crate::TuiSurfaceKind::Health),
        )
        .unwrap_or(Rect::ZERO),
    );
}
