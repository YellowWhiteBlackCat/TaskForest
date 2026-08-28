//! xe PMU two-counter engine-busy fallback — pure safe Rust.
//!
//! xe (Intel Core Ultra / Xe-LPG) registers a per-device PMU at
//! `/sys/bus/event_source/devices/xe_<BDF>/` whose engine-busy counter returns
//! cumulative TICKS, not nanoseconds, and needs BOTH the active-ticks
//! (`XE_PMU_EVENT_ENGINE_ACTIVE_TICKS = 0x2`) and total-ticks
//! (`XE_PMU_EVENT_ENGINE_TOTAL_TICKS = 0x3`) counters per engine to form the
//! ratio `active_delta / total_delta`. So this fallback opens TWO
//! [`GpuEngineCounter`]s per engine via the audited boundary crate (the
//! one of the workspace's four `unsafe` trust roots — see ADR-022), reads them in lockstep
//! each tick, and emits each pair as [`EngineBusySource::Ticks`]. The rate math
//! itself (active-over-total, ignoring wall-elapsed) lives in the shared tracker
//! in [`super::engines`].
//!
//! Mirrors [`super::engines::IntelPmuFallback`]: an absent xe PMU yields `None`
//! (no fabrication); a restrictive `perf_event_paranoid` is surfaced honestly as
//! `FailureKind::PermissionDenied` via the existing IO classifier; a partial
//! open keeps the engine pair that succeeded. The rest of the GPU sample
//! (frequency, RC6) is unaffected. An absent/unavailable probe is retried at a
//! bounded interval so a later permission or driver recovery is observable.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use taskmanager_core::{DeviceId, FailureKind};
use taskmanager_perf_ioctl::GpuEngineCounter;

use super::super::super::intel::discover_xe_pmu_layout;
use super::super::super::{
    EngineBusySource, GpuFieldRead, IntelEngineRead, gpu_io_failure, preferred_gpu_failure,
};

/// xe PMU fallback for the per-engine busy breakdown.
///
/// Stateful across ticks: the per-engine counter PAIRS stay open for the life of
/// the device entry, so each tick is two `read_counter` calls per engine, not a
/// fresh `perf_event_open`.
#[derive(Default)]
pub(super) struct XePmuFallback {
    /// device_id → probe result. `Absent` when no xe PMU matches or every open
    /// failed; `Active` when at least one engine pair is reading.
    devices: HashMap<String, XePmuDeviceState>,
}

enum XePmuDeviceState {
    /// Probed but no usable counter pair. Carries the most actionable failure
    /// (`None` when the xe PMU itself is absent — not a fault;
    /// `Some(PermissionDenied)` when `perf_event_open` was denied, etc.) and
    /// the next bounded probe time.
    Absent {
        failure: Option<FailureKind>,
        retry_at: Instant,
    },
    Active {
        engines: Vec<XePmuEngineCounters>,
    },
}

struct XePmuEngineCounters {
    label: String,
    active: GpuEngineCounter,
    total: GpuEngineCounter,
}

impl XePmuFallback {
    /// Read cumulative active+total ticks for every engine pair of one device.
    ///
    /// Returns `None` when the xe PMU is unavailable for this device — the
    /// caller keeps the upstream read (sysfs or i915 PMU) and the breakdown
    /// stays honestly absent. Returns `Some(GpuFieldRead)` otherwise: an engine
    /// is emitted ONLY when BOTH its active and total counters read (an atomic
    /// pair — a lone active reading cannot form a valid ratio); a read failure
    /// on either is folded into the field failure.
    pub(super) fn sample(
        &mut self,
        device_id: &str,
        device_path: &Path,
        now: Instant,
    ) -> Option<GpuFieldRead<Vec<IntelEngineRead>>> {
        let should_probe = match self.devices.get(device_id) {
            None => true,
            Some(XePmuDeviceState::Absent { retry_at, .. }) => {
                super::engines::intel_pmu_retry_due(now, *retry_at)
            }
            Some(XePmuDeviceState::Active { .. }) => false,
        };
        if should_probe {
            self.devices
                .insert(device_id.to_string(), probe_xe_device(device_path, now));
        }

        let mut transition_to_absent = None;
        let state = self.devices.get_mut(device_id)?;
        let result = match state {
            XePmuDeviceState::Absent { failure: None, .. } => None,
            XePmuDeviceState::Absent {
                failure: Some(failure),
                ..
            } => Some(GpuFieldRead::unavailable(*failure)),
            XePmuDeviceState::Active { engines } => {
                let mut reads = Vec::with_capacity(engines.len());
                let mut failure = None;
                for entry in engines {
                    let active = entry.active.read_counter();
                    let total = entry.total.read_counter();
                    match (active, total) {
                        (Ok(active), Ok(total)) => reads.push(IntelEngineRead {
                            name: entry.label.clone(),
                            busy: EngineBusySource::Ticks { active, total },
                        }),
                        (Err(error), _) | (_, Err(error)) => {
                            failure = preferred_gpu_failure(
                                failure,
                                Some(gpu_io_failure(&error, FailureKind::TemporarilyUnavailable)),
                            );
                        }
                    }
                }
                match (reads.is_empty(), failure) {
                    (true, maybe_failure) => {
                        let failure = maybe_failure.unwrap_or(FailureKind::Unsupported);
                        transition_to_absent = Some(failure);
                        Some(GpuFieldRead::unavailable(failure))
                    }
                    (false, Some(failure)) => Some(GpuFieldRead::partial(reads, failure)),
                    (false, None) => Some(GpuFieldRead::available(reads)),
                }
            }
        };
        if let Some(failure) = transition_to_absent {
            self.devices.insert(
                device_id.to_string(),
                XePmuDeviceState::Absent {
                    failure: Some(failure),
                    retry_at: super::engines::intel_pmu_retry_at(now),
                },
            );
        }
        result
    }

