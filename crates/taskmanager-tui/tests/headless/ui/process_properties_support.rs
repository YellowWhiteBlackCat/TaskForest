//! Test-only Process Properties overlay adapter.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::render_process_properties_at;
use crate::ui::TuiFocusPlan;
use crate::ui::frame_plan::overlay_popup;
use crate::{TuiApp, TuiTheme};

pub(crate) fn render_process_properties(
    frame: &mut Frame<'_>,
    target: &ProcessPropertiesTarget,
    app: &TuiApp,
    theme: TuiTheme,
    focus: TuiFocusPlan,
    area: Rect,
) {
    render_process_properties_at(
        frame,
        target,
        app,
        theme,
        focus,
        overlay_popup(
            area,
            crate::TuiInputScope::SharedSurface(
                taskmanager_application::SurfaceKind::ProcessProperties,
            ),
        )
        .unwrap_or(Rect::ZERO),
    );
}

pub(crate) use super::ProcessPropertiesTarget;
