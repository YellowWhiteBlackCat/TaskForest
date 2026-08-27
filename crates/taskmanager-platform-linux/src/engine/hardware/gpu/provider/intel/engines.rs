//! Generation-scoped Intel per-engine busy counter-to-rate assembly.
//!
//! Mirrors [`super::rc6::IntelRc6Tracker`]: a raw `busy` sysfs counter is only
//! meaningful as a delta over a measured interval, so the first sighting of an
//! engine seeds its baseline (no rate yet) and every later tick converts the
//! monotonic delta into a 0–100% utilization.
//!
//! Two `busy` node semantics are tolerated:
//!   * a **cumulative busy-time counter in nanoseconds** — the prevailing
//!     kernel accounting (i915/xe PMU, and any future `busy` sysfs node), turned
//!     into `delta_busy_ns / elapsed_ns * 100`; and
//!   * an **instantaneous 0–100 percentage** (some vendor/DKMS drivers), passed
//!     through directly when *both* the current and previous samples stay at or
//!     below 100 — a reliable discriminator because a real ns counter exceeds
//!     100 within the first microsecond of uptime.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use taskmanager_core::core::metrics::{GpuEngine, GpuEngineKind, GpuMetricField, GpuMetrics};
use taskmanager_core::{DeviceId, FailureKind};
use taskmanager_perf_ioctl::GpuEngineCounter;

use super::super::super::intel::discover_intel_pmu_layout_with_receipt;
use super::super::super::{
    EngineBusySource, GpuFieldRead, IntelEngineRead, gpu_io_failure, preferred_gpu_failure,
};
use super::xe_pmu::XePmuFallback;

/// Retry interval for an Intel PMU probe that was absent or unavailable. It
/// bounds denied-host probe cost while allowing permission/driver recovery.
pub(super) const INTEL_PMU_RETRY_INTERVAL: Duration = Duration::from_secs(5);

pub(super) fn intel_pmu_retry_at(now: Instant) -> Instant {
    now.checked_add(INTEL_PMU_RETRY_INTERVAL).unwrap_or(now)
}

pub(super) fn intel_pmu_retry_due(now: Instant, retry_at: Instant) -> bool {
    now >= retry_at
}

/// One per-engine read after rate conversion, ready to populate `GpuMetrics`.
pub(super) struct IntelEngineObservation {
    pub(super) engines: Vec<GpuEngine>,
    pub(super) failure: Option<FailureKind>,
}

impl IntelEngineObservation {
    /// Fold the breakdown into a running sample: assign the typed engine list,
    /// advertise `Engines` when any engine was read, and merge a per-field
    /// failure if the read was partial. Absent tree → empty list, no field.
    pub(super) fn apply(
        self,
        metrics: &mut GpuMetrics,
        fields: &mut Vec<GpuMetricField>,
        failures: &mut BTreeMap<GpuMetricField, FailureKind>,
    ) {
        let has_engines = !self.engines.is_empty();
        metrics.engines = self.engines;
        if has_engines {
            fields.push(GpuMetricField::Engines);
        }
        if let Some(failure) = self.failure {
            failures
                .entry(GpuMetricField::Engines)
                .and_modify(|current| {
                    *current =
                        preferred_gpu_failure(Some(*current), Some(failure)).unwrap_or(failure);
                })
                .or_insert(failure);
        }
    }
}

#[derive(Default)]
pub(super) struct IntelEngineTracker {
    previous: HashMap<String, (EngineBusySource, Instant)>,
}

