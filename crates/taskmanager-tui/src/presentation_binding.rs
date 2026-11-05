//! TUI's binding of the shared presentation resolvers: terminal cells and a
//! whole-frame repaint. The neutral contract still owns the role, measurement
//! and motion slots; this module lifts the theme's axis values and reports
//! how a cell grid realizes them.

use taskmanager_theme::tokens::{
    DURATION_FAST, DURATION_HOVER, DURATION_MEDIUM, MotionPolicy, RowDensity, UiSize,
};
use taskmanager_ui_contract::FrontendShape;
use taskmanager_ui_contract::presentation::motion::{
    MotionDurations, MotionResolution, MotionRole, resolve_motion,
};
use taskmanager_ui_contract::presentation::sizes::{
    ControlMeasurement, ControlRole, ControlState, DensityPoint, ResolutionUnit, UiSizePoint,
    resolve_size,
};

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

/// `realized_extent` is a whole-cell count: the terminal has no sub-cell
/// geometry, and the shared logical-px intent stays in `extent`.
#[must_use]
pub fn control_measurement(
    role: ControlRole,
    state: ControlState,
    size: UiSize,
    density: RowDensity,
) -> ControlMeasurement {
    let measured = resolve_size(
        FrontendShape::Tui,
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
    debug_assert_eq!(measured.unit, ResolutionUnit::TerminalCells);
    debug_assert_eq!(measured.realized_extent, measured.extent.ceil());
    measured
}

/// Resolve one motion role. A terminal repaints the final state; the theme's
/// policy is applied here so the neutral contract never imports the codec.
#[must_use]
pub fn motion_resolution(role: MotionRole, policy: MotionPolicy) -> MotionResolution {
    let durations = motion_durations();
    let duration = policy.animation_duration(durations.for_class(role.duration_class()));
    resolve_motion(FrontendShape::Tui, role, duration)
}

#[cfg(test)]
#[path = "../tests/headless/presentation_binding.rs"]
mod presentation_binding_tests;
