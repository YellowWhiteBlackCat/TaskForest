//! Iced's binding of the shared presentation resolvers: scaled logical pixels
//! and `iced::Animation` classes, with no second measurement or motion table.

use taskmanager_theme::tokens::{
    DURATION_FAST, DURATION_HOVER, DURATION_MEDIUM, MotionPolicy, RowDensity, UiSize,
};
use taskmanager_ui_contract::FrontendShape;
use taskmanager_ui_contract::presentation::motion::{
    MotionDurations, MotionResolution, MotionRole, resolve_motion,
};
use taskmanager_ui_contract::presentation::resolution::{
    ResolvedPerformance, ResolvedPresentation, resolution_marker_text,
};
use taskmanager_ui_contract::presentation::sizes::{
    ControlMeasurement, ControlRole, ControlState, DensityPoint, ResolutionUnit, UiSizePoint,
    resolve_size,
};
use taskmanager_ui_contract::presentation::vocabulary::DeviceNavigationPresentation;

/// The theme's duration tokens as the neutral contract consumes them: one
/// explicit weld per frontend, so a token rename cannot leave a shape behind.
#[must_use]
pub fn motion_durations() -> MotionDurations {
    MotionDurations {
        fast: DURATION_FAST,
        hover: DURATION_HOVER,
        medium: DURATION_MEDIUM,
    }
}

/// Build the capture marker text for one resolved frame.
///
/// The values are the ones the frame ACTUALLY resolved; the shared contract
/// verifies them against the same folds it mandates.
#[must_use]
pub fn presentation_resolution_marker(
    frame: &crate::ui::responsive::IcedPerformanceFrame,
    ui_size: UiSize,
    density: RowDensity,
) -> Option<String> {
    let budget = frame.budget;
    let measured = control_measurement(ControlRole::Button, ControlState::Normal, ui_size, density);
    let resolved = ResolvedPresentation {
        profile: frame.profile,
        vertical_space: crate::ui::responsive::page_layout_for_viewport(iced::Size::new(
            frame.budget.workspace_width,
            frame.content_height,
        ))
        .vertical_space,
        page_padding: frame.page_padding,
        width_input: frame.width_input,
        page_content_height: frame.content_height,
        control_height: ui_size.control_height().0,
        body_font_size: ui_size.body_font_size().0,
        renderer_scale: ui_size.renderer_scale(),
        row_padding_y: density.row_padding_y().0,
        line_height: density.line_height().0,
        control_extent: measured.extent,
        hit_target: measured.hit_target,
        performance: Some(ResolvedPerformance {
            runway: budget.vertical,
            chart_inventory: budget.chart_inventory,
            details: budget.details,
            device_navigation: budget.device_navigation,
            workspace_width: frame.budget.workspace_width,
            content_height: frame.content_height,
            sidebar_visible: budget.device_navigation == DeviceNavigationPresentation::Sidebar,
            persisted_sidebar_width: None,
            sidebar_width: budget.sidebar_width,
            stats_width: budget.stats_width,
            main_width: budget.main_width,
        }),
    };
    Some(resolution_marker_text(&resolved))
}

/// Iced applies product zoom at the application boundary, so the resolved
/// logical px are already in the shape's declared unit; the theme's axis
/// values lift here and nowhere else.
#[must_use]
pub fn control_measurement(
    role: ControlRole,
    state: ControlState,
    size: UiSize,
    density: RowDensity,
) -> ControlMeasurement {
    let measured = resolve_size(
        FrontendShape::Iced,
        role,
        state,
        UiSizePoint {
            control_height: size.control_height().0,
            body_font_size: size.body_font_size().0,
            renderer_scale: size.renderer_scale(),
        },
        DensityPoint {
            row_padding_y: density.row_padding_y().0,
            line_height: density.line_height().0,
        },
    );
    debug_assert_eq!(measured.unit, ResolutionUnit::ScaledPixels);
    measured
}

#[must_use]
pub fn motion_resolution(role: MotionRole, policy: MotionPolicy) -> MotionResolution {
    let durations = motion_durations();
    let duration = policy.animation_duration(durations.for_class(role.duration_class()));
    resolve_motion(FrontendShape::Iced, role, duration)
}

#[cfg(test)]
#[path = "../tests/headless/presentation_binding.rs"]
mod presentation_binding_tests;
