//! The discrete settings-choice ladders shared by the discrete settings
//! surfaces.
//!
//! GPUI and Iced drive continuous controls over the same policy clamp; the
//! TUI and Bevy settings forms offer these discrete steps instead. The
//! ladders are one product fact — the offered choices and the live value's
//! index among them live here so two surfaces cannot drift apart.

use taskmanager_application::TelemetryInterval;

/// The telemetry refresh-cadence choices (ms), in display order — inside the
/// policy clamp window, so [`TelemetryInterval::clamped`] never deviates from
/// a requested step.
pub const REFRESH_CHOICES_MS: [u64; 4] = [500, 1_000, 2_000, 5_000];

/// The history-capacity choices (samples), in display order — steps inside
/// the shared store's bounded ring (`shell::presentation::graph_window`).
pub const CAPACITY_CHOICES: [usize; 4] = [60, 120, 300, 600];

/// The index of the live interval among the offered cadence choices; `None`
/// when the effective interval is not one of the offered steps (a continuous
/// surface's off-step value) — rendered honestly as no selection.
#[must_use]
pub fn refresh_choice_index(interval: TelemetryInterval) -> Option<usize> {
    let millis = u64::try_from(interval.duration().as_millis()).unwrap_or(u64::MAX);
    REFRESH_CHOICES_MS
        .iter()
        .position(|&choice| choice == millis)
}

/// The index of the live capacity among the offered capacity choices; `None`
/// when the capacity is not one of the offered steps.
#[must_use]
pub fn capacity_choice_index(capacity: usize) -> Option<usize> {
    CAPACITY_CHOICES
        .iter()
        .position(|&choice| choice == capacity)
}

#[cfg(test)]
#[path = "../../tests/headless/ui_settings_choices.rs"]
mod ui_settings_choices_tests;
