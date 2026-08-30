//! Test-only confirmation-overlay adapters: freeze the popup rectangle the
//! production renderer would compute and delegate to the `_at` renderers.
//! Production dispatch never routes through this module.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::{
    render_batch_confirmation_at, render_end_confirmation_at,
    render_service_control_confirmation_at, render_session_control_confirmation_at,
    render_smart_self_test_confirmation_at, render_startup_control_confirmation_at,
};
use crate::{TuiApp, TuiTheme};

pub(crate) fn render_end_confirmation(
    frame: &mut Frame<'_>,
    _app: &TuiApp,
    theme: TuiTheme,
    name: &str,
    pid: u32,
    area: Rect,
) {
    render_end_confirmation_at(
        frame,
        _app,
        theme,
        name,
        pid,
        crate::ui::frame_plan::overlay_popup(
            area,
            crate::TuiInputScope::SharedSurface(
                taskmanager_application::SurfaceKind::Confirmation(
                    taskmanager_application::ConfirmationKind::EndTask,
                ),
            ),
        )
        .unwrap_or(Rect::ZERO),
    );
}

pub(crate) fn render_service_control_confirmation(
    frame: &mut Frame<'_>,
    _app: &TuiApp,
    theme: TuiTheme,
    pending: &taskmanager_application::ServiceControlTarget,
    area: Rect,
) {
    render_service_control_confirmation_at(
        frame,
        _app,
        theme,
        pending,
        crate::ui::frame_plan::overlay_popup(
            area,
            crate::TuiInputScope::SharedSurface(
                taskmanager_application::SurfaceKind::Confirmation(
                    taskmanager_application::ConfirmationKind::ServiceControl,
                ),
            ),
        )
        .unwrap_or(Rect::ZERO),
    );
}

pub(crate) fn render_session_control_confirmation(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    pending: &taskmanager_application::SessionControlConfirmation,
    area: Rect,
) {
    render_session_control_confirmation_at(
        frame,
        theme,
        pending,
        crate::ui::frame_plan::overlay_popup(
            area,
            crate::TuiInputScope::SharedSurface(
                taskmanager_application::SurfaceKind::Confirmation(
                    taskmanager_application::ConfirmationKind::SessionControl,
                ),
            ),
        )
        .unwrap_or(Rect::ZERO),
    );
}

pub(crate) fn render_startup_control_confirmation(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    pending: &taskmanager_application::StartupControlRequest,
    area: Rect,
) {
    render_startup_control_confirmation_at(
        frame,
        theme,
        pending,
        crate::ui::frame_plan::overlay_popup(
            area,
            crate::TuiInputScope::SharedSurface(
                taskmanager_application::SurfaceKind::Confirmation(
                    taskmanager_application::ConfirmationKind::StartupControl,
                ),
            ),
        )
        .unwrap_or(Rect::ZERO),
    );
}

pub(crate) fn render_batch_confirmation(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    intent: &taskmanager_core::core::process::ProcessBatchIntent,
    area: Rect,
) {
    render_batch_confirmation_at(
        frame,
        theme,
        intent,
        crate::ui::frame_plan::overlay_popup(
            area,
            crate::TuiInputScope::SharedSurface(
                taskmanager_application::SurfaceKind::Confirmation(
                    taskmanager_application::ConfirmationKind::ProcessBatch,
                ),
            ),
        )
        .unwrap_or(Rect::ZERO),
    );
}

pub(crate) fn render_smart_self_test_confirmation(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    intent: &taskmanager_core::core::system_health::SmartSelfTestIntent,
    area: Rect,
) {
    render_smart_self_test_confirmation_at(
        frame,
        theme,
        intent,
        crate::ui::frame_plan::overlay_popup(
            area,
            crate::TuiInputScope::SharedSurface(
                taskmanager_application::SurfaceKind::Confirmation(
                    taskmanager_application::ConfirmationKind::SmartSelfTest,
                ),
            ),
        )
        .unwrap_or(Rect::ZERO),
    );
}
