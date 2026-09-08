//! Host load-average facts and the normalized cross-host comparison value.

use serde::{Deserialize, Serialize};

/// The three conventional load-average windows plus values normalized by the
/// measured physical-core count when the host exposes it. A raw load of `8.0`
/// means very different pressure on a 4-core and a 32-core host; consumers can
/// use [`Self::normalization_processors`] to see the denominator.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct SystemLoadAverage {
    pub one_minute: f32,
    pub five_minutes: f32,
    pub fifteen_minutes: f32,
    pub normalized_one_minute: f32,
    pub normalized_five_minutes: f32,
    pub normalized_fifteen_minutes: f32,
    pub logical_processors: u32,
    /// Physical-core denominator used for the normalized values. `None` means
    /// the provider could only establish the logical-processor count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_cores: Option<u32>,
}

impl SystemLoadAverage {
    /// Normalize a `/proc/loadavg` triple. Invalid, negative, or unscalable
    /// values remain absent instead of becoming a plausible zero.
    #[must_use]
    pub fn from_raw(
        one_minute: f32,
        five_minutes: f32,
        fifteen_minutes: f32,
        logical_processors: usize,
    ) -> Option<Self> {
        Self::from_raw_with_physical(
            one_minute,
            five_minutes,
            fifteen_minutes,
            logical_processors,
            None,
        )
    }

    /// Normalize a load triple using physical cores when the native provider
    /// measured that topology. A logical count remains the honest fallback
    /// for virtual machines and minimal test fixtures that expose no physical
    /// package/core identity.
    #[must_use]
    pub fn from_raw_with_physical(
        one_minute: f32,
        five_minutes: f32,
        fifteen_minutes: f32,
        logical_processors: usize,
        physical_cores: Option<usize>,
    ) -> Option<Self> {
        if logical_processors == 0
            || ![one_minute, five_minutes, fifteen_minutes]
                .into_iter()
                .all(|value| value.is_finite() && value >= 0.0)
        {
            return None;
        }
        let logical_processors = u32::try_from(logical_processors).ok()?;
        let physical_cores = physical_cores
            .filter(|cores| *cores > 0)
            .and_then(|cores| u32::try_from(cores).ok());
        let divisor = f64::from(physical_cores.unwrap_or(logical_processors));
        let normalized = [one_minute, five_minutes, fifteen_minutes]
            .into_iter()
            .map(|value| (f64::from(value) / divisor) as f32)
            .collect::<Vec<_>>();
        if normalized.iter().any(|value| !value.is_finite()) {
            return None;
        }
        Some(Self {
            one_minute,
            five_minutes,
            fifteen_minutes,
            normalized_one_minute: normalized[0],
            normalized_five_minutes: normalized[1],
            normalized_fifteen_minutes: normalized[2],
            logical_processors,
            physical_cores,
        })
    }

    /// Return the denominator used by the normalized load values.
    #[must_use]
    pub const fn normalization_processors(self) -> u32 {
        match self.physical_cores {
            Some(cores) => cores,
            None => self.logical_processors,
        }
    }

    #[must_use]
    pub const fn normalized_peak(self) -> f32 {
        self.normalized_one_minute
            .max(self.normalized_five_minutes)
            .max(self.normalized_fifteen_minutes)
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_load_tests.rs"]
mod tests;
