//! Generation-scoped RC6 counter-to-rate assembly.

use std::collections::HashMap;
use std::time::Instant;

use taskmanager_core::{DeviceId, FailureKind};

use super::super::super::{GpuFieldRead, preferred_gpu_failure};

pub(super) struct IntelRc6Observation {
    pub(super) utilization_pct: Option<f32>,
    pub(super) idle_pct: Option<f32>,
    pub(super) failure: Option<FailureKind>,
}

#[derive(Default)]
pub(super) struct IntelRc6Tracker {
    previous: HashMap<String, (u64, Instant)>,
}

impl IntelRc6Tracker {
    pub(super) fn observe(
        &mut self,
        device_id: &str,
        current: GpuFieldRead<u64>,
        now: Instant,
    ) -> IntelRc6Observation {
        let Some(counter) = current.value else {
            self.previous.remove(device_id);
            return unavailable(current.failure.unwrap_or(FailureKind::Unsupported));
        };
        let Some((previous, previous_at)) =
            self.previous.insert(device_id.to_owned(), (counter, now))
        else {
            return unavailable(
                preferred_gpu_failure(current.failure, Some(FailureKind::TemporarilyUnavailable))
                    .unwrap_or(FailureKind::TemporarilyUnavailable),
            );
        };
        if counter < previous {
            return unavailable(FailureKind::IdentityChanged);
        }
        let Some(elapsed) = now.checked_duration_since(previous_at) else {
            return unavailable(FailureKind::TemporarilyUnavailable);
        };
        let elapsed = elapsed.as_secs_f32();
        if elapsed <= 0.0 {
            return unavailable(FailureKind::TemporarilyUnavailable);
        }
        let idle_pct =
            (((counter - previous) as f32 / (elapsed * 1_000.0)) * 100.0).clamp(0.0, 100.0);
        IntelRc6Observation {
            utilization_pct: Some(100.0 - idle_pct),
            idle_pct: Some(idle_pct),
            failure: current.failure,
        }
    }

    pub(super) fn prune(&mut self, device_ids: &[DeviceId]) {
        for device_id in device_ids {
            self.previous.remove(device_id.as_str());
        }
    }
}

fn unavailable(failure: FailureKind) -> IntelRc6Observation {
    IntelRc6Observation {
        utilization_pct: None,
        idle_pct: None,
        failure: Some(failure),
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/headless/linux_engine_hardware_gpu_provider_intel_rc6_tests.rs"]
mod tests;