impl IntelEngineTracker {
    /// Fold one per-engine read (sysfs OR i915 PMU OR xe PMU) into the rate
    /// tracker.
    ///
    /// The provider chooses the source — sysfs `busy` first, then the i915 PMU
    /// fallback, then the xe two-counter PMU fallback — and hands the resulting
    /// [`GpuFieldRead<Vec<IntelEngineRead>>`] here. Each read carries its unit
    /// ([`EngineBusySource`]) so the rate math picks the correct branch:
    /// nanoseconds-over-elapsed for sysfs/i915, active-over-total for xe.
    pub(super) fn observe(
        &mut self,
        device_id: &str,
        current: GpuFieldRead<Vec<IntelEngineRead>>,
        now: Instant,
    ) -> IntelEngineObservation {
        let mut failure = current.failure;
        let Some(samples) = current.value else {
            // Whole engine tree unreadable: drop every baseline for this device
            // so a later reappearance is a fresh seed, not a counter jump.
            self.prune_device(device_id);
            return IntelEngineObservation {
                engines: Vec::new(),
                failure,
            };
        };

        let prefix = format!("{device_id}|");
        let mut seen_this_tick: Vec<String> = Vec::with_capacity(samples.len());
        let mut engines: Vec<GpuEngine> = Vec::new();

        for IntelEngineRead { name, busy: source } in samples {
            let key = format!("{prefix}{name}");
            seen_this_tick.push(name.clone());
            // Refresh the baseline whatever happens below so the next tick
            // always deltas against the latest sample; insert returns the
            // previous pair (if any) for the rate calculation.
            let previous = self.previous.insert(key, (source, now));
            let Some((previous_source, previous_at)) = previous else {
                // First sighting: seed the baseline, no rate yet (matches RC6).
                continue;
            };
            match reduce_engine_rate(source, previous_source, previous_at, now) {
                Ok(usage_pct) => engines.push(GpuEngine {
                    kind: GpuEngineKind::from_display_name(&name),
                    name,
                    usage_pct,
                }),
                Err(engine_failure) => {
                    failure = preferred_gpu_failure(failure, Some(engine_failure));
                }
            }
        }

        // Drop baselines for engines of THIS device that did not reappear this
        // tick (hotplug / changed layout). Other devices are left untouched.
        self.previous.retain(|key, _| {
            key.strip_prefix(&prefix)
                .map(|engine| seen_this_tick.iter().any(|seen| seen == engine))
                .unwrap_or(true)
        });

        IntelEngineObservation { engines, failure }
    }

    pub(super) fn prune(&mut self, device_ids: &[DeviceId]) {
        for device_id in device_ids {
            self.prune_device(device_id.as_str());
        }
    }

    fn prune_device(&mut self, device_id: &str) {
        let prefix = format!("{device_id}|");
        self.previous.retain(|key, _| !key.starts_with(&prefix));
    }
}

/// i915 PMU fallback for the per-engine busy breakdown.
///
/// On mainline i915 the GT `engines/*/busy` sysfs node is absent, so
/// [`read_intel_gt_engines`] yields empty. This fallback opens one
/// [`GpuEngineCounter`] per engine (via the audited boundary crate — the only
/// `unsafe` in the workspace) on the first sample and at bounded retry points,
/// then reads cumulative-busy nanoseconds each tick. The result is the same
/// `ns` units as the sysfs `busy` node, so it drops straight into
/// [`IntelEngineTracker::observe`] and reuses the unchanged rate-conversion
/// path. The opened counters are stateful and held across ticks here.
///
/// Permission denial (a restrictive `perf_event_paranoid`) is surfaced
/// honestly as `FailureKind::PermissionDenied` rather than silently degrading;
/// the rest of the GPU sample (frequency, RC6) is unaffected. Absent or
/// unavailable entries are re-probed at the bounded interval owned by the
/// parent Intel provider, so a later permission/driver recovery can publish
/// real counters without a hot loop.
#[derive(Default)]
pub(super) struct IntelPmuFallback {
    /// device_id → probe result (Absent when no i915 PMU or all opens failed;
    /// Active when at least one counter is reading).
    devices: HashMap<String, IntelPmuDeviceState>,
}

enum IntelPmuDeviceState {
    /// Probed but no usable counters. Carries the most actionable failure seen
    /// during probing (`None` when the i915 PMU itself is absent — not a fault;
    /// `Some(PermissionDenied)` when `perf_event_open` was denied, etc.) and
    /// the next bounded probe time.
    Absent {
        failure: Option<FailureKind>,
        retry_at: Instant,
    },
    Active {
        counters: Vec<IntelPmuCounter>,
        /// A partial open or read failure is retained even while sibling
        /// counters continue to produce real values. It is retried at the
        /// bounded deadline instead of disappearing after the first tick.
        failure: Option<FailureKind>,
        retry_at: Option<Instant>,
    },
}