    pub(super) fn prune(&mut self, device_ids: &[DeviceId]) {
        for device_id in device_ids {
            self.devices.remove(device_id.as_str());
        }
    }

    /// Cascade entry for the xe layer: return the upstream read unchanged when
    /// it already yielded engines (sysfs or i915 PMU won); otherwise try the xe
    /// PMU. Never probes when the upstream read is non-empty, so a host with a
    /// working sysfs/i915 path pays no xe `perf_event_open` cost.
    pub(super) fn fallback_if_empty(
        &mut self,
        device_id: &str,
        device_path: &Path,
        upstream: GpuFieldRead<Vec<IntelEngineRead>>,
        now: Instant,
    ) -> GpuFieldRead<Vec<IntelEngineRead>> {
        if upstream
            .value
            .as_ref()
            .is_some_and(|engines| !engines.is_empty())
        {
            upstream
        } else {
            self.sample(device_id, device_path, now).unwrap_or(upstream)
        }
    }
}

/// Probe one device at a bounded retry point: discover the xe PMU layout and
/// open an active+total counter pair per engine. Failure-tolerant — a
/// partially-opening device keeps the pairs that succeeded; the most actionable
/// open failure is retained so it can be surfaced when no pair opened at all.
fn probe_xe_device(device_path: &Path, now: Instant) -> XePmuDeviceState {
    let Some(layout) = discover_xe_pmu_layout(device_path) else {
        // No xe PMU for this device — not a failure, just unavailable.
        return XePmuDeviceState::Absent {
            failure: None,
            retry_at: super::engines::intel_pmu_retry_at(now),
        };
    };
    let mut engines = Vec::new();
    let mut open_failure = None;
    for engine in &layout.engines {
        let active =
            GpuEngineCounter::open_enabled(layout.pmu_type, engine.active_config, layout.cpu);
        let total =
            GpuEngineCounter::open_enabled(layout.pmu_type, engine.total_config, layout.cpu);
        match (active, total) {
            (Ok(active), Ok(total)) => engines.push(XePmuEngineCounters {
                label: engine.label.clone(),
                active,
                total,
            }),
            (Err(error), _) | (_, Err(error)) => {
                // PMU OPEN failure: an EACCES here is the escalatable Intel xe
                // PMU denial (perf_event_paranoid), so route through the
                // escalation-aware classifier rather than the bare IO one. The
                // PMU still yields no numbers; only the typing becomes
                // escalation-aware (FailureKind::RequiresEscalation) when the
                // gate says IntelPmu can be reached via the OS-native prompt.
                open_failure = preferred_gpu_failure(
                    open_failure,
                    Some(super::escalation::classify_intel_pmu_open_failure(&error)),
                );
            }
        }
    }
    if engines.is_empty() {
        XePmuDeviceState::Absent {
            failure: open_failure,
            retry_at: super::engines::intel_pmu_retry_at(now),
        }
    } else {
        XePmuDeviceState::Active { engines }
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/headless/linux_engine_hardware_gpu_provider_intel_xe_pmu_tests.rs"]
mod tests;
