//! The Iced binding from a live neutral theme snapshot to the shared
//! `ControlRole` measurement matrix.
//!
//! `taskmanager-ui-contract::presentation::sizes` owns the roles, states, and
//! the invariant that focus/hover/press/disabled never change geometry;
//! `crate::presentation_binding` owns the single fold into Iced's declared
//! `ScaledPixels` unit. This module only selects the live UI-size/density axis
//! from the snapshot and exposes the resolved values under the same public
//! `theme::*` paths every control consumes. No renderer owns a second ladder.

use taskmanager_theme::Theme;
use taskmanager_theme::tokens::{RowDensity, UiSize};
use taskmanager_ui_contract::presentation::sizes::{ControlMeasurement, ControlRole, ControlState};

/// Resolve the live control geometry from the shared role/state matrix.
///
/// Iced applies the product zoom at the application boundary, so a control
/// authored in the theme's logical px stays on the shared `ScaledPixels`
/// extent; the theme snapshot itself carries the UI-size axis through
/// `root_rem_size` (Small 14 / Standard 16 / Large 18). Only the measurement
/// is shared: the roles, hit targets and the state→geometry invariant all
/// come from `taskmanager-ui-contract`, never a second Iced ladder.
#[must_use]
pub fn control_measurement(theme: &Theme, role: ControlRole) -> ControlMeasurement {
    let size = ui_size_from_root(theme.root_rem_size);
    crate::presentation_binding::control_measurement(
        role,
        ControlState::Normal,
        size,
        RowDensity::Comfortable,
    )
}

/// The UI-size axis the live root rem encodes. Unknown root sizes fail closed
/// to Small, the unscaled baseline, so a malformed window value cannot invent
/// a fourth product size.
#[must_use]
pub fn ui_size_from_root(root_rem_size: f32) -> UiSize {
    UiSize::ALL
        .into_iter()
        .find(|size| (size.body_font_size().0 - root_rem_size).abs() <= 0.01)
        .unwrap_or(UiSize::Small)
}

/// The role's resolved height in theme logical px. State never changes this
/// value: `ControlState::changes_measurement` permits only `Compact`, and
/// control roles do not admit density.
#[must_use]
pub fn control_extent(theme: &Theme, role: ControlRole) -> f32 {
    control_measurement(theme, role).extent
}

/// The role's resolved horizontal padding in theme logical px.
#[must_use]
pub fn control_padding_x(theme: &Theme, role: ControlRole) -> f32 {
    control_measurement(theme, role).padding_x
}

/// The role's resolved vertical padding in theme logical px.
#[must_use]
pub fn control_padding_y(theme: &Theme, role: ControlRole) -> f32 {
    control_measurement(theme, role).padding_y
}
