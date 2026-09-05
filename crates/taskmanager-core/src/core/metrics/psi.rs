//! Pressure Stall Information (PSI) domain models (Linux 4.20+, cgroup v2, cross-platform mapping).

use serde::{Deserialize, Serialize};

use crate::core::ScalarObservation;

/// Moving-average and total metrics for one stall window (e.g. `some` or `full`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct PressureWindow {
    /// 10-second exponential moving average stall percentage (0.00..=100.00).
    pub avg10: f32,
    /// 60-second exponential moving average stall percentage (0.00..=100.00).
    pub avg60: f32,
    /// 300-second (5-minute) exponential moving average stall percentage (0.00..=100.00).
    pub avg300: f32,
    /// Cumulative stall time in microseconds.
    pub total_us: u64,
}

impl PressureWindow {
    #[must_use]
    pub const fn new(avg10: f32, avg60: f32, avg300: f32, total_us: u64) -> Self {
        Self {
            avg10,
            avg60,
            avg300,
            total_us,
        }
    }
}

/// Resource stall reading distinguishing `some` (at least one non-idle task stalled)
/// from `full` (all non-idle tasks simultaneously stalled / complete thrashing).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ResourcePressure {
    /// Some tasks delayed.
    pub some: PressureWindow,
    /// All tasks delayed (None for system-level CPU pressure, which has no full definition).
    pub full: Option<PressureWindow>,
}

impl ResourcePressure {
    #[must_use]
    pub const fn some_only(some: PressureWindow) -> Self {
        Self { some, full: None }
    }

    #[must_use]
    pub const fn new(some: PressureWindow, full: PressureWindow) -> Self {
        Self {
            some,
            full: Some(full),
        }
    }
}

/// System-wide Pressure Stall Information snapshot across CPU, Memory, and I/O.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct SystemPressureSnapshot {
    pub cpu: ScalarObservation<ResourcePressure>,
    pub memory: ScalarObservation<ResourcePressure>,
    pub io: ScalarObservation<ResourcePressure>,
}

impl SystemPressureSnapshot {
    #[must_use]
    pub const fn is_available(&self) -> bool {
        self.cpu.current_value().is_some()
            || self.memory.current_value().is_some()
            || self.io.current_value().is_some()
    }
}
