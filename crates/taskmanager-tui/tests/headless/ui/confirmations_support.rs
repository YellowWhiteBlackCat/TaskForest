//! Test-only confirmation-overlay adapter: freeze the popup rectangle the
//! production renderer would compute and delegate to the `_at` renderer.
//! Production dispatch never routes through this module.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::render_startup_control_confirmation_at;
use crate::TuiTheme;
use taskmanager_application::{ConfirmationKind, StartupControlRequest, SurfaceKind};

pub(crate) fn render_startup_control_confirmation(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    pending: &StartupControlRequest,
    area: Rect,
) {
    render_startup_control_confirmation_at(
        frame,
        theme,
        pending,
        crate::ui::frame_plan::overlay_popup(
            area,
            crate::TuiInputScope::SharedSurface(SurfaceKind::Confirmation(
                ConfirmationKind::StartupControl,
            )),
        )
        .unwrap_or(Rect::ZERO),
    );
}