struct IntelPmuCounter {
    label: String,
    counter: GpuEngineCounter,
}

impl IntelPmuFallback {
    /// Read cumulative-busy ns for every engine of one device.
    ///
    /// Returns `None` when the i915 PMU is unavailable for this device — the
    /// caller then falls back to the (typed-empty) sysfs read and the breakdown
    /// stays honestly absent. Returns `Some(GpuFieldRead)` otherwise: `partial`
    /// if some counters read while others failed, `unavailable` (carrying e.g.
    /// `PermissionDenied`) when the PMU was present but every open failed.
    pub(super) fn sample(
        &mut self,
        device_id: &str,
        device_path: &Path,
        now: Instant,
    ) -> Option<GpuFieldRead<Vec<IntelEngineRead>>> {
        let should_probe = match self.devices.get(device_id) {
            None => true,
            Some(IntelPmuDeviceState::Absent { retry_at, .. }) => {
                intel_pmu_retry_due(now, *retry_at)
            }
            Some(IntelPmuDeviceState::Active {
                retry_at: Some(retry_at),
                ..
            }) => intel_pmu_retry_due(now, *retry_at),
            Some(IntelPmuDeviceState::Active { retry_at: None, .. }) => false,
        };
        if should_probe {
            let previous = self.devices.remove(device_id);
            let replacement = probe_device(device_path, now);
            let replacement = match (previous, replacement) {
                (
                    Some(IntelPmuDeviceState::Active { counters, .. }),
                    IntelPmuDeviceState::Absent { failure, retry_at },
                ) => IntelPmuDeviceState::Active {
                    counters,
                    failure: Some(failure.unwrap_or(FailureKind::Unsupported)),
                    retry_at: Some(retry_at),
                },
                (_, replacement) => replacement,
            };
            self.devices.insert(device_id.to_string(), replacement);
        }

        let mut transition_to_absent = None;
        let state = self.devices.get_mut(device_id)?;
        let result = match state {
            IntelPmuDeviceState::Absent { failure: None, .. } => None,
            IntelPmuDeviceState::Absent {
                failure: Some(failure),
                ..
            } => Some(GpuFieldRead::unavailable(*failure)),
            IntelPmuDeviceState::Active {
                counters,
                failure: active_failure,
                retry_at: _,
            } => {
                let mut reads = Vec::with_capacity(counters.len());
                let mut failure = *active_failure;
                for entry in counters {
                    match entry.counter.read_counter() {
                        Ok(busy) => reads.push(IntelEngineRead {
                            name: entry.label.clone(),
                            busy: EngineBusySource::NanoSeconds(busy),
                        }),
                        Err(error) => {
                            failure = preferred_gpu_failure(
                                failure,
                                Some(gpu_io_failure(&error, FailureKind::TemporarilyUnavailable)),
                            );
                        }
                    }
                }
                if reads.is_empty() {
                    let failure = failure.unwrap_or(FailureKind::Unsupported);
                    transition_to_absent = Some(failure);
                    Some(GpuFieldRead::unavailable(failure))
                } else {
                    *active_failure = failure;
                    match *active_failure {
                        Some(failure) => Some(GpuFieldRead::partial(reads, failure)),
                        None => Some(GpuFieldRead::available(reads)),
                    }
                }
            }
        };
        if transition_to_absent.is_none()
            && let Some(IntelPmuDeviceState::Active {
                failure: active_failure,
                retry_at,
                ..
            }) = self.devices.get_mut(device_id)
        {
            *retry_at = active_failure.map(|_| intel_pmu_retry_at(now));
        }
        if let Some(failure) = transition_to_absent {
            self.devices.insert(
                device_id.to_string(),
                IntelPmuDeviceState::Absent {
                    failure: Some(failure),
                    retry_at: intel_pmu_retry_at(now),
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

    /// Source selection for the engine breakdown: return `sysfs` unchanged when
    /// it already yielded engines; otherwise try the i915 PMU and fall back to
    /// the (typed-empty) sysfs read when no PMU is available. The provider calls
    /// this so its `collect` stays a bounded composition unit.
    pub(super) fn fallback_if_empty(
        &mut self,
        device_id: &str,
        device_path: &Path,
        sysfs: GpuFieldRead<Vec<IntelEngineRead>>,
        now: Instant,
    ) -> GpuFieldRead<Vec<IntelEngineRead>> {
        if sysfs
            .value
            .as_ref()
            .is_some_and(|engines| !engines.is_empty())
        {
            sysfs
        } else {
            self.sample(device_id, device_path, now).unwrap_or(sysfs)
        }
    }
}

/// Probe one device at a bounded retry point: discover the i915 PMU layout and
/// open a counter per engine. Failure-tolerant — a partially-opening device
/// keeps the engines that succeeded. The most actionable open failure is
/// retained so it can be surfaced when no counter opened at all.
fn probe_device(device_path: &Path, now: Instant) -> IntelPmuDeviceState {
    let discovery = discover_intel_pmu_layout_with_receipt(device_path);
    let discovery_failure = discovery.failure.filter(|failure| {
        // Unsupported is the normal no-PMU/no-engine result. Other failures
        // must reach the engine capability receipt unchanged.
        *failure != FailureKind::Unsupported
    });
    let Some(layout) = discovery.value.flatten() else {
        return IntelPmuDeviceState::Absent {
            failure: discovery_failure,
            retry_at: intel_pmu_retry_at(now),
        };
    };
    let mut counters = Vec::new();
    let mut open_failure = discovery_failure;
    for engine in &layout.engines {
        match GpuEngineCounter::open_enabled(layout.pmu_type, engine.config, layout.cpu) {
            Ok(counter) => counters.push(IntelPmuCounter {
                label: engine.label.clone(),
                counter,
            }),
            Err(error) => {
                // PMU OPEN failure: an EACCES here is the escalatable Intel PMU
                // denial (perf_event_paranoid), so route through the escalation-
                // aware classifier rather than the bare IO classifier. The PMU
                // still yields no numbers; only the typing becomes escalation-
                // aware (FailureKind::RequiresEscalation) when the gate says the
                // IntelPmu feature can be reached via the OS-native prompt.
                open_failure = preferred_gpu_failure(
                    open_failure,
                    Some(super::escalation::classify_intel_pmu_open_failure(&error)),
                );
            }
        }
    }
    if counters.is_empty() {
        IntelPmuDeviceState::Absent {
            failure: open_failure,
            retry_at: intel_pmu_retry_at(now),
        }
    } else {
        IntelPmuDeviceState::Active {
            retry_at: open_failure.map(|_| intel_pmu_retry_at(now)),
            failure: open_failure,
            counters,
        }
    }
}

/// Cascading per-engine busy source: sysfs first, then the i915 PMU fallback,
/// then the xe two-counter PMU fallback.
///
/// The provider calls [`IntelEngineFallback::fallback_if_empty`] once with the
/// sysfs read; this struct decides whether i915 or xe needs to step in and
/// returns the first non-empty breakdown. Selection picks the source that
/// yields engines on THIS host; an `EPERM`/`EACCES` perf_event_paranoid is
/// surfaced honestly via the classifier, never a panic.
#[derive(Default)]
pub(super) struct IntelEngineFallback {
    i915: IntelPmuFallback,
    xe: XePmuFallback,
}

impl IntelEngineFallback {
    pub(super) fn fallback_if_empty(
        &mut self,
        device_id: &str,
        device_path: &Path,
        sysfs: GpuFieldRead<Vec<IntelEngineRead>>,
        now: Instant,
    ) -> GpuFieldRead<Vec<IntelEngineRead>> {
        let after_i915 = self
            .i915
            .fallback_if_empty(device_id, device_path, sysfs, now);
        self.xe
            .fallback_if_empty(device_id, device_path, after_i915, now)
    }

    pub(super) fn prune(&mut self, device_ids: &[DeviceId]) {
        self.i915.prune(device_ids);
        self.xe.prune(device_ids);
    }
}

/// The per-interval busy delta with its unit, derived from one
/// [`EngineBusySource`] sample pair. [`engine_usage_pct`] dispatches on the arm
/// so the xe ticks path cannot accidentally divide by wall-clock (xe returns
/// TICKS, not nanoseconds).
#[derive(Debug, Clone, Copy, PartialEq)]
enum EngineBusyDelta {
    /// Cumulative busy-ns delta (sysfs `busy` + i915 PMU): rate over
    /// wall-elapsed ns.
    NanoSeconds(u64),
    /// xe two-counter ticks: `active_delta` over `total_delta`. Wall-elapsed is
    /// irrelevant.
    TickRatio { active_delta: u64, total_delta: u64 },
}

/// Reduce one engine's `(current, previous)` sample pair into a 0–100% rate.
///
/// Handles the sysfs percentage-snapshot quirk (both samples ≤ 100 → the node
/// holds an instantaneous %, not a cumulative counter) and the rollback reseed
/// for both unit paths, then delegates the surviving delta to
/// [`engine_usage_pct`]. A unit change for the same engine key across ticks
/// (e.g. a source swap from sysfs-ns to xe-ticks mid-session) is an
/// `IdentityChanged` reseed — never a mixed-unit rate.
fn reduce_engine_rate(
    current: EngineBusySource,
    previous: EngineBusySource,
    previous_at: Instant,
    now: Instant,
) -> Result<f32, FailureKind> {
    match (current, previous) {
        (EngineBusySource::NanoSeconds(cur), EngineBusySource::NanoSeconds(prev)) => {
            // Percentage-snapshot mode: a genuine ns counter exceeds 100 within
            // the first microsecond, so both-bounded samples are an inst-% read.
            if cur <= 100 && prev <= 100 {
                return Ok(cur as f32);
            }
            if cur < prev {
                // Monotonic counter went backwards → reset/wrap. Reseed (the
                // caller has already stored the new baseline).
                return Err(FailureKind::IdentityChanged);
            }
            engine_usage_pct(EngineBusyDelta::NanoSeconds(cur - prev), previous_at, now)
        }
        (
            EngineBusySource::Ticks {
                active: cur_active,
                total: cur_total,
            },
            EngineBusySource::Ticks {
                active: prev_active,
                total: prev_total,
            },
        ) => {
            // xe ticks: a rollback on EITHER counter is a reset/wrap reseed.
            if cur_active < prev_active || cur_total < prev_total {
                return Err(FailureKind::IdentityChanged);
            }
            engine_usage_pct(
                EngineBusyDelta::TickRatio {
                    active_delta: cur_active - prev_active,
                    total_delta: cur_total - prev_total,
                },
                previous_at,
                now,
            )
        }
        // Mixed units for one engine key across ticks: reseed, never fabricate.
        _ => Err(FailureKind::IdentityChanged),
    }
}

/// Convert one per-interval delta into a 0–100% rate.
///
/// Each arm is unit-correct: `NanoSeconds` divides by wall-elapsed (the sysfs
/// `busy` + i915 PMU accounting); `TickRatio` divides active by total (the xe
/// accounting) and guards `total_delta == 0` as a typed `IdentityChanged` gap
/// rather than ever dividing by zero. Both clamp to `[0.0, 100.0]` so a
/// runaway counter cannot fabricate >100%.
fn engine_usage_pct(
    delta: EngineBusyDelta,
    previous_at: Instant,
    now: Instant,
) -> Result<f32, FailureKind> {
    match delta {
        EngineBusyDelta::NanoSeconds(busy_delta) => {
            let Some(elapsed) = now.checked_duration_since(previous_at) else {
                return Err(FailureKind::TemporarilyUnavailable);
            };
            let elapsed_ns = elapsed.as_nanos();
            if elapsed_ns == 0 {
                return Err(FailureKind::TemporarilyUnavailable);
            }
            Ok((busy_delta as f32 / elapsed_ns as f32 * 100.0).clamp(0.0, 100.0))
        }
        EngineBusyDelta::TickRatio {
            active_delta,
            total_delta,
        } => {
            if total_delta == 0 {
                // No elapsed ticks in the interval: a typed gap. The tracker
                // has already reseeded to the new baseline.
                return Err(FailureKind::IdentityChanged);
            }
            Ok((active_delta as f32 / total_delta as f32 * 100.0).clamp(0.0, 100.0))
        }
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/headless/linux_engine_hardware_gpu_provider_intel_engines_tests.rs"]
mod tests;
