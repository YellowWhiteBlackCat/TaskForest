//! Linux CPU thermal-throttle counter provider.

use std::path::Path;

pub use taskmanager_core::core::sensors::ThermalThrottleSnapshot;
use taskmanager_core::{FailureKind, ScalarObservation};

#[must_use]
#[cfg(feature = "test-support")]
pub fn collect_thermal_throttle(now_ms: u64) -> ThermalThrottleSnapshot {
    collect_thermal_throttle_from(Path::new("/sys/devices/system/cpu"), now_ms)
}

#[must_use]
pub fn collect_thermal_throttle_from(root: &Path, now_ms: u64) -> ThermalThrottleSnapshot {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            let failure = io_failure(&error);
            return assemble_snapshot(
                ScalarObservation::unavailable(failure),
                ScalarObservation::unavailable(failure),
                now_ms,
            );
        }
    };
    let mut core_events = CounterAggregate::default();
    let mut package_events = CounterAggregate::default();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let failure = io_failure(&error);
                core_events.observe_failure(failure);
                package_events.observe_failure(failure);
                continue;
            }
        };
        if !entry.file_name().to_string_lossy().starts_with("cpu") {
            continue;
        }
        let throttle = entry.path().join("thermal_throttle");
        core_events.observe_sum(read_counter(throttle.join("core_throttle_count")));
        package_events.observe_max(read_counter(throttle.join("package_throttle_count")));
    }
    assemble_snapshot(
        core_events.finish(now_ms),
        package_events.finish(now_ms),
        now_ms,
    )
}

fn assemble_snapshot(
    core_events_observation: ScalarObservation<u64>,
    package_events_observation: ScalarObservation<u64>,
    now_ms: u64,
) -> ThermalThrottleSnapshot {
    ThermalThrottleSnapshot::from_observations(
        now_ms,
        core_events_observation,
        package_events_observation,
    )
}

#[derive(Debug, Default)]
struct CounterAggregate {
    value: u64,
    observed: bool,
    failure: Option<FailureKind>,
}

impl CounterAggregate {
    fn observe_failure(&mut self, failure: FailureKind) {
        self.failure = Some(match self.failure {
            Some(previous) => select_failure(previous, failure),
            None => failure,
        });
    }

    fn observe_sum(&mut self, result: Result<u64, FailureKind>) {
        self.observe_with(result, u64::saturating_add);
    }

    fn observe_max(&mut self, result: Result<u64, FailureKind>) {
        self.observe_with(result, u64::max);
    }

    fn observe_with(
        &mut self,
        result: Result<u64, FailureKind>,
        merge: impl FnOnce(u64, u64) -> u64,
    ) {
        match result {
            Ok(value) => {
                self.value = if self.observed {
                    merge(self.value, value)
                } else {
                    value
                };
                self.observed = true;
            }
            Err(failure) => self.observe_failure(failure),
        }
    }

    fn finish(self, now_ms: u64) -> ScalarObservation<u64> {
        if self.observed {
            match self
                .failure
                .filter(|failure| *failure != FailureKind::Unsupported)
            {
                Some(failure) => ScalarObservation::partial(self.value, now_ms, failure),
                None => ScalarObservation::available(self.value, now_ms),
            }
        } else {
            ScalarObservation::unavailable(self.failure.unwrap_or(FailureKind::Unsupported))
        }
    }
}

fn read_counter(path: impl AsRef<Path>) -> Result<u64, FailureKind> {
    std::fs::read_to_string(path)
        .map_err(|error| io_failure(&error))?
        .trim()
        .parse()
        .map_err(|_| FailureKind::ProviderFault)
}

fn io_failure(error: &std::io::Error) -> FailureKind {
    match error.kind() {
        std::io::ErrorKind::NotFound => FailureKind::Unsupported,
        std::io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        std::io::ErrorKind::TimedOut => FailureKind::TimedOut,
        _ => FailureKind::TemporarilyUnavailable,
    }
}

const fn select_failure(left: FailureKind, right: FailureKind) -> FailureKind {
    if failure_priority(right) > failure_priority(left) {
        right
    } else {
        left
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::ProviderFault => 7,
        FailureKind::TimedOut => 6,
        FailureKind::TemporarilyUnavailable => 5,
        FailureKind::MissingDependency => 4,
        FailureKind::IdentityChanged => 3,
        FailureKind::Rejected => 2,
        FailureKind::Unsupported => 1,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_sensors_trend_tests.rs"]
mod tests;
