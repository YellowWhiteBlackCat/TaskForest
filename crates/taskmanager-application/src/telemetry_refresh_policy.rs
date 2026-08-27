//! Frontend-owned scheduling policy for system telemetry refreshes.

use std::time::Duration;

/// Smallest accepted frontend telemetry cadence.
pub const MIN_TELEMETRY_INTERVAL: Duration = Duration::from_millis(100);
/// Largest accepted interactive telemetry cadence.
pub const MAX_TELEMETRY_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryIntervalError {
    TooFast,
    TooSlow,
}

/// Validated frontend scheduling interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TelemetryInterval(Duration);

impl TelemetryInterval {
    pub fn new(duration: Duration) -> Result<Self, TelemetryIntervalError> {
        if duration < MIN_TELEMETRY_INTERVAL {
            Err(TelemetryIntervalError::TooFast)
        } else if duration > MAX_TELEMETRY_INTERVAL {
            Err(TelemetryIntervalError::TooSlow)
        } else {
            Ok(Self(duration))
        }
    }

    #[must_use]
    pub fn clamped(duration: Duration) -> Self {
        Self(duration.clamp(MIN_TELEMETRY_INTERVAL, MAX_TELEMETRY_INTERVAL))
    }

    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}

impl Default for TelemetryInterval {
    fn default() -> Self {
        Self(Duration::from_secs(1))
    }
}

impl From<TelemetryInterval> for Duration {
    fn from(interval: TelemetryInterval) -> Self {
        interval.duration()
    }
}

/// Synchronous changes accepted by [`TelemetryRefreshPolicy::apply`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryRefreshPolicyChange {
    /// Set the persistent/manual pause controlled by Ctrl+Space.
    SetPaused(bool),
    /// Set the transient pause controlled by holding the Ctrl modifier.
    ///
    /// This is deliberately a policy input rather than a platform effect: the
    /// GPUI adapter observes the modifier lifecycle, while the application
    /// scheduler owns the combined paused decision.
    SetControlHeld(bool),
    /// Apply a validated cadence and clear the manual pause. A held Ctrl
    /// modifier remains an independent transient pause.
    SetInterval(TelemetryInterval),
}

/// Validated local policy used to decide whether a telemetry refresh is due.
///
/// This state belongs above platform ports: pausing the frontend scheduler
/// must not require an operating-system provider or a round-trip event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TelemetryRefreshPolicy {
    interval: TelemetryInterval,
    manual_paused: bool,
    control_held: bool,
}

impl TelemetryRefreshPolicy {
    #[must_use]
    pub const fn new(interval: TelemetryInterval) -> Self {
        Self {
            interval,
            manual_paused: false,
            control_held: false,
        }
    }

    #[must_use]
    pub const fn interval(self) -> TelemetryInterval {
        self.interval
    }

    #[must_use]
    pub const fn is_paused(self) -> bool {
        self.manual_paused || self.control_held
    }

    /// Whether the persistent/manual Ctrl+Space pause is active.
    #[must_use]
    pub const fn is_manually_paused(self) -> bool {
        self.manual_paused
    }

    /// Whether the transient hold-Ctrl pause is active.
    #[must_use]
    pub const fn is_control_held(self) -> bool {
        self.control_held
    }

    pub fn apply(&mut self, change: TelemetryRefreshPolicyChange) {
        match change {
            TelemetryRefreshPolicyChange::SetPaused(paused) => {
                self.manual_paused = paused;
            }
            TelemetryRefreshPolicyChange::SetControlHeld(held) => {
                self.control_held = held;
            }
            TelemetryRefreshPolicyChange::SetInterval(interval) => {
                self.interval = interval;
                self.manual_paused = false;
            }
        }
    }

    /// Decide from an explicit monotonic elapsed duration.
    ///
    /// `None` means no request has ever been submitted. A real zero duration
    /// means a request was just submitted and is never an absence sentinel.
    #[must_use]
    pub fn should_submit(self, elapsed_since_last: Option<Duration>) -> bool {
        if self.is_paused() {
            return false;
        }
        let Some(elapsed_since_last) = elapsed_since_last else {
            return true;
        };
        elapsed_since_last >= self.interval.duration()
    }
}

impl Default for TelemetryRefreshPolicy {
    fn default() -> Self {
        Self::new(TelemetryInterval::default())
    }
}

#[cfg(test)]
#[path = "../tests/headless/application_telemetry_refresh_policy_tests.rs"]
mod tests;
